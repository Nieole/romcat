# 04: 词表扫描咬得住每一次 push

**What to build:** 词表扫描加一个「跟哪个提交比」的覆盖口（环境变量）：给了就扫它之后的改动，没给照旧比 `main` 的 merge base。CI 工作流在 push 事件上把 `github.event.before` 递进去；它是全零（新分支、强推）时照旧如实跳过。
本地合并再推的改动从此在 CI 上也被扫；本地合并前也能手动指一个提交自己验。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q543`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 先写红测试（词表扫描已有的测试）：站在 `main` 上、给一个覆盖提交 → 扫的是那之后新写的代码（今天只看未提交的）
- [ ] 覆盖口为全零 → 如实跳过、退 0、说清为什么
- [ ] 没给覆盖口 → 行为与今天一样
- [ ] CI 工作流在 push 事件上递了上一版（看工作流文件）
- [ ] `long-jobs.md` 那句「推到 main 的那一趟一行都不扫」改成实话
