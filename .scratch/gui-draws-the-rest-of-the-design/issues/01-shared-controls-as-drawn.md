# 01: 共用控件照稿：小标题、勾选框、单选、两格令牌

**What to build:** 全仓共用的几枚控件长得像设计稿，其余各屏的票都建在它之上：
- 小标题 `.sec` 照稿加粗。
- 勾选框**自画稿上的 `.ckb`**（圆角方框、强调色填满），全仓约十九处 egui 默认勾选框都换；尺寸、圆角、填色取令牌。
- 单选拆出「单独一枚圆点」；待确认屏与刮削弹层那五处 egui 自带单选换成共用那一枚；合并向导的「保留」标签摆到作品名后。
- 令牌添「卡片两列门槛」「库屏左栏占比」两格，子库卡片与库屏改取它们，设计稿核对脚本核这两格。

受影响的截图基线在这一张里**一次重批**。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q770`：见 `../grill.md` 里那一条。
收挂单 `Q946`：见 `../grill.md` 里那一条。
收挂单 `Q1043`：见 `../grill.md` 里那一条。
收挂单 `Q1015`：见 `../grill.md` 里那一条。
收挂单 `Q813`：见 `../grill.md` 里那一条。
收挂单 `Q824`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写在今天 `main` 上红的测试：界面源码里不再有 egui 默认勾选框与单选的调用（或等价的行为断言：勾选框按下去画的是令牌里的填色）
  - 证据（读源码那一半）：新写 `crates/gui/tests/controls.rs` 的 `界面源码里没有egui自带的勾选框与单选`（另一条 `方法调用与控件类型都抓_共用那一枚与注释里的不算` 是扫描器自测）。
    读法与 `colors.rs` 共用，两份测试的那几样（列源码、抹注释与字面量、抹 `#[cfg(test)]`、报路径）搬进了 `crates/gui/tests/shared/source.rs`。
    在基点 `329d76a` 的界面源码上红，报 **24 处**，正好是票面预测的数：勾选框 19 处（`browse.rs` 3、`browse/merge.rs` 3、`scrape.rs` 4、`queue.rs` 2、`sublibrary.rs` 2、`table.rs` 2、
    `roots.rs` / `shaping.rs` / `stages.rs` 各 1），单选 5 处（`queue.rs:2347`、`:2372`、`:2374`，`scrape.rs:644`，`browse/merge.rs:771`）。现在两条绿。
  - 证据（行为那一半，`crates/gui/src/look.rs` 的单元测试）：`勾选框照稿_方框取令牌_勾上是强调色描边填满_里头一圈底色`（两套主题：15 点、圆角 4、1.5 点描边；没勾面板底 `line-2` 描边，
    勾上强调色描边、里头隔 2.5 点填一块强调色，期望值同时钉着稿上的 15 / 4 / 1.5 / 2.5）、`勾选框点方框点字都拨一次_按不动时不拨_字在方框右边隔一格`、
    `勾选框拨了那一帧交回changed_读屏说得出勾没勾`、`勾选框悬停时描边换成按钮悬停那一档_勾上的那一格不换`、`勾选框拿到焦点只圈方框_不圈连字的整颗`、
    `带说明的勾选一行照稿_方框对着名字那一行_名字说明左沿对齐_点说明也拨`、`单选圆点照稿_直径圆心缝取令牌_选中是强调色外圈底色缝强调色圆心`、
    `行内单选点圆点点字都交回点击_字在圆点右边隔一格_读屏说得出选没选`、`行内单选记值_点一下换成这一颗的值_只在换了那一帧交回changed`。
  - 共用的几枚（`look.rs`）：`checkbox`（稿 `.ckb`）、`checkbox_option`（稿 `.opt` 里摆一枚方框，浏览屏「显示非游戏资产」改用它）、`radio_dot`（单独一枚圆点）、
    `radio_option` / `radio_option_with`（用 `radio_dot` 拼，后者给名字后头留一格）、行内的 `radio` / `radio_value`。新令牌 `checkbox-size` `checkbox-stroke` `checkbox-gap`。
