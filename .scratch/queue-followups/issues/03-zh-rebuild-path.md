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


## 挂单裁决（第二轮）

从 `.scratch/PARKING-LOT.md` 迁来；编号 `Qn` 留着占号、不复用。

### Q149 — `zh sync` 断网时不再顺手把等着重建的那份索引补回来

- **来自：** 票 `queue-followups/03`
- **类别：** 票没说的第三种情况
- **在哪：** `crates/cli/src/main.rs::run_zh_sync`（从前那句 `heal_zh_store`）
- **为什么没停线：** 这是票正文点名的那个顺序（「先取到远端版本、确认指纹没变，才就地
  重建」）的必然后果，而**离线重建那条路一条都没少**。
- **这张票实际做了什么：** **走的是「`run_zh_sync` 干脆不 heal」那条**（挂单 `Q15` 摆出的
  两条出路之一，另一条是「把 heal 挪到取到 latest 之后」）。挑它是因为 `zh::sync::sync`
  自己那道闸本来就把「等着重建的那一份」排除在「指纹没变就跳过」之外，取数那一趟顺手
  就把重建做了——挪 heal 等于在同一条路上再摆一份同样的判断。代价：`aux/latest.json`
  取不回来（断网、闸门拦下）时，`zh sync` 现在整条失败，而从前会先把索引就地重建好。
  离线重建仍有四条路：`identify`、`scrape`、`zh find` 与界面刮削，它们走
  `heal_zh_store` / `open_zh`，一个网络请求都不发。真要让 `zh sync` 断网时也补，
  得在「取 latest 失败」那一支上补一句回落——那是「网络不通时这条命令该怎么收场」的
  口径问题，不是这一票的正题。
- **谁来裁：** 拿主意的人
- **状态：** open
- **收尾裁决（第二轮）：** **settled** —— 这条的决定已经落进代码，并有测试钉着。


## 挂单裁决（第三轮）

从 `.scratch/PARKING-LOT.md` 迁来（票 `parking-3/05` 收的）；编号 `Qn` 留着占号、不复用。

### Q150 — 下载那 435 MB 仍然停不掉：Ctrl-C 要等它下完才生效

- **来自：** 票 `queue-followups/03`
- **类别：** 路过发现，不在范围内
- **在哪：** `crates/core/src/zh/sync.rs::sync` 里那句 `fetcher.download`；
  `dat::fetch::Fetcher` 那个接缝不收中断信号
- **为什么没停线：** 票的验收说的是「**重建期间**按得停」，而重建那一段（读+解 960 MB，
  时间的大头）现在停得下来了。
- **这张票实际做了什么：** 只把**读原件**那一段接上了中断信号
  （`zh::sync::Context::cancel`，每读一条看一眼）。下载那一段照旧：按下 Ctrl-C 之后要等
  那 435 MB 下完，才在读第一条记录之前收手。**没有假装它停得下来**——
  `sources::refetch` 的文档已经改成「`zh::sync::sync` 接得住的只是读那份原件那几分钟，
  下载那 435 MB 照样停不掉」。与挂单 `Q62`（三条取数入口都停不下来）是同一件事的一部分。
- **谁来裁：** 挂单 `Q62` 那条活
- **状态：** open
- **收尾裁决（第三轮）：** **settled** —— 由票 `parking-3/05` 做掉。
  `Fetcher` 上加了一条带默认实现的 `download_while`：`HttpFetcher` 覆盖它，
  一块 1 MiB（`DOWNLOAD_CHUNK`）地搬、**每块之前问一次要不要收手**，收手时把那个
  `.partial` 删掉；`zh::sync::sync` 把 `Context::cancel` 折成那个问法递下去，
  收手折成 `SyncError::Halted`（不是「取数失败」）。命令行的 Ctrl-C 与界面那颗「停下」
  接的是同一个信号，两边一并有了。
  测试：`dat::fetch::那份_435_兆的原件按停之后在当前这一块读完就收手_不等整份下完`
  （判据是**搬进去了多少字节**：放行三块就收手，搬进去正好 3 MiB，而那份是
  435,891,841 字节）与 `zh::sync::那_435_兆的下载按停之后当场收手_不等它下完`
  （两头都跑：按了停下的那一趟一块都没搬、缓存里不留半截；没按的那一趟 416 块一块不少）。
  **只收了中文离线源这一条**：DAT 与 Switch 那两条仍走 `download`，如实记在
  `Q239`（`Q62` 剩下那两段）。

### Q151 — 界面刮削那一趟把「按停了」折成一句对不上的话，任务台会把它记成失败

- **来自：** 票 `queue-followups/03`
- **类别：** 路过发现，不在范围内
- **在哪：** `crates/gui/src/scrape.rs` 四处 `task.step("…").map_err(|_| "按停了".to_string())`
  （695、698、720、728 行），排活的地方在同文件 404 行
- **为什么没停线：** 早在这张票之前就是这样，与「中文索引重建这条路」无关。
- **这张票实际做了什么：** **没碰它，只在核心库这一侧把口径对上。** 任务台分
  「停了」与「失败」的判据是**那句话正是 `Halted` 交出来的那一句**
  （`task::Board::settle`），差一个字就记成失败。这一票新开的那条停下之路两处都对得上：
  `zh::sync::SyncError::Halted` 是 `#[error(transparent)]`，`sources::refetch` 也不再往
  前头加「取中文离线源失败：」。gui 那四处交的是 `"按停了"`，与
  `Halted.to_string()`（「按停下了：停在两步之间，没留下半截状态。」）差着字——
  界面上按一下「停下」，那一趟会显示成失败，用户会以为自己按坏了什么。
  改法一句话：那四处改成 `.map_err(String::from)`（`Halted` 有 `From<Halted> for String`）。
- **谁来裁：** 拿主意的人 / 下一张动那几屏的票
- **状态：** open
- **收尾裁决（第三轮）：** **settled** —— 由票 `parking-3/05` 做掉，**但没走挂单里那个
  改法**。挂单说的是「那四处改成 `.map_err(String::from)`」，那条能让这一处对上，可判据
  照旧是字符串比对——下一处措辞差一个字，同一个 bug 就再来一遍。走的是另一条：
  一趟活交不出产物时的出口换成 `task::Cutoff { Halted, Failed(String) }`，
  `Board::settle` 按**支**分档，`Halted` 那一支**一个字都不带**；同时把
  `From<Halted> for String` 删掉，于是「把被按停洗成一句话」在类型上没有顺手的写法了
  （`Q234` `Q235` `Q236`）。界面刮削那四处手写的「按停了」直接删掉，`?` 一下把手就是。
  测试：`crates/gui/tests/task.rs::按下停下之后任务屏历史写的是停了那一类_不是失败`
  与 `crates/core/tests/task.rs::措辞与核心库那一句差着字的按停照旧记成停了`
  ——两条交上来的都是一句**与 `Halted` 差着字**的话（`CollectionError::Halted`），
  实测把那条折支的路换回「洗成字符串」两条当场红。
