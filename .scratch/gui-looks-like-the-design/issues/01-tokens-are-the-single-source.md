# 01 — 颜色与尺寸只有一份来源

**What to build:** 维护者打开界面，看到的配色、圆角与字号与设计稿一致；日后要改一个颜色，
只改一份数据文件——设计稿与程序两边偏离时，门禁当场红。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **数据不是代码**：这份令牌与 `platforms.toml` / `profiles.toml` / `priorities.toml`
同一条纪律，编进二进制，不在运行时读盘。

- [x] 两套主题的颜色、平台色、字号、间距、圆角、版式尺寸落在 `romcat-gui` 里的一份 TOML 数据文件里
  - 证据：`crates/gui/src/tokens.toml`（`git mv` 自设计稿目录），`include_str!` 编进二进制
    （`crates/gui/src/tokens.rs` 的 `BUILTIN`），运行时不读盘。读得严，`tokens::tests` 7 条：
    `内置令牌读得出来`、`少一项当场报错并说出是哪一项`、`多一项当场报错并说出是哪一项`、
    `颜色只认六位与八位的十六进制`、`平台色少了兜底那一格当场报错`、`版本对不上当场报错`、
    `平台色的键都是平台清单里的平台`。新添一节 `[color.video]`（视频播放标两色，挂单 `Q484`）。
- [x] 亮暗两套 Visuals 从它取值；界面里**没有第二处写死的颜色**（置信度四档、平台色、面板底色、描边全收进那一处）
  - 证据：`look::install` 两套主题都走 `visuals(tokens, theme)` 与 `text_styles(tokens)`；置信度四档
    `tier_color` 取令牌 `hi` / `mid` / `lo` / `none`，面板底色 `panel_fill ← win`、`window_fill ← panel`，
    描边（各档 `bg_stroke`、`window_stroke`）全取令牌；视频播放标直接取令牌 `color.video`。平台色在令牌
    `color.platform` 里（按平台名问 `Platforms::of`，表外落到 `other`）——基点上界面没有一处画平台色，
    本票也没加画它的地方。
  - 量法 `grep -rnE 'Color32::|Rgba|from_rgb|from_gray' crates/gui/src`：基点 `a14db7a` 上 3 处命中，
    其中 **2 处是写死的颜色**（`media.rs:368` `from_black_alpha(80)`、`:374` `WHITE`），1 处是
    `ColorImage` 解码；做完（`316f72f`）14 处命中，非测试代码只剩 `tokens.rs:259`（文档）、`:270`
    （`Color32::from_hex`，解析器本身）与 `media.rs:492`（同一处解码），其余 11 处都在两个测试模块里
    ——**写死的颜色 0 处**。
  - 保留：没有测试守着「没有第二处」（挂单 `Q486`）；阴影照 egui 默认，不在令牌里（挂单 `Q485`）。
- [x] 一条测试拿令牌**逐项**比对两套 Visuals；**变异实测**：装配里任一处不跟着令牌走、或令牌里任一颜色没人读，当场红（原措辞「改令牌里任意一个颜色」字面走不通，见 `Q480`）
  - 证据：`look::tests::暗色主题的颜色逐项取自令牌` / `亮色主题的颜色逐项取自令牌`——走 `install` 装好之后，
    `Visuals` 整个拆开不写 `..`（egui 升版多一个字段就编不过），36 格颜色 + 7 格圆角，外加置信度四档；
    `两套主题的字号取自令牌` 比八档字号。
  - 变异写成测试：`改暗色令牌里任意一个颜色界面跟着变` / `改亮色令牌里任意一个颜色界面跟着变`，每套主题
    25 个颜色逐个改，**拿改过的令牌走一遍整条装配**：装出来的对改过的令牌必须零偏离（写死一格——哪怕与
    令牌同值——就报「接线断了」），对没改的令牌必须报出那个键；没人读的恰好是界面还没用上的 8 个键。
    平台色与播放标直接读令牌、没有映射可接错，不进变异。手工实测
    （`/Users/nicoer/dev/game-wt/logs/slot-2-gl01-mutate2.log`）：`install` 把 `faint_bg_color` 接错成
    `panel` → 4 条红；`panel_fill` 写死成**与令牌同值**的颜色 → 逐项比对照绿，两条变异测试红并指出 `win`。
  - 保留：字面意义上「手改 `tokens.toml` 里一个颜色」这条测试不红——单一来源下界面与比对两边跟着走，
    红的是 `check_tokens.py`（实测暗色 `accent` 改一位：`look` 7 条全绿、脚本退出 1）。挂单 `Q480`。
