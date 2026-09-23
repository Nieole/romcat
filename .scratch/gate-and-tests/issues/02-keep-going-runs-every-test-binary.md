# 02: `--keep-going` 时 test 步看全；check 编全部目标

**What to build:** - 门禁带 `--keep-going` 时，test 那一步加 `--no-fail-fast`，一趟看到所有红的测试二进制；默认仍是第一处红就停。`long-jobs.md` 里「手动另跑一遍补」那段删掉。
- check 那一步加 `--all-targets`，「编测试但不开 demo 特性」那一格从此有人编（翻 `Q208` 的「不补」）。check 仍排在 clippy 之前。

⚠️ 与 03 都改门禁那一个文件，串着做。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q510`：见 `../grill.md` 里那一条。
收挂单 `Q1082`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 先写红测试（门禁自己那份测试）：`--keep-going` 时 test 步的参数里有 `--no-fail-fast`（今天没有）
- [ ] 不带 `--keep-going` 时没有 `--no-fail-fast`
- [ ] check 步的参数里有 `--all-targets`
- [ ] `long-jobs.md` 那段手动补跑的说明已删
- [ ] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据；记下 check 步前后耗时
