# 04 — 「是不是非游戏资产」提成核心库一处

**What to build:** 一份街机压缩包里捎带的 BIOS 命中了，**整包游戏的作品不再跟着它定**——
屏上那一行写的是维护者真正拥有的那个游戏。而且一个真叫 `BIOS` 的**根**，不会让它底下所有东西
都被当成非游戏资产。

收挂账 `D66`：容器里捎带的 BIOS 把整包游戏拽进同一个作品。

**Blocked by:** 无 —— 可立即开工

**Status:** done

门禁（2026-09-14，本分支工作树，`TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp cargo xtask gate -j 3 --test-threads 3 --keep-going`）：fmt、glossary、check、clippy、test（71 个测试二进制，1,936 条通过、0 条失败，315 秒）、doc，**6 条全绿**，`EXIT=0`。

⚠️ **ADR-0024 的头一个真用例。** 这个判断**仓库里已经有了**（导出那一层，票 16 加的，真库上挑出 11 个），
识别够不着它。按 ADR-0024：**把判断提到两处都够得着的那一层，两处都改成从那一处取**
——**不是把正确的那份复制过去**。

⚠️ **提上去的判断收「这份内容在库里的键」**：变体级调用方传变体键，内容级传成员键。
识别那侧本来就够得着——候选上带着「是哪个成员撞上的」与「容器内部路径」。

⚠️ **顺手剥掉根名。** 眼下那个判断把整条键按 `/` 切开逐段比对，**根名那一段也参与判断**。
核心库里有现成的函数把根名剥掉。这是缺陷不是特性。

- [x] 容器里捎带一份 BIOS 时，挑**作品**不被它定
  - 证据：`crates/core/tests/identify.rs` 的 `容器里捎带一份_bios_时作品不被它定`。fixture 里一个透明容器同时装着游戏与 `bios/disksys.rom`，两条候选都自动通过，而且 BIOS 那条排在第一。基点 `d39f027` 上红（作品是 `[BIOS] Family Computer Disk System`），改后绿（作品是 `Contra`）。改动在 `crates/core/src/identify.rs` 的 `assemble`：挑作品时跳过 `hit_non_game_asset` 的候选。候选照旧留着。
- [x] 一个**真叫 `BIOS` 的根**底下的普通游戏，照旧是游戏（根名不参与判断）
  - 证据：`crates/core/tests/pegasus.rs` 的 `真叫_bios_的根底下的游戏照旧导出成条目`。基点上红（非游戏资产计 1），改后绿（计 0，条目照旧写出）。判断本身的单元测试是 `crates/core/src/classify.rs` 的 `bios_目录里的东西是非游戏资产`（`BIOS/FC/魂斗罗.zip` 不算，`BIOS/街机/BIOS/neogeo.zip` 算）。剥根名用的是现成的 `path::relative_of_key`，也就是 `split_root(key).1`。
  - 保留：识别侧那条 `真叫_bios_的根底下的游戏照旧认得出作品` 在基点上本来就过，因为识别从前根本不判 `bios`。它挡的是日后有人忘了剥根名，证明不了这次修过什么。
- [x] 导出那一侧的行为**不变**（它本来就传变体键），并且改成调同一处判断
  - 证据：`crates/core/src/adapter/converge.rs` 的 `excluded` 改调 `classify::non_game_asset(&variant.key)`，私有的那份连同它的单元测试一起删掉。原来那五条断言原样搬进 `classify.rs`，键前补上根名。`crates/core/tests/pegasus.rs` 的 `非游戏资产与补丁不导出为前端条目` 一个字没改，照旧是绿的（BIOS 那一个仍计 1）。唯一的行为差别就是上一条说的根名。
- [x] 全仓再没有第二处判「是不是非游戏资产」的代码
  - 证据：在 `crates/`（core、cli、gui）与 `xtask/` 里不分大小写 grep `bios`，除掉测试和注释，只剩 `crates/core/src/classify.rs` 的 `non_game_asset` 一处。它从核心库公开面够得着（`pub mod classify` 加 `pub fn`），票 `gui-looks-like-the-design/08` 可以直接调。调用方有两家：`converge.rs` 传变体键；`identify.rs` 的 `hit_non_game_asset` 传成员键，容器里的内容再接上容器内部路径。审查的 Spec 轴自己 grep 了一遍，结论相同。
- [x] 两侧各留一条测试，钉住它们从此得出同一个答案
  - 证据：两对测试。根名那一对是识别侧 `真叫_bios_的根底下的游戏照旧认得出作品` 对导出侧 `真叫_bios_的根底下的游戏照旧导出成条目`，注释里互相指着。BIOS 那一对是识别侧 `容器里捎带一份_bios_时作品不被它定` 的后半截（单放的 `库/FC/bios/disksys.rom` 照旧命中、不挂作品）对导出侧 `非游戏资产与补丁不导出为前端条目`（同一种变体被挡成非游戏资产）。

收挂账 `D66` 照 grill 第 74 行的裁定，只收了「挑作品」那一侧。grill 第 30 行描述 `D66` 时并列的另一侧是「裁决锚点先认主文件」（`identify.rs` 的 `ordered()`），它没动，可能照旧挑中捎带的 BIOS 当内容锚，记在挂单 `Q682`。另一个取舍记在挂单 `Q681`：单独一份 BIOS 的变体不再挂作品。
