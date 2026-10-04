# 20: 键盘走到卡片或表格一行上按 Enter，照稿打开作品详情页

**What to build:** 浏览屏上用键盘（Tab 走到一张卡、或表格里一行拿着焦点 / 高亮）按 **Enter**，照稿与快捷键表那一句打开**作品详情页**；今天开的是侧边详情。卡片墙与表格两处一个说法；空格照旧（选中 / 勾选，照快捷键表）。快捷键表那一行若与新行为对不上，跟着改成实话。

来源：拿主意的人 2026-10-04 在票 06 的人的关上裁（挂单 `Q1469`）：卡片与表格一起照稿改，另开这一张。

规格：`../spec.md`；稿 `../../gui-looks-like-the-design/prototype.html`（卡片墙与表格的键盘、快捷键表）。

⚠️ 05–09、19 与本票都动浏览屏 `browse.rs`，串着做。

**Blocked by:** 06

**Status:** done

- [x] 先写在今天 `main` 上红的测试（界面）：卡片墙上 Tab 走到一张卡按 Enter → 当前屏是作品详情页、开的是那一部
  证据：`crates/gui/tests/menu.rs` 的 `卡片墙上tab走到一张卡按回车_当前屏是那一部的作品详情页`——一下下按 Tab，直到读屏那一层（AccessKit）报焦点落在一张卡上；`Enter` 之后作品详情页开着、停在概览（`page().tab() == Overview`），开的是那一部（`work()` 与 `highlighted()` 都是它），没勾选。在基点 `f3ba507` 上红：`left: None / right: Some(Overview)`（`/Users/nicoer/dev/game-wt/logs/q8-gd-20-red1.log`，`EXIT=101`）。
- [x] 表格上高亮一行按 Enter → 同样打开那一部的作品详情页
  证据：`crates/gui/tests/menu.rs` 的 `表格上tab走到一行按回车_当前屏是那一部的作品详情页`——Tab 走到表格一行行首那一格（认法：先走到与那一行作品名同高的勾选框，再按一下 Tab），`Enter` 开那一部的作品详情页、停在概览、高亮挪到它；在基点上红（同样是 `None`，窄跑）。同一条后半段钉「只高亮着、没有焦点」那一路：关掉详情页、上下键挪到相邻一行，`Enter` 开的是那一行（这一路基点上就通，原有的 `回车打开作品详情页` 只看进没进详情页，这里补上「开的是那一部」）。
  实现：表格那一行与卡片墙那张卡都交出 `table::Opened { page: true }`，落在双击一行那一路（`open_work` + `open_page(Tab::Overview)`，与侧边详情「查看详情」、窗口那一层的 `open_focused` 同一处 `open_page`），没另写开页逻辑。
- [x] 空格照旧只选中 / 勾选，不跳屏；有弹层、浮层、光标在输入框里时 Enter 不接（走 `keys::allowed` 那一处）
  证据：卡片墙 `卡片墙上tab走到一张卡_空格勾选_浮层摊着时空格回车都不接`（票 `06` 那一条改名改写：空格勾中、不跳屏；右键摊着菜单时空格不去勾、回车不开详情页）；表格 `表格上一行拿着焦点时_空格不跳屏_浮层摊着时回车不接`（空格同点一下那一行、不跳屏；右键之后断言焦点照旧在那一格上（读屏节点号），菜单摊着时回车不开详情页）。键盘那一下只在 `keys::press` 一处认（指针点的 / 拿着焦点按的 `Enter`·`空格` / 门关着），门问的是 `keys::allowed`；变异检查：让那道门恒开，这两条都红。
  弹层：同一个函数的第一道门；egui 的 `Modal` 开着时底下的卡与行本就拿不住焦点（`Memory::allows_interaction`），窗口那一层照旧由 `keys::allowed(None)` 挡。光标在输入框里：焦点在框上，卡与行收不到那一下，窗口那一层同上（原有 `光标在输入框里时单键不接`、`卡片墙上有弹层有浮层光标在搜索框里时上下键不挪高亮` 钉着）。这两样没为 `Enter` 另写测试。
  **保留**：表格那一行上的空格「照旧」是点一下那一行（高亮、侧边详情），不是快捷键表那一句「勾选 / 取消勾选」——两种读法都站得住，记挂单 `Q1787`。表格那一行上的空格如今也问三道门（从前一道都不问）；右键已把高亮挪到那一行，这一层测不出差别，没有单独的测试。
- [x] 快捷键表里 Enter 那一行与行为一致
  证据：`keys::groups` 浏览那一组本就写「打开作品详情 Enter」（取共用的 `打开键`，`keys::tests::浏览那几个键取的是共用的那一份` 钉着），这张表一个字没改；如今高亮着按、Tab 走到卡或行上按都开作品详情页（上面两条测试），表上那一句说的是实话。`keys.rs` 模块文档补了一句说明。
- [x] 受影响的截图基线（若有）重批；每张新截图基线先由编排者对稿自审，过关的再交拿主意的人点头
  证据：没有一张变：`cargo test -p romcat-gui --all-features --test snapshot`，`test result: ok. 139 passed`（`/Users/nicoer/dev/game-wt/logs/q8-gd-20-snapshot1.log`，`EXIT=0`；审查之后收成 `keys::press` 那一版，门禁 `test` 那一步里截图门照样全过），不用重批，`snapshots/` 下一个文件没动。
- [x] 门禁全绿
  证据：`/Users/nicoer/dev/game-wt/logs/q8-gd-20-gate2.log`（开头两行 `/Users/nicoer/dev/game-wt/slot2`、`q8/gd-20`；`CARGO_INCREMENTAL=0 cargo xtask gate --keep-going -j 3 --test-threads 3`），末行 `EXIT=0`，汇总表「7 条全绿」：fmt、glossary（扫了 5 份 `.rs` 新写的 350 行，没撞上）、check、clippy、test（`--no-fail-fast`，990 秒）、numbers、doc。README 数：测试 2,882、票 187/217（`cargo xtask numbers --write` 写回）。头一趟 `q8-gd-20-gate1.log` 红在 fmt（测试文件没过 `cargo fmt`），当场停掉，`cargo fmt --all` 之后跑的就是这一趟。没动令牌，不用跑 `check_tokens.py`。

## Comments

### 实现者记的（2026-10-04）

- **改了什么**：`keys::press` 一处认这一下是指针点的、还是拿着焦点时按的 `Enter` / `空格`（egui 把后者也算成 `clicked`），键盘那一下问 `keys::allowed`；卡片墙那张卡与表格那一行都只问它。`Enter` 交出 `Opened { page: true }`，走双击那一路开作品详情页、停在概览。
- **表格那一行的焦点**：一行是七格拼起来的，每一格各是一个 Tab 站，只有行首那一格画焦点圈；`Enter` 落在哪一格上都算这一行（挂单 `Q1789`）。
- **卡片墙双击**照旧只开侧边详情，稿上开详情页（挂单 `Q1788`）。
- **挂单**：本票用了 `Q1787`–`Q1789`；`Q1469` 上注明本票已照做。
