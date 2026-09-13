# 01 — 颜色与尺寸只有一份来源

**What to build:** 维护者打开界面，看到的配色、圆角与字号与设计稿一致；日后要改一个颜色，
只改一份数据文件——设计稿与程序两边偏离时，门禁当场红。

**Blocked by:** 无 —— 可立即开工

**Status:** ready-for-agent

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
    `ColorImage` 解码；做完 14 处命中，非测试代码只剩 `tokens.rs:271`（文档）、`:282`
    （`Color32::from_hex`，解析器本身）与 `media.rs:491`（同一处解码），其余 11 处都在两个测试模块里
    ——**写死的颜色 0 处**。
  - 保留：没有测试守着「没有第二处」（挂单 `Q486`）；阴影照 egui 默认，不在令牌里（挂单 `Q485`）。
- [x] 一条测试拿令牌**逐项**比对两套 Visuals；**变异实测**：改令牌里任意一个颜色，这条当场红
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
- [ ] 门禁全绿（`--all-features`）
