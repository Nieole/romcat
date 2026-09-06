# 03 — 中文索引重建不再白解、不再解两遍、不再哑着跑

**What to build:** `romcat zh sync` 撞上一份等着重建的中文索引时，眼下会先就地重建（解压整份 435 MB 原件，几分钟），**紧接着**发现远端有新版、下载再解一遍——头一遍的产物当场被覆盖。同一条路上还有两处：`--full` 会让原件解两遍；就地重建那几分钟**一个字都不打**，用户看见的是一个像死掉了的进程，而且按不停。

这一票把这条路收干净：**先取到远端版本、确认指纹没变，才就地重建**；`--full` 不重复解；重建期间报进度、按得停，**停下之后手上那份旧索引仍然可用**。

**Blocked by:** 无 —— 可立即开工

**Status:** done

- [x] 远端有新版时**不做**那一趟就地重建，直接走取新版那条路 ——
  `run_zh_sync` 里那句 `heal_zh_store` 拿掉了：取数自己那条路先取 `aux/latest.json`，
  再决定读哪份原件（`zh::sync::sync` 本来就把「等着重建的那一份」排除在「指纹没变就
  跳过」之外）。证据：`romcat-core` 新测试
  `zh::sync::远端有新版时不先拿旧原件白重建一遍` —— 一份等着重建的索引 + 远端换了新版，
  **「开读」那一下只报了 1 次**（`解了几遍 == 1`），库里落的是新那一版的条目（`id == 7`）、
  指纹也换成了新那一版。
- [x] 远端没有新版（指纹一样）时才就地重建 ——
  `zh::sync::等着重建的那一份不许因为指纹没变而整件跳过`（原有，这一票补了一句
  `assert!(!outcome.downloaded)`）：`CannedFetcher` 只备了 `latest.json`，真去下会当场
  失败；结果是从本机那份原件重建出 1 条。
- [x] `--full` 撞上等着重建的索引时，原件只解一遍 ——
  新测试 `zh::sync::full_撞上等着重建的索引时原件也只解一遍`：`解了几遍 == 1`、
  `!skipped`（`--full` 本来就不走跳过那条）、`!downloaded`。
  **带保留**：核心库这一侧钉死的是「取数这条路本身只读一遍」；命令行那一侧从前的
  双解来自 `heal_zh_store` + `sync` 两次调用，修法是删掉前者，而仓库里没有跑
  `romcat zh sync` 的端到端测试（那要发真请求，闸门也不许），所以那一半靠代码走查
  ——`run_zh_sync` 里现在只剩一处 `zh::sync::sync`，`grep heal_zh_store` 在这个函数里
  一处都没有。
- [x] 重建期间报得出进度（读到第几条、还剩多少）——
  `zh::sync::Progress { records, games, bytes, total }`，由 `zh::sync::Context` 的回调
  往外报（取数与重建走的是同一遍流 `read_dump`，所以两条路一并有了）。
  新测试 `zh::sync::读原件时报得出读到第几条与还剩多少`：**开读之前那一下**
  `records == 0` 且 `total > 0`（分母取的是 zip 目录里的未压缩大小，一个字节都不必
  先解），**收尾那一下** `records == 1、games == 1、0 < bytes <= total`。
  命令行两处每五秒打一行（`run_zh_sync`、`heal_zh_store`），界面接到 `Handle::tick`
  （`gui/src/scrape.rs::open_zh`、`core/src/sources.rs::refetch`）。
- [x] 重建期间按得停，**停下之后旧索引仍然可用** —— 两条新测试：
  `zh::sync::重建按得停_停下之后那份索引原样等着下一趟`（`Rebuilt::Halted`、
  `store.rebuilding()` 仍是 `Some`、指纹没变；紧接着再来一趟没人叫停的就
  `Rebuilt::Done { games: 1, .. }`）；
  `zh::sync::取数读原件时按停_手上那份索引原样可用`（库里已有旧那一版，读新原件读到
  一半按停 → `SyncError::Halted`，`store.load()` 里仍是旧那一版的 `id == 4`、指纹仍是
  旧的）。后者还钉了一句 `error.to_string() == Halted.to_string()` ——
  任务台按「那句话正是 `Halted` 交出来的那一句」把「停了」与「失败」分开记
  （`task::Board::settle`），差一个字就记成失败。
- [x] 重建这条路**一个网络请求都不发** —— `zh::sync::rebuild` 的签名里没有 `Fetcher`；
  原有测试 `zh::sync::结构版本一变就从本机那份原件重建_不下载不报错` 整条测试里
  没有 fetcher。命令行的 `heal_zh_store`（`identify` / `scrape` / `zh find`）与界面的
  `open_zh` 走的都是它。
- [x] 门禁全绿（`--all-features`）——
  `cargo clippy --workspace --all-targets --all-features -j 1`：0 error、0 warning；
  `cargo test --workspace --all-features -j 1 -- --test-threads=2`：
  **58 个 `test result: ok`、1,544 条通过、0 失败**（基线 58 / 1,539，这一票加了 5 条）。


## 挂单裁决

从 `.scratch/PARKING-LOT.md` 迁来。那份挂单是这一轮队列的**待办队列**，收尾时清空；
裁决之后条目迁回它所属的票，编号 `Qn` 留着占号、不复用。

### Q15 — `romcat zh sync` 里先 heal 再 sync，远端有新版时那一趟解压是白干的

