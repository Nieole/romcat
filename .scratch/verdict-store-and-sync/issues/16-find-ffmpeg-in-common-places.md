# 16: ffmpeg 多找几处常见位置

**What to build:** 核心库找 ffmpeg 时，PATH 之外按固定顺序再试几处常见位置（Homebrew 的两处、Windows 常见安装目录），不落盘。找不到时交出找过的清单，设置屏照实写。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1066`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 先写红测试（核心库）：PATH 里没有、注入的一个候选位置里有 → 找得到
- [ ] 全不中 → 交出找过的清单
- [ ] 设置屏在找不到时列出找过的位置（界面测试）
- [ ] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