- [x] 小标题与帮助字同字号时靠字重分得开（渲染测试）
  - 证据：`look.rs` 的 `小标题与帮助字同字号_靠字重分得开_中文靠正文色深一档_带着稿上那一点字距`——同画「主库 · 3 个」：两段同字号 `size-small`；
    小标题落在粗体族、帮助字落在常规体；「3」在两段里取的是图集里不同的两块（粗体那一份字形），「主」取的是同一块（中文不打包粗体、不合成假粗体）；
    小标题正文色 `ink-2`、帮助字 `ink-3`；小标题带字距 `section-tracking × 字号`、帮助字不带。
  - 保留：**中文那一层是靠颜色分开的，不是靠字重**——字体预算只有拉丁与数字的粗体。用哪一色是拿主意的人 2026-10-01 裁的（见下面「人的关」第 1 条），字距是第 2 条。
    任务屏自己那一份小标题（`task::section`）改取同一段字（`look::section_text`），不再各写一份。
- [x] 合并向导「保留」标签紧跟作品名
  - 证据：`crates/gui/tests/merge.rs` 的 `第一步保留那枚标签紧跟在作品名后头_底下才是平台年份那一行`：标签与作品名同一行、紧跟其后（隔 `option-gap` 加标签自己那份留白），
    底下才是「平台 · 年份 · 几个变体 · 置信度」。在基点上红：标签画在 (497.6, 358.5)–(521.6, 375.5)，作品名右沿在 348.9、竖向在 349–368——隔着整句说明那么远、落在两行之间。
  - 做法：`look::radio_option_with` 在名字那一行后头摆标签；没挂标签的那几行留出标签那么高，几张卡一样高（`browse/merge.rs` 的 `step_pick`）。标签的样子（稿 `.keepb`）没动，见「人的关」第 4 条。
- [x] 改令牌里卡片两列门槛 / 库屏左栏占比，屏上跟着变（令牌变异那一族测试）
  - 证据：`crates/gui/src/sublibrary.rs` 的 `子库卡片两列门槛取令牌_改门槛同一个宽上摆几列跟着变`（1040 宽两列、1039 一列；门槛改 600 后 1040 只摆一列、改 400 后 800 摆两列）；
    `crates/gui/src/roots.rs` 的 `库屏两栏的比例取令牌_改比例左栏跟着变`（1018 宽、隔 18，左栏 555.56；比例改 `[1, 1]` 后 500）。新令牌 `sublibrary-card-min = 520`、`library-columns = [1.25, 1.0]`，
    取值与从前一样，库屏、子库屏的基线没有一张因为它们变（变的只有小标题）。
  - 保留：变异测的是**屏上调用的那一个函数**（`卡片几列` / `左栏宽`），不是拿改过的令牌把整屏画一遍——这两格与别的版式令牌一样从 `Tokens::builtin()` 那份全局只读的取，
    颜色那一族能变异是因为颜色经 `install_tokens` 装进每个 `Context`。令牌名与 grill 写的不同，记挂单 `Q1417`。
- [x] 设计稿核对脚本核到新立的令牌
  - 证据：`.scratch/gui-looks-like-the-design/check_tokens.py` 新加四条核对：`.ckb` 的宽高、圆角（取 `radius.small`）、描边宽，`.ckb.on` 的 inset，`.sec` 的字号、字重、字距，
    `.libgrid` 的两栏 fr；`layout.sublibrary-card-min` 进豁免表，理由：稿 `.devs` 是 `repeat(2,minmax(0,1fr))`，固定两列、没有断点，520 是界面自己定的。
    跑 `python3 .scratch/gui-looks-like-the-design/check_tokens.py` → 退 1，只红基点就有的那两行（size-title 17/15 即 `Q1246`、视频播放标 shade 即 `Q1247`），没多红一行、没报漏核。
    变异实测：拿一份把 `checkbox-size` 改 16、`checkbox-gap` 改 2、`section-tracking` 改 0.05、`library-columns` 改 `[1.2, 1.0]` 的令牌副本跑，多出五行
    「checkbox-size 宽 / 高、checkbox-gap、section-tracking、library-columns 左：设计稿是 …，令牌是 …」。