- [x] 设计稿目录那份检查脚本改成读**同一份**文件并跑通；设计稿目录里不再留第二份令牌
  - 证据：`check_tokens.py` 改读 `crates/gui/src/tokens.toml`（按脚本自己的位置找仓库根），在
    `/Users/nicoer/dev/game-wt/logs` 下跑：`一致：核对了 65 项`、退出 0；设计稿目录只剩
    `before check_tokens.py issues prototype.html spec.md`。改令牌里暗色 `accent` 一位：
    `dark: --accent 设计稿是 #8190F6，令牌是 #8190F7`、退出 1。规格开头「判据」与设计稿说明区两处
    令牌路径跟着改了。
  - 保留：脚本不在门禁里，设计稿对令牌这一侧仍靠人跑（挂单 `Q481`）。
- [x] 门禁全绿（`--all-features`）
  - 证据：在实现提交 `316f72f` 上跑 `TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp cargo xtask gate -j 3
    --test-threads 3 --keep-going`，日志 `/Users/nicoer/dev/game-wt/logs/slot-2-gl01-gate.log`：
    fmt 1s、check 26s、clippy 41s、test 403s、doc 7s，「5 条全绿。」`EXIT=0`。test 那一步 70 份
    `test result: ok`，合计 1,839 passed、0 failed、0 ignored。日志里那几行 `unresolved link to NoSuchItem`
    出自 `xtask/tests/gate.rs` 故意造红的丢弃 crate，不是本仓库的文档告警。

## 挂单裁决（第五轮收口，2026-09-23）

从 `.scratch/PARKING-LOT.md` 迁来（`/settle`，范围 `Q449`–`Q1185`）。每条顶上多一行**裁决**：落哪一档、去了哪儿。
编号 `Qn` 留着占号、不复用。

### Q480 — 「改令牌里任意一个颜色，这条当场红」落成测试里的逐键变异，而不是手改令牌文件