- **来自：** 票 `offline-chinese-fields/02`（`/code-review` 的产出）
- **类别：** 路过发现，不在范围内（是票 01 引进来的）
- **在哪：** `crates/cli/src/main.rs` 的 `run_zh_sync`（约 4129 行那句 `heal_zh_store`）、
  `crates/core/src/zh/sync.rs:130` 的 `pending` 那道闸
- **为什么没停线：** 同 Q14，在这张票没动过的文件里；而且它只是**慢**，不出错。
- **这张票实际做了什么：** 没碰它，只记准：`sync()` 自己已经有 `pending` 那道闸——
  等着重建的那一份本来就不走「指纹没变就跳过」，会自己 `read_dump` + `replace` 一遍。
  先 heal 的话，heal 把本机那份旧 zip 整份解开读完（435 MB / 解开 960 MB，几分钟），
  紧接着 sync 发现 `latest.json` 是新的一版，下载新档再解一遍，**heal 那一趟的产物当场
  被覆盖**。只有指纹一样时 heal 才是有用的（heal 完 sync 就 skip 了）。两条出路：
  `run_zh_sync` 里干脆不 heal（交给 sync 自己那条路），或者把 heal 挪到「取到 latest、
  确认指纹没变」之后。`identify` / `scrape` 那两条路上的 heal 不受影响，那里没有 sync。
- **谁来裁：** 拿主意的人
- **状态：** open
- **收尾裁决：** **由本票接着做** —— 它就是从这条挂单立起来的。
- **本票落地：** 走的是**第一条出路**——`run_zh_sync` 里那句 `heal_zh_store` 干脆拿掉，
  交给 `sync()` 自己那条路（它本来就有 `pending` 那道闸）。挪 heal 那条没挑，因为那等于
  在同一条路上再摆一份同样的判断。代价记在挂单 `Q149`：断网时 `zh sync` 现在整条失败，
  而离线重建那四条路（`identify` / `scrape` / `zh find` / 界面）一条都没少。

### Q25 — 中文索引就地重建时一句话都不打，用户看见的是一个像死掉了的进程

- **来自：** 票 `offline-chinese-fields/03`（`/code-review` 的产出）
- **类别：** 路过发现，不在范围内（票 01 引进来的）
- **在哪：** `crates/cli/src/main.rs` 的 `heal_zh_store`（那句 `Rebuilt::Done` 的
  eprintln 在重建**跑完之后**才发）、`open_zh_store` 与 `run_zh_find` 两个调用点
- **为什么没停线：** 只是没说话，不出错；而且它在票 01 的代码里。
- **这张票实际做了什么：** 没碰它。这张票把 `load_zh_index` 拆成
  `open_zh_store` + `load`（刮削那一趟要留着库句柄读简介），`heal_zh_store` 的调用位置
  与时机**一个字都没变**。记准症状：`zh::sync::rebuild` 要读+解压 960 MB 再整份写库，
  几分钟起步，这期间标准错误上一个字都没有；而且这一趟不看取消令牌，`identify` 的
  Ctrl-C 管不到它。改法：开工**之前**先打一句「结构版本 X → Y，正在从本机那份原件重建，
  不会下载任何东西」，并把 `cancel` 传进去。
- **谁来裁：** 拿主意的人（与 Q15、Q23 同属「中文索引重建这条路」，该并成一张票）
- **状态：** open
- **收尾裁决：** **由本票接着做** —— 它就是从这条挂单立起来的。
- **本票落地：** 照挂单说的两样都做了，只是那句「开工之前先打一句」挪到了
  **开读那一下**（`Progress` 的头一次回调）——摆在调用之前打的话，缓存里根本没有那份
  原件时会先许一句「正在重建」、紧接着又说「找不到原件」，头一句是假的。
  `cancel` 也传进去了（`zh::sync::Context::cancel`），`identify` 的 Ctrl-C 现在管得到它。

### Q31 — `romcat zh sync --full` 撞上等着重建的索引时，那份 435 MB 原件解两遍

- **来自：** 票 `offline-chinese-fields/04`（`/code-review` 的产出）
- **类别：** 路过发现，不在范围内
- **在哪：** `crates/cli/src/main.rs` 约 4156 行，`heal_zh_store` 与紧接着那句 `sync()`
- **为什么没停线：** `heal_zh_store` 先从本机原件整份重建一次（读 zip、写 8.7 万条），
  紧接着 `sync()` 因为 `--full` 永远不走「指纹没变就跳过」，把同一个文件又读一遍、
  `replace` 一遍。多花的是分钟级的白工，不是错误结果；而这是票 01 落下的代码，
  这张票不该顺手改另一张票的控制流。
- **这张票实际做了什么：** 没改，只记在这儿。修法一句话：`heal_zh_store` 那一步加个
  `!args.full` 的闸——`--full` 那一档本来就要整份重读，重建这一趟是纯白工。
- **谁来裁：** 拿主意的人（同上，并进「中文索引重建这条路」那张票）
- **状态：** open
- **收尾裁决：** **由本票接着做** —— 它就是从这条挂单立起来的。
- **本票落地：** 修法比挂单里那句更彻底：不是给 `heal_zh_store` 那一步加个 `!args.full`
  的闸，而是整句拿掉（见 `Q15`）——`--full` 与不带 `--full` 于是走同一条路，
  那份原件在哪一档都只解一遍。
