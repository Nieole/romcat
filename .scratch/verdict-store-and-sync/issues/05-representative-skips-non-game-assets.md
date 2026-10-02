# 05: 代表那一份先跳过 BIOS

**What to build:** 识别挑「代表这个变体的那一份」（裁决的内容锚就钉在它上）时，先跳过**非游戏资产**：一个变体里捎带了 BIOS，裁决钉在游戏本身那一份上，不再管到所有带同一份 BIOS 的变体。
已有裁决不迁（真库 0 条）。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q682`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

改动只在一处判断：`crates/core/src/identify.rs` 的 `ordered()`（`representative` 背后那一句）排之前先跳过非游戏资产，问的是 `classify::non_game_asset`，键怎么凑与挑作品那一侧（`hit_non_game_asset`）收成同一个 `non_game_asset_at`。消费者 `content_print` / `content_prints` / `find_verdict`，以及收藏、合集、队列、合并、刮削那几处调它的，一个字没改。没写迁移，沉淀库一行没动；主库只读（测试全在临时目录里摆字节）。

- [x] 先写红测试：一个变体里有一份 BIOS 与一份更小的游戏主文件 → 落一条裁决 → 另一个只带同一份 BIOS 的变体不被这条裁决管到（今天会）
  - 证据：`crates/core/tests/identify.rs` 的 `容器里捎带一份_bios_时裁决钉在游戏那一份上_不管到别的带同一份_bios_的变体`。`魂斗罗 带 BIOS.zip` 里游戏 4,096 字节、`bios/disksys.rom` 8,192 字节；照队列那条路（`identify::content_print`）钉一条「这是《魂斗罗》」的裁决、重跑识别。「另一个只带同一份 BIOS 的变体」两种读法都测了：`别的游戏 带 BIOS.zip`（别的游戏加同一份 BIOS）与单放的 `FC/bios/disksys.rom`，都不再挂到《魂斗罗》下；反过来，只装着同一份游戏的 `魂斗罗.zip` 归这条裁决管——裁决钉的确实是游戏那串字节。基点 `d959166` 上红（`别的游戏 带 BIOS.zip` 的作品是 `Some("魂斗罗")`），改后绿。
  - 另两条钉住这一刀切在哪（挂单 `Q1587`，待拿主意的人裁）：`一整个变体只有_bios_时代表它的照旧是那份_bios`（单放的 BIOS 与只装着 `bios/…` 的包，代表照旧是那份 BIOS——基点上本来就绿，挡的是「非游戏资产一律不当代表」那种切过头的改法；只做第一刀时它红过）；`游戏那一份拿不到判据时_代表不退到捎带的_bios_上`（RAR5 里游戏只记 BLAKE2sp、没记 CRC-32，`content_print` 答 `None`；基点上红，答的是 BIOS 那一份）。
- [x] 没有非游戏资产的变体，代表那一份照旧（主文件优先、同为主文件取大的）
  - 证据：`crates/core/tests/identify.rs` 的 `没有非游戏资产的变体_代表那一份照旧是主文件优先_同为主文件取大的`：一个包里两份取大的（`big.nes`），`cue` 加更大的 `bin` 取主文件 `cue`。原有的 `crates/core/tests/triage.rs` 的 `内容锚只管代表成员_附属成员上的裁决不盖住整个变体` 照旧绿。
  - 保留：这条在基点上本来就绿，它挡的是日后改坏，证明不了这次修过什么。基点上怎么跑的：把 `crates/core/src/identify.rs` 换回 `d959166` 那一份、测试文件用改后的，分两趟——头一趟（RAR5 那条还没写）跑名字里带 `bios` 的四条与这一条，「4 过 1 红」，红的是上面第一条；后一趟单跑 RAR5 那条，红（代表是 `bios/disksys.rom`）。跑完换回改后的那一份。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  - 证据（2026-10-02，本分支工作树，提交之前）：`CARGO_INCREMENTAL=0 cargo xtask gate --keep-going -j 3 --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-05-gate1.log`，开头两行是 `/Users/nicoer/dev/game-wt/slot3` 与 `q8/vs-05`，末行 `EXIT=0`。汇总表 fmt、glossary（扫了 `crates/` 下 3 份 `.rs` 新写的 247 行，没跳过）、check、clippy、test（732 秒，带 `--no-fail-fast`，2,769 条通过、0 条失败、2 条 `#[ignore]` 的量级测量）、numbers、doc，**7 条全绿**。`sync_run` 一条没红：括号里「五条已知红除外」那句已经过时，本机默认盘上门禁就是绿的。
  - `numbers` 先在本分支 `cargo xtask numbers --write` 写回过：票数 176/210 → 177/210，测试条数 2,767 → 2,771。
- [x] 收挂单 `Q682`：照 grill 的裁定「直接改、不迁」做完，`../grill.md` 那一条已标 settled。
  - 保留：grill 只数了裁决（真库 0 条），钉在同一个内容锚上的收藏与合集成员没数，本票同样不迁——记在挂单 `Q1588`，待拿主意的人裁。