- [x] 每张新截图基线先由编排者对稿自审，过关的再交拿主意的人点头
  - 证据：121 张里变了 95 张（36 张一个像素没动），一次重批。清单、分组、为什么变、八条岔路口：`/Users/nicoer/dev/game-wt/logs/q8-gd-01-compare/CHANGED.md`；
    并排图（旧 | 新 | 稿）`01`–`12`，按裁定改色之后重拼的是 `01-小标题-开场-v2.png` 与 `12-特写-小标题-v2.png`；设计稿截图 `design/`（`ego-browser` 截的）、旧基线 `old/`。
    编排者 2026-10-01 对稿看过，没有打回的溢出、截断、错位；拿主意的人 2026-10-01 逐条裁了八条（见下），按裁定改完之后重批的日志
    `/Users/nicoer/dev/game-wt/logs/q8-gd-01-snap-update4.log`（`EXIT=0`，121 条全过）。
  - 保留：改色（第 1 条）与悬停（第 7 条）之后的那一版，编排者合并前再看 `-v2` 两张。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  - 证据：`/Users/nicoer/dev/game-wt/logs/q8-gd-01-gate1.log`，开头两行 `/Users/nicoer/dev/game-wt/slot2` 与 `q8/gd-01`，最后一行 `EXIT=0`
    （`cargo xtask gate --keep-going -j 3 --test-threads 3`，跑在提交之前、改完票面 `Status` 与 `numbers --write` 之后）。汇总表：
    ```
    绿  fmt        3s  cargo fmt --all --check
    绿  glossary    1s  cargo xtask glossary
    绿  check     35s  cargo check --workspace --all-targets -j 3
    绿  clippy    41s  cargo clippy --workspace --all-targets --all-features -j 3
    绿  test    1647s  cargo test --workspace --all-features --no-fail-fast -j 3 -- --test-threads=3
    绿  numbers   19s  cargo xtask numbers --check
    绿  doc       16s  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins -j 3
    7 条全绿。
    ```
    `test` 那一步 88 个测试二进制各一行 `test result: ok`，没有 `FAILED`。之前先跑了 `cargo xtask numbers --write`（`/Users/nicoer/dev/game-wt/logs/q8-gd-01-numbers.log`，`EXIT=0`）：
    README 的票数 161/205 → 162/205、测试条数 2,648 → 2,663、测试目标数 87 → 88。`glossary` 这一趟扫的是未提交的改动（分支还没有自己的提交）。
  - 票面括号里那句已经过时：票 `gate-and-tests/01` 之后本机默认盘上门禁就绿，`sync_run` 那几格如实跳过，没有「已知红」可除。

## 人的关：八条岔路口，拿主意的人 2026-10-01 裁

（问题与选项的全文在 `CHANGED.md` 末尾。）

1. **小标题的中文怎么跟帮助字分开**：裁**丙**——字色用正文色 `ink-2`，拉丁与数字照旧粗体族，带字距。稿上 `.sec` 是 `ink-3`；`ink-3` 分不开纯中文的小标题与帮助字，
   `font::strong` 自带的强调字色 `ink` 又比稿黑太多。已改（`look::section_text`）。
