# 01: 本机门禁不靠稀疏盘：不分大小写的盘上如实跳过

**What to build:** 维护者在默认 APFS（不分大小写）的 Mac 上跑门禁，不挂任何额外的盘就绿：`sync_run` 那五条真盘测试先问临时目录分不分大小写，不分就跳过「分大小写」那一遍并印「跳过：…」，其余照跑；CI 的 ext4 上照跑全量。照测试夹具里已有的「先问、摆不出就印跳过」那个做法。
`long-jobs.md` 里「怎么处置还没裁，见 Q479」改成裁定。门禁起手核 TMPDIR（`Q947`）随之作废。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q479`：见 `../grill.md` 里那一条。——**已收**：`crates/core/tests/sync_run.rs` 的 `现场` 摆现场时就问一句 `romcat_core::fs::case_insensitive(&RealFs, 卡)`（与 `testing/target.rs:244` 同一句、同一个口径：只有 `Some(true)` 才算不分，问不出来照跑），存进 `卡不分大小写`。
四条（`计划算完之后才出现的落点占用_…`、`不注入时闸在真盘上…`、`列不开的目录底下_落点被占…`、`平台目录是个符号链接_…`）只把红的那一格——「`…/tetris.zip` 不在」——收进 `现场::旁边一个字节都没写`：不分就印「跳过：…」，分就照断；同一条里别的断言（挡下来记一条、维护者那份一个字节没动、没写成的不进清单）两面都照跑。
一条（`只差大小写的那个文件不是目录_分大小写的盘上照样建得出目录`）**整条跳过**：它整条都在验分大小写的盘——不分大小写的盘上 `gb` 这个文件与 `GB/` 这个目录并存不了，`create_dir_all` 撞上的是盘不是闸，拆不出与大小写无关的一半。
收挂单 `Q947`：见 `../grill.md` 里那一条。——**已收（作废）**：随 `Q479` 作废，门禁起手不核 `TMPDIR`，`xtask/` 一行没动；`long-jobs.md` 的裁定里写明了。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先演示今天的错：在默认 TMPDIR（不分大小写）下单跑那五条，记下红的回执
      —— 2026-09-30，这棵树、基点代码，`TMPDIR=/var/folders/y3/qbq09z4n58nc5rdn0bdf5ff80000gn/T/`（系统默认），`cargo test -p romcat-core --test sync_run -j 3 -- --test-threads 3`：
      `test result: FAILED. 21 passed; 5 failed`，`EXIT=101`。红的正是点名那五条，不多不少（行号是改前的）：
      `计划算完之后才出现的落点占用_执行这一层也挡得住`（`:891`「分大小写：挡下来就一个字节都不写」，红在第一档，第二档同一句也会红）、`不注入时闸在真盘上照样挡得住`（`:923`）、`列不开的目录底下_落点被占照样挡得住`（`:968`）、`平台目录是个符号链接_底下那份照样挡得住`（`:1283`）——这四条红在同一句「挡下来就一个字节都不写」，别的断言全过；
      `只差大小写的那个文件不是目录_分大小写的盘上照样建得出目录`（`:1249`，`[Failure { path: "GB/tetris.zip", act: Add, why: "File exists (os error 17)" }]`）。日志 `/Users/nicoer/dev/game-wt/logs/q6-gt01-before-ci.log`。
- [x] 改完后同样条件下五条不红，输出里有「跳过：…」
      —— 同样的默认 `TMPDIR`，`… --test sync_run -j 3 -- --test-threads 3 --nocapture`：`test result: ok. 26 passed; 0 failed`，`EXIT=0`，印出六行「跳过：…」：
      跳一格的四条共五行（第一条两档各一行，行尾分别注「（视图按「分大小写」折的那一趟）」「（视图按「不分大小写」折的那一趟）」），形如
      「跳过：临时目录所在的盘不分大小写，`GB/tetris.zip` 就是维护者那份，「旁边一个字节都不写」这一格只有分大小写的盘上断得了」（软链那条是 `别处/tetris.zip`）；
      整条跳过那一条一行：「跳过：临时目录所在的盘不分大小写，摆不出并存的 `gb` 文件与 `GB/` 目录」。日志 `/Users/nicoer/dev/game-wt/logs/q6-gt01-after-review-ci.log`（开头三行：本树、本分支、`TMPDIR=/var/folders/…`）。
- [x] 分大小写的临时目录下（本机稀疏盘或 CI）五条照跑、分大小写那一遍真断言
      —— `TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp`（`hdiutil` 挂的分大小写稀疏盘），同一条命令：`26 passed; 0 failed`，`EXIT=0`，「跳过」**0 行**——五条的分大小写那一格都走了断言那一支。日志 `/Users/nicoer/dev/game-wt/logs/q6-gt01-after-review-cs.log`。
      那一格咬得动（变异实测，做在审查改动之前那一版上、行号也是那一版的；改完即还原，`execute.rs` 未进提交）：把 `sync/execute.rs::place` 里 `Occupancy::Unreadable(dir) if !taken => Some(Refusal::Unreadable(dir))` 改成 `=> None`（「列不开」折回「没有」那个老毛病），`列不开的目录底下_落点被占照样挡得住` 在稀疏盘上**红**（`sync_run.rs:991` panicked），在默认 `TMPDIR` 上**绿**；同一变异下假视图那一条 `列不开的目录底下_假视图上闸照样挡得住` 在默认 `TMPDIR` 上红（`:1052`）。
      ⚠️ 这一格也说明了本机那个「绿」少验了什么：见文末「本机少验的那几格」。
- [x] `long-jobs.md` 那一句已改成裁定
      —— 「读回执」末尾那段整段换成「本机的门禁不靠分大小写的盘，那几格由 CI 验」：已裁（`Q479`）、四条跳一格一条整条跳、CI 的 ext4 上照跑全量、本机不用带 `TMPDIR`、门禁起手不核（`Q947` 作废）、本机红了就是真红、看跳过带 `--nocapture`、要在本机验就让 `TMPDIR` 指一块分大小写的卷；并指向 `Q1186`。
- [x] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据
      —— **不带 `TMPDIR` 覆盖**，日志 `/Users/nicoer/dev/game-wt/logs/q6-gt01-gate1.log`（3,362 行），开头三行 `/Users/nicoer/dev/game-wt/slot-1`、`q6/gt-01-local-gate-green-without-a-sparse-disk`、`TMPDIR=/var/folders/y3/qbq09z4n58nc5rdn0bdf5ff80000gn/T/`；末行 `EXIT=0`。汇总表：
      `绿 fmt 1s` / `绿 glossary 0s` / `绿 check 17s cargo check --workspace --all-targets -j 3` / `绿 clippy 21s` / `绿 test 1043s cargo test --workspace --all-features --no-fail-fast -j 3 -- --test-threads=3` / `绿 numbers 4s` / `绿 doc 18s`，「7 条全绿。」
      test 那一步 86 个 `test result`，合计 2,632 条通过、0 条失败、2 条 ignored（`magnitude.rs` 那两条量级测量）；`sync_run` 26 条全过。默认 `TMPDIR` 下除这五条外没有别的红——本机不挂盘，整份门禁就是绿的。
      门禁跑在审查改完之后、翻 `Status` 之前；之后只动了本票（`Status` 改 `done` 让 README 票数差一，照编排约定合并时在 `main` 上重写），不进编译，没跑第二趟。

**本机少验的那几格**（如实写，不是本票能补的）：不分大小写的盘上，`不注入时闸在真盘上…`、`列不开的目录底下_落点被占…`、`平台目录是个符号链接_…` 三条剩下的断言靠**逐字那一问**就过得去（按名字开得了维护者那份）——它们各自钉的那一格（真盘上折起来那一问、「列不开」是第三态、软链那一枝不看 `kind`）坏了，本机照绿，上面那次变异就是实证。「列不开」那一格有假视图那条在任何机器上兜着；另两格本机没人兜，只有 CI 的 ext4 验。这正是 `Q479` 裁的「CI 照跑全量」，每条测试的注释里写明了。
另，稀疏盘当 `TMPDIR` 从前顺带替 `gamelist` 的跨盘降级复制造出了跨盘，本机不带它之后那一条在哪儿都只剩跳过——挂单 `Q1186`。

审查（`/code-review`，两轴只读，审查范围与自取的 4 个文件逐个对得上）：Spec 轴报 `long-jobs.md`「同一条里别的断言照跑」与整条跳过那一条对不上、盘是执行完才问的（会沾被测代码写下的东西）；Standards 轴报同一句、四处 if/else 几乎逐字重复、`（不分大小写那一档）` 接在「只有分大小写的盘上断得了」后面读着自相矛盾，以及 `long-jobs.md` 没提跨盘那一条。都改了：摆现场时就问、收成 `现场::旁边一个字节都没写`、尾注改成「视图按「…」折的那一趟」、`long-jobs.md` 分开写四条与一条并指向 `Q1186`。
