# 13: 差量预览三处：补回、撞车容量、--json

**What to build:** - 勾「补回」时先停掉台上那一趟再重排，不白跑。
- 「放不进目标」那一行撞车的几份只算一次容量，屏上写明口径。
- 撞车明细进 `--json`。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q1029`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 先写红测试（核心库）：三份撞到同一路径 → 放不进目标的容量只算一份（今天算三份）
- [ ] `romcat sublibrary preview --json` 里有撞车明细（命令行测试）
- [ ] 界面测试：差量预览跑着时勾「补回」→ 台上那一趟被停、新一趟起来
- [ ] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