2. **字距**：裁**带**，维持 `section-tracking = 0.04`（稿 `.sec` 的 `letter-spacing:.04em`）。
3. **单选**：裁**全仓一种 13 点**（令牌 `radio-*`），维持；稿里那一种 16 点的 `.radio` 不另立。
4. **「保留」标签照稿改成 `.keepb`**：**托给票 `gui-draws-the-rest-of-the-design/03`**（编排者往那张票里追加一行），本票不动。
5. **刮削弹层「采法」改成两行、小字取核心库那一句**：**托给票 `gui-draws-the-rest-of-the-design/13`**（编排者追加一行），本票不动。
6. **焦点圈**：裁**在方框 / 圆点外头隔一个线宽描一圈**，维持。
7. **勾选框悬停**：裁**乙**——悬停时描边换 `ink-3`（按钮悬停那一档），已做（`look.rs` 的 `paint_checkbox`）。勾上的那一格描边照旧是强调色：换成灰的就把稿上 `.ckb.on` 那一圈强调色拆了。
   **单选照同一个说法办，结果是不另画**：它没选中那一圈本来就是 `ink-3`，悬停换成同一色看不出变化；选中的那一枚与勾上的勾选框同理不换。要有动静就得先把平时那一圈换浅，那是另一个样式决定。
8. **卡片两列门槛**：裁**照旧 520**，维持。

## 收尾（2026-10-01）

- **收挂单 `Q770`**（`../grill.md`，第五轮收口已标 settled）：小标题照稿加粗——`look::section` 走 `font::strong`（拉丁与数字粗），中文由颜色分开（正文色 `ink-2`，人的关第 1 条），
  外加稿上的字距；任务屏那一份改取同一段字。稿 `.sec` 的颜色 `ink-3` 没照，是裁定。
- **收挂单 `Q946`**（同上已标 settled）：待确认屏「手工指定」三处、刮削弹层「采法」、合并向导第二步「首选」，五处 egui 单选换成共用那一枚（`look::radio_value` / `look::radio`，圆点与 `radio_option` 同一个画法）；
  `radio_option` 拆出单独一枚 `radio_dot`。grill 里那句「check_tokens.py 考虑核版式令牌」：本票新立的版式令牌都核了（验收第 5 条），旧令牌没去补。
- **收挂单 `Q1043`**（同上已标 settled）：自画稿上的 `.ckb`，全仓 19 处 egui 勾选框都换（`look::checkbox` / `look::checkbox_option`），立三格令牌；`tests/controls.rs` 守着不许再写回 egui 那一枚。
  grill 核查里那句「挂单的前提写错了：稿 .ckb 不是圆形」照实：做的是 15 点圆角方框、不画勾。
- **收挂单 `Q1015`**（同上已标 settled）：拆 `radio_option`，「保留」标签摆到作品名后头（`look::radio_option_with`）；标签的样子托给 `03`（人的关第 4 条）。
- **收挂单 `Q813`**（同上已标 settled）：令牌添 `sublibrary-card-min = 520`，子库卡片两列门槛改取它，不再借 `dialog-width[0]`；空态卡宽照旧借 `dialog-width[1]`（稿上那张卡就是 620，与弹层第二档同宽，grill 没要动它）。
- **收挂单 `Q824`**（同上已标 settled）：令牌添 `library-columns = [1.25, 1.0]`，库屏两栏改取它，删掉 `roots.rs` 那个常量；核对脚本照稿 `.libgrid` 核两格。
- **挂单**：`Q1417`（open，收尾：两格令牌的名字没照 grill）、`Q1418`（open，拿主意的人：卡片视图那枚选择框，补 `Q1237` 没记的那一半）、
  `Q1419`（open，拿主意的人：几个台账没收的真库数，刮削弹层那句悬停字留着原样）。
- **顺手**：本票改过的文件里复述的真库数换成量级词、指台账或写真出处（`browse.rs` 四处、`queue.rs` 两处、`roots.rs`、`table.rs`、`task.rs` 两处）；
  `look.rs` 模块文档里「键盘焦点」「线宽只有一处从令牌来」两段跟着改成今天的实话。
- **测试辅助**：`look.rs` 测试里的「摊平的图形 / 画出来的框 / 画出来的圆 / 按一下 / 点了什么」收成一处；`tests/shared/mod.rs` 添 `画着的每一处`（`头一处画在哪儿` 改用它）。
