# 07: 设计稿核对脚本漏核就退 1

**What to build:** 设计稿核对脚本：先补齐漏核的存量（票 18 那二十多个、票 24 那三个）；再加完整性检查——令牌文件里每个非颜色键，要么被某条核对核到，要么列在脚本里的显式豁免表（带一句理由），否则退 1 并说出是哪几个。脚本仍手动跑，设计稿不进门禁。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q963`：见 `../grill.md` 里那一条。
收挂单 `Q1083`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先演示今天的错：删掉一条核对，脚本照样报「一致」
  - 证据：基点 `43ef95d` 上原样跑 `python3 .scratch/gui-looks-like-the-design/check_tokens.py` → 「一致：核对了 302 项」`EXIT=0`；只在内存里删掉 `literal_pairs(".facet", …)` 那一行再跑（不动文件）→ 「一致：核对了 301 项」`EXIT=0`——`facet-gap` 从此没人比，脚本一声不吭。
  - 修好之后同一个变异 → `EXIT=1`，印「没人核的令牌 1 格：space.facet-gap」。
- [x] 补齐存量后正常那份跑通（**带保留**：两处令牌与设计稿对不上，照实红着，见 `Q1246` `Q1247`）
  - 完整性检查头一回跑，印出来的漏核是 **275 格**（数组逐格数；并成键名约一百九十个），远不止 grill 记的「票 18 那二十多个 + 票 24 那三个」：作品详情页（票 15）、待确认屏（票 18）、库体检（票 28）、差量账（票 24）、开场与弹层、外壳宽度、卡片视图、子库弹层表头、字体族、`[mix]`、`[shadow.pop]`、`[color.video]` 都有。
  - 设计稿里有对应物的全写了核对（新增 `shell_literals` / `opening_literals` / `card_literals` / `queue_literals` / `library_literals` / `sublibrary_literals` / `work_literals` 与几段颜色、字体族、宽高比、阴影偏移与扩散、弹层实际用到的宽），没有的进豁免表（15 项、19 格）。
  - 现在跑 `python3 .scratch/gui-looks-like-the-design/check_tokens.py` → `EXIT=1`，只剩两行，都是令牌与稿对不上，**没替拿主意的人挑一边改**：
    「size-title: 设计稿是 17，令牌是 15」（弹层标头 `.mhead h3`，`Q1246`）、「视频播放标 shade: 设计稿 .mtile .pv .play 是 rgba(0,0,0,.3)，令牌是 #00000050」（`Q1247`）。漏核一格都不剩。
  - 在内存里拿掉这两条再跑 → 「一致：核对了 569 项；令牌 556 格，核到 537 格、豁免 19 格」`EXIT=0`。
- [x] 拿一份故意多一个没人核的键的令牌文件跑 → 退 1、说出那个键
  - 证据：脚本加了可选参数 `[令牌文件]`。拿 `tokens.toml` 在 `[layout]` 下多插一行 `unchecked-probe = 5` 的副本（放草稿目录）跑 → `EXIT=1`，印「没人核的令牌 1 格（写一条核对，或者进 EXEMPT 并写明设计稿里为什么没有可核的对应物）：layout.unchecked-probe」（外加上面那两行对不上）。
  - 数组逐格记账：拿掉 `diff-tile-padding` 第二格的核对 → 报「space.diff-tile-padding[1]」。
- [x] 豁免表里每一项有理由
  - 证据：`EXEMPT` 15 项，每项一句「设计稿里为什么没有可核的对应物」：`version`、`font.bold-coverage`、`space.steps`、`layout.rail-collapse-below`、`card-info-height`、`card-group-height`、`fold-mark`、`root-name-max`、`health-list-max-height`、`settings-name-width`、`radio-diameter` / `radio-dot` / `radio-gap`、`title-name-share`、`shadow.pop.blur`。`blur` 是「稿里有、故意不照」的近似，理由写的是它为什么没有等值可核，并另钉一条前提：稿上 `--pop` 的模糊与扩散仍是 30 / -12（内存里把稿改成 36 → 报「…是对着 30px -12px 调的，重调」）；`spread` 不豁免，核换算 `max(0, 稿上的扩散)`。
  - 脚本自己守着这张表：理由是空白 → 「豁免表里没写理由：…」；豁免的格已从令牌文件删掉 → 「豁免表里有、令牌文件里没有：…」；既核了又豁免 → 「既核了又在豁免表里：…」，三种各做了一次内存变异，都 `EXIT=1` 并说出是哪一格。
- [x] 收尾清单（票面约定）里写明「新立令牌要么写核对、要么进豁免表」
  - 证据：写进 `docs/agents/issue-tracker.md`「收尾一张票」一节（每个 agent 收尾都照那一节勾票，`CLAUDE.md` 的「工单」直接指向它）；`crates/gui/src/tokens.toml` 文件头那段说明也补了一句，新立令牌的人当场看得见。
  - 脚本仍手动跑，没进门禁，没改 xtask 与 CI。

## 收尾（2026-09-30）

- **收挂单 `Q963`**（`../grill.md`）：照裁定做了——存量补齐、完整性检查立起来、脚本仍手动跑。票 18 那批（`dist-row-gap`、`batch-*`、`why-padding`、
  `list-*-padding`、`candidate-*`、`keys-*`、`kbd-padding`、`match-*` 等）全在 `queue_literals` 里。settled。
- **收挂单 `Q1083`**（`../grill.md`）：`size-diff-value` / `diff-tile-padding` / `diff-gap` 在 `library_literals` 里核 `.diff` / `.diff div` / `.diff b`；
  「新立令牌须进脚本」写进了收尾一节。settled。
- **怎么算「核到」**：令牌读进来后套一层记账壳，任何一条核对从里面取过哪一格（数组逐格）就算；只数键名不算。口径的岔路记在 `Q1249`。
  代码审查抓到一处「取了但没拿去与稿比」：`in_steps` 遍历间距档位，只验稿上的 gap 落不落在档里，却把五档都记成了核到（把档位改成 `[1,2,12,3,5]` 也不报）。
  改成读不记账的原始令牌，档位进豁免表。同一轮审查还让 `dialog-width` 多核一道稿里弹层实际用到的宽（`w:`、`.w||620`、`.modal`、行内 style），
  不只核说明表里那句话（令牌改成 `[520,600,720,840]` → 两处都报 620）。
  颜色也一起记账（票面只要求非颜色键）：`:root` 那两套本来就全核，多出来的是 `[color.platform] other` 与 `[color.video]`——恰好一个核出了对不上。
- **跟着改的注释**：`tokens.toml` 里三处关于脚本的说法（文件头、`[color.video]` 节头、`[shadow.pop]` 节头）改成现在的样子，行数不变（别处引的行号不挪）；`tokens.rs:147` 同一句没动（本票不动 Rust），记 `Q1248`。
- **挂单**：`Q1246`（弹层标题 17 对 15）、`Q1247`（视频播放标那层底 .3 对 0x50）、`Q1248`（`tokens.rs` 注释跟不上）、`Q1249`（「核到」的口径）。
