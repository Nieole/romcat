# 33 — 命令行与核心库跟上 2026-09-14 定下的用词

**What to build:** 拿主意的人看界面候选图时逐个定了几处用词（词表 `CONTEXT.md` 已在 `fe17029`、`fd60a78` 两个提交里改好），界面那几屏已经照新词画；命令行提示、报告和核心库交给两边的原话还是旧词。这张票把它们对齐，让屏上、命令行、文档说同一套话。

**Blocked by:** 05、06、09、20、25、32（都已合进 `main`）。

**Status:** done

⚠️ **这张票是看图时拿主意的人定出来的**，不是规格切出来的：词表改名的理由写在 `CONTEXT.md` 对应条目里（2026-09-14）。

⚠️ **只换字，不改行为**；已公开的旗标名、库列名与协议字段不改（`CONTEXT.md`「三类不算撞 `_Avoid_`」第 2 类）——要改某个旗标名的，停下来列成岔路口问拿主意的人。

- [x] 规则组「连接」改叫「组合方式」：核心库报错、命令行帮助与提示里指这件事的地方都换掉（`cargo test -p romcat-cli --bin romcat`、`cargo test -p romcat-cli --test sublibrary --test sync`）
- [x] 打断一趟任务的那一下写「停止」（原「停下」）：命令行 Ctrl+C 之后的提示、报告里的说法（上述 CLI 测试；`cargo test -p romcat-core --lib`）
- [x] 取回 / 开跑 → 下载 / 运行：命令行对数据源与工序的提示照新词（`cargo test -p romcat-core --lib sources::tests`、上述 CLI 测试）
- [x] 设备或根的盘没插上写「未连接」（原「不在位」）：核心库给界面与命令行的原话（`cargo test -p romcat-core --test sublibrary --test sync`、上述 CLI 同步测试）
- [x] 任务历史表头「结果」、容量条图例「已选 / 清单外文件 / 容量上限」、合计行「没有重复」：命令行里有对应输出的照新词（`SelectionReport::render_text` 断言「规则之间没有重复」）
- [x] 测试断言跟着换字，门禁全绿；词表门禁不撞（`cargo fmt --all --check`、`cargo xtask glossary`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 与文档门禁均绿；完整门禁的 `test` 仅有既有 Q479 的 5 项 `sync_run` 大小写敏感 APFS 失败，和本票无关）

## Comments

- 2026-09-17：已完成 CLI 与核心库的人可见用词对齐，并完成双轴审查；审查指出的票据收尾与遗漏的 CLI/核心提示均已补齐。