- **裁决（第五轮收口，2026-09-23）：** **记** —— 做法对；只剩验收措辞写错。文档就地改正：.scratch/gui-looks-like-the-design/issues/01-tokens-are-the-single-source.md

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 两份东西矛盾
- **在哪：** 票 01 验收第 3 条（与规格「Testing Decisions」参数层那一条）对同一张票的硬约束「亮暗两套 `Visuals` 从令牌取值」；`crates/gui/src/look.rs` 测试 `改暗色令牌里任意一个颜色界面跟着变` / `改亮色令牌里任意一个颜色界面跟着变` 与辅助函数 `变异`
- **为什么没停线：** 两句都在这张票里；推翻是换一种测试写法，不动 `install` 与令牌文件
- **这张票实际做了什么：** 字面照做走不通——`Visuals` 从令牌取值之后，手改 `tokens.toml` 里一个颜色，界面与比对两边一起跟着变，永远不红（手工实测：暗色 `accent` 改一位，`look` 7 条全绿，红的是 `check_tokens.py`）。于是把「改令牌」搬进测试：每套主题 25 个颜色逐个改，**拿改过的令牌走一遍整条装配**（`install_tokens`，四档走 `tier_color_in`）——装出来的对改过的令牌必须零偏离（哪一格写死了颜色，哪怕与令牌同值，就报「接线断了」），对没改的令牌必须报出那个键（报不出算「没人读」，没人读的必须恰好是界面还没用上的 8 个键 `NOT_YET_USED`）。平台色与播放标直接读令牌、没有映射可接错，不进变异。手工实测（`/Users/nicoer/dev/game-wt/logs/slot-2-gl01-mutate2.log`）：`install` 把 `faint_bg_color` 接错成 `panel` → 4 条红；`panel_fill` 写死成**与令牌同值**的颜色 → 逐项比对照绿，两条变异测试红并指出 `win`。（代码审查指出初版只拿改过的令牌去比「内置令牌装出来的 `Visuals`」，抓不到写死成同值的一格；已改成现在这样）
- **没走的那条：** 测试里另存一份期望颜色的快照，手改令牌文件就当场红
- **建议留：** 现在这条，并把验收措辞改成「装配里任一处不跟着令牌走、或令牌里任一颜色没人读，当场红」。快照那条等于每改一个颜色要改两份，正是验收第 1 条要消灭的事
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q481 — 设计稿那一侧的核对没进门禁

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 两份东西矛盾
- **在哪：** 票 01 的 What to build「设计稿与程序两边偏离时，门禁当场红」，对验收第 4 条「检查脚本改成读同一份文件并跑通」与规格「设计稿那一侧由 `check_tokens.py` 比对」；`.scratch/gui-looks-like-the-design/check_tokens.py`、`xtask/src/gate.rs`
- **为什么没停线：** 进门禁是在 `xtask` 或界面测试里加一步，不拆本票落地的东西
- **这张票实际做了什么：** 脚本改读 `crates/gui/src/tokens.toml`（在哪个目录下跑都行），手跑 65 项一致；门禁五步没动。界面对令牌那一侧由 `look` 的测试进门禁，设计稿对令牌这一侧仍靠人跑脚本。**说白了：用户故事 99 与票的 What to build 那句「两边偏离时门禁当场红」眼下只对「界面↔令牌」成立**——改令牌里一个颜色（或只改 `prototype.html` 的一个 CSS 变量），门禁里一条都不红，红的只有手跑的脚本（`Q480` 的手工实测）
- **没走的那条：** 让门禁也跑它：`romcat-gui` 里一条测试起 `python3` 跑脚本，或把脚本移植成读 `prototype.html` 的 Rust 测试
- **建议留：** 现在这条。`.scratch/` 是一轮 effort 的材料，收口时可能归档或改稿，门禁依赖它会在那天红得莫名其妙；令牌是权威，设计稿改版时顺手跑一遍脚本即可
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q482 — 选中与焦点都取 `accent`，选中的字没用 `accent-ink`

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 裁定：维持现状：选中与焦点都取 `accent`（2026-09-23 拿主意的人）。

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 两份东西矛盾
- **在哪：** `crates/gui/src/look.rs` 的 `visuals`（`selection.stroke`、`widgets.active.bg_stroke`、`text_cursor`）；egui 0.36 的 `TextEdit` 拿到焦点时描的是 `selection.stroke`（`widgets/text_edit/builder.rs`），可选标签选中时的字也是它；设计稿焦点 `:focus-visible` 是 `--accent`，选中导航的字是 `--accent-ink`
- **为什么没停线：** 改一行映射、改测试表里一格
- **这张票实际做了什么：** `selection.stroke` 与焦点描边都取 `accent`（令牌注释「强调色：主按钮、选中、焦点」），`accent-soft` 是选中底，`accent-ink` 只进了链接色。按钮、文本框、自己画底色的行拿到焦点仍是同一个颜色（`tests/layout.rs` 三条焦点测试照绿）
- **没走的那条：** `selection.stroke` 取 `accent-ink`，焦点跟着变成 `accent-ink`（egui 把两件事绑在同一个槽位上，拆不开）
- **建议留：** 现在这条。两个颜色同一色相、只差一档明度；`accent` 当选中字在亮色 `accent-soft` 上对比度约 5.5，读得出来。左栏导航照稿重排时若要 `accent-ink` 的选中字，在那一处给色即可
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q483 — 正文取 `ink-2`、强调字取 `ink`，不是设计稿正文的 `ink`

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 裁定：维持现状：正文 `ink-2`、强调字 `ink`（2026-09-23 拿主意的人）。

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 规格没说
- **在哪：** `crates/gui/src/look.rs` 的 `visuals`（`widgets.noninteractive.fg_stroke` 与其余几档的 `fg_stroke`）；设计稿 `body{color:var(--ink)}`
- **为什么没停线：** 一格映射加测试表里一格
- **这张票实际做了什么：** `ui.label` 的正文取 `ink-2`；`strong_text_color`（`widgets.active.fg_stroke`，`font::strong` 带的就是它，见 `Q463`）与按钮字取 `ink`
- **没走的那条：** 正文也取 `ink`，照设计稿
- **建议留：** 现在这条，到各屏照稿重排时再翻。中文不打包粗体，眼下各屏的小标题就是 `font::strong(中文)`——正文与强调字都给 `ink`，一句纯中文的小标题与正文在屏上一模一样，今天还分得开的那一层就没了；设计稿的 `.note` 正是「正文 `ink-2`、`b` 取 `ink`」
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q484 — 视频播放标两色新开 `[color.video]` 保住原样，没照第四稿改

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 裁定：维持现状：播放标留半透明黑底（2026-09-23 拿主意的人）。

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 两份东西矛盾
- **在哪：** `crates/gui/src/media.rs` 的 `cell`（播放标）、`crates/gui/src/tokens.toml` 的 `[color.video]`；设计稿第四稿 `.thumb.vid::after` 是一个 `--ink` 三角、不透明度 .75、没有底，而 `media.rs` 原注释说照原型画的是「一层半透明黑底加一个 ▶」
- **为什么没停线：** 两格令牌加一处取值；翻过去是改那几行画法
- **这张票实际做了什么：** 令牌新增 `[color.video] shade = "#00000050"`、`mark = "#FFFFFF"`，与原来写死的 `from_black_alpha(80)`、`WHITE` 逐位相同（`tokens.rs` 的测试断言了这一点），`media.rs` 直接取 `Tokens::builtin().color.video`；设计稿里没有这两格的 CSS 变量，`check_tokens.py` 不核
- **没走的那条：** 照第四稿：▶ 取当前主题的 `ink` 乘 .75、去掉那层底，令牌里不添新节
- **建议留：** 现在这条。本票硬约束是「换色只换取值来源」；而第四稿那个三角画在占位图上，真界面里它压在一帧真实视频上，亮色主题下深色三角叠深色画面未必看得见——留不留底，由重排媒体格的那一屏对着真图定
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q485 — 阴影不在令牌里，照 egui 默认

