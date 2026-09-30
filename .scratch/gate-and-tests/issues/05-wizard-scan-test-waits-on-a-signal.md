# 05: 扫描加闸，向导那条测试不赌挂钟；两处并进占位活

**What to build:** 「走完向导之后扫描……按得下停下」那一条不再靠一块三万文件的大盘赌「扫描比进主窗口慢」：给扫描那条路加一道测试用的闸（走到第 N 个条目停住等信号，与共享的「占位活」同一规矩），测试在闸上按停，彻底不等时间；那块大盘删掉。
另两处等按停各写一个 sleep 循环的（库屏测试的「占住任务台」、任务屏测试的「排一趟停在原地」）并进共享的「占位活」。生产路径不变。

⚠️ 与 `gui-draws-the-rest-of-the-design/11`（库屏工序段）同在库屏扫描那一带，派活时错开。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q833`：见 `../grill.md` 里那一条。——**已收**：库屏扫描加了测试用的读盘层（`roots::Screen::scan_through`，经 `App` / `Program::scan_through` 递进去），测试递一道「第 N 回读文件停住等信号」的闸（`tests/shared` 的 `闸` / `闸口`）；那条向导测试改用两个文件的小盘，在闸上按停，大盘删了。
收挂单 `Q864`：见 `../grill.md` 里那一条。——**已收**（已并进 `Q833`）：真扫描有了等信号的口子，就是上面那道闸。
收挂单 `Q1146`：见 `../grill.md` 里那一条。——**已收**（随 `Q833`）：压满机器连跑五趟，那条向导测试五趟都绿；同样压满时基点上它红（见下面第二格）。
收挂单 `Q741`：见 `../grill.md` 里那一条。——**已收**：`tests/roots.rs` 的「占住任务台」删了，七处改用 `占位活::排上` + `按停`；`tests/task.rs` 的「排一趟停在原地的」改建在 `占位活::照这样排上` 上。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 那条向导测试不再造大盘、不含任何时间窗口断言
  - 证据：`摆一块大盘`（30 组×1000 个 zip）删了，全仓搜不到；`走完向导之后扫描已经排在任务台上报得出进度也按得下停下` 改用两个文件的 `摆一块盘`，扫描隔着 `一道闸(1)` 读盘：等扫描走到闸上 → 按「停止」→ 放行 → 等收场。测试里没有一句「多少时间内发生」的断言；等的两处都是等信号、上限只防挂住（`闸口::等扫描走到闸上` 一分钟，见挂单 `Q1228`；`等台上那一趟收场` 是原有的 600 帧）。
  - 先演示今天的错：只把大盘换成小盘、不加闸，这条当场红（扫描 17 毫秒就「完成」了，屏上已没有「停止」）。
  - 新增一句断言：历史里记成「部分完成」（按停的，不是放行后自己扫完的）。变异实测：去掉按「停止」那一下，它当场红（「按了「停止」，那一趟却没记成「部分完成」」），复原后绿。
- [x] 并排跑整份界面测试（压满机器）连跑五趟都绿（回执为证）——**保留，见挂单 `Q1229`**
  - 跑法：romcat-gui 的 24 个测试二进制**同时起跑**、各用默认并发（8 核），连跑五趟；编排者停下另两个槽的宽跑后跑（外部 tonefit 编译偶尔也在）。日志 `/Users/nicoer/dev/game-wt/logs/q6-gt05-stress1.log`（开头两行是本树与本分支，末行 `EXIT=1`），每个二进制的输出在同名 `.log.d/` 底下。
  - 机器负载（`uptime` 一分钟均值，每 30 秒采一次）：五趟峰值 138.2 / 152.4 / 104.2 / 120.2 / 111.1。
  - **那条向导测试五趟都绿**；本票改过的三份测试五趟都绿：`program` 43 passed ×5、`roots` 82 passed ×5、`task` 14 passed ×5。120 次二进制运行里 113 次绿。
  - 红的 7 次与本票无关：`media` 每趟 6 条红（`tests/media.rs` 的 `等图` 二十秒截止之后悄悄往下走，读到 `Waiting`）；第 2 趟（峰值 152）另有 `snapshot` 2 条（子库手动例外弹层两张图）与 `work` 1 条（`work.rs:1951`）。这几条都不走本票碰过的路。
  - 基点对照：把改动暂放、回到 `43ef95d` 原样，同样压满跑一趟（`/Users/nicoer/dev/game-wt/logs/q6-gt05-stress-base.log`，峰值 124.4，末行 `EXIT=1`）：`media` 红的是**同样六条、同样行号**——早就在；**那条向导测试在基点上也红**（`program.rs:1168`「这一趟活迟迟不收场」），正是本票要治的病。
- [x] 另两处换成占位活后原测试照旧绿；全仓那两个 sleep 循环不在了
  - 证据：`cargo test -p romcat-gui --all-features --test task --test roots`：`roots` 82 passed、`task` 14 passed；压测五趟同样全绿（见上一格）。全仓搜「`loop` 里 `check()` 再 `sleep`」为零；`占住任务台` 这个名字只剩 `media.rs:520` 一句描述性注释（那里用的本来就是占位活）。
- [x] 扫描的生产路径没有新增只给测试用的分支之外的行为（闸只在测试里注入）
  - 证据：`roots::Screen` 手里那一层默认就是 `RealFs`，扫描照旧直读真盘；`Program` 记的是 `Option`，生产那条路一直是 `None`，换态时 `if let Some` 才递——那是唯一新增的分支，只给测试用（取舍见挂单 `Q1227`）。闸（`闸` / `闸口` / `一道闸`）只住在 `crates/gui/tests/shared/mod.rs`。两轴审查（Spec 轴）核过这一条：「规格原句『只在测试里用；生产路径不变』成立」。
- [x] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据
  - 日志 `/Users/nicoer/dev/game-wt/logs/q6-gt05-gate1.log`：开头两行是本树与本分支，末行 `EXIT=0`。汇总表：
    ```
    绿  fmt        4s  cargo fmt --all --check
    绿  glossary    0s  cargo xtask glossary
    绿  check     36s  cargo check --workspace -j 3
    绿  clippy    73s  cargo clippy --workspace --all-targets --all-features -j 3
    绿  test    1536s  cargo test --workspace --all-features -j 3 -- --test-threads=3
    绿  numbers    4s  cargo xtask numbers --check
    绿  doc        5s  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins -j 3
    7 条全绿。
    ```
  - `glossary` 的范围那一行：「merge base 就是 `HEAD`（43ef95d）……只看未提交的改动」——跑时改动全未提交，所以整张票的代码都扫到了。门禁之后代码没再动（压测前暂放、压测后取回，与备份补丁逐字一致）。