- **裁决（第五轮收口，2026-09-23）：** **记** —— 票 04 已裁进令牌并落地。已落地：票 gui-looks-like-the-design/04

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 规格没说
- **在哪：** `crates/gui/src/look.rs` 的 `visuals`（`window_shadow`、`popup_shadow` 没动）与测试里 `颜色_偏离` 拆 `Visuals` 时写明不比的那两格；设计稿 `--shadow` / `--pop`
- **为什么没停线：** 令牌加一节、映射加两行
- **这张票实际做了什么：** 两套主题的阴影颜色与形状照 egui 默认（暗色黑 96/255、亮色黑 25/255），比对测试里逐格写明为什么不比
- **没走的那条：** 令牌加阴影颜色（取设计稿 `--pop` 的 `rgba(16,20,30,.45)` 之类）并接进 `Shadow::color`
- **建议留：** 现在这条，交给弹层票 04 一并定：设计稿的阴影是 CSS box-shadow（负扩散、两层叠），egui 的 `Shadow` 画不出一样的形状，颜色该跟形状一起挑。已往票 04 正文追加「收挂单 `Q485`」
- **谁来裁：** 收尾
- **状态：** settled
- **裁决（票 `gui-looks-like-the-design/04` 收尾）：** 进令牌。颜色 `pop-color`：两套主题都是设计稿 `--pop` 里那一色 `rgba(16,20,30,.45)`（设计稿暗色没有另写 `--pop`，照亮色那一份）；设计稿三处主题块各加 `--pop-color`、`--pop` 改成引它，`check_tokens.py` 照旧核得上。形状另起 `[shadow.pop]` 一节（`offset` / `blur` / `spread`，往后设计稿 `--shadow` 那一种在 `[shadow]` 底下另添一格）：偏移 `[0, 10]`、模糊 24、扩散 0——egui 的 `Shadow::spread` 不收负数，于是模糊从设计稿的 30 收窄一截，让阴影照旧主要落在下沿；第二层 `0 0 0 1px line-2` 就是窗口描边。`look::visuals` 把 `window_shadow`、`popup_shadow` 接上，比对测试逐格比颜色与形状（不再写「不比」）。像不像设计稿由票 05 的截图门说了算。

### Q486 — 「界面里没有第二处写死的颜色」没有测试守着

- **裁决（第五轮收口，2026-09-23）：** **活** —— 无家可归（全仓没有一张开着的票占这片地），进 `/grill-with-docs`，束「门禁、测试与约定」。

- **来自：** 票 `gui-looks-like-the-design/01`
- **类别：** 规格没说
- **在哪：** 票 01 验收第 2 条；`crates/gui/src/`
- **为什么没停线：** 加一条测试是加一个函数，不动已落地的东西
- **这张票实际做了什么：** 验收第 2 条的证据是一次 grep：基点 `a14db7a` 上 `media.rs` 两处写死的颜色（另一处命中是 `ColorImage` 解码，不是颜色），做完之后非测试代码里零处；没加守它的测试——约定的接缝是 `install` 与令牌数据，扫源码不在接缝上
- **没走的那条：** `look` 里一条测试逐个读 `crates/gui/src/*.rs`，剔掉测试模块与令牌解析器，见一处 `Color32::` 字面量就红
- **建议留：** 没走的那条。票 05–31 每张都要重写一屏，顺手写一个 `Color32::from_rgb` 是最自然的回退，而截图门（票 05）只在像素变了时红、说不出是哪一行；扫源码便宜，红的时候直接指到文件与行
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）