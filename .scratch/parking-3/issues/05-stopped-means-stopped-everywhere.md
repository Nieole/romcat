# 05 — 按停之后到处都说「停了」

**What to build:** 维护者在界面上按一下「停下」，那一趟显示成**停了**，不是失败——
不会以为自己按坏了什么。中文离线源那 435 MB 下载按 Ctrl-C 之后**当场**收手，
不用等它下完。（挂单 `Q151` `Q150`）

眼下任务台分「停了」与「失败」的判据是**那句话正好是某一句**，界面刮削那几处交出去的
话与核心库定的差着字，于是按停被记成失败。

**Blocked by:** 03

**Status:** done

- [x] 分「停了」与「失败」的判据**由类型说话**，不再靠字符串比对。改完之后界面任何一处措辞改动都不可能把「停了」悄悄变成「失败」。 ——
  新立 `task::Cutoff { Halted, Failed(String) }` 当**一趟活交不出产物时的唯一出口**，
  `Job<T>` / `Board::queue` / `Board::run_here` 的错误类型从 `String` 换成它，
  `Board::settle` 改成 `Err(Cutoff::Halted) => Ending::Stopped` /
  `Err(Cutoff::Failed(why)) => Ending::Failed`——**那一段里一个字符串比对都没有了**，
  连 `handle.stopped()` 都不看了。
  **`Cutoff::Halted` 是个不带字段的支**：措辞根本不到任务台手上，改哪一处都动不了分档。
  同时**删掉 `impl From<Halted> for String`**——「把被按停洗成一句话」在类型上没有顺手的
  写法了，编译器会逼所有收把手的长入口换成 `Cutoff`（`sync::prepare`、
  `sublibrary::survey`、`sources::refetch`、界面那四条闭包）。
  测试：`crates/core/tests/task.rs::措辞与核心库那一句差着字的按停照旧记成停了`
  ——它交上来的是 `CollectionError::Halted`，那句话**与 `Halted` 那一句差着字**
  （测试自己 `assert_ne!` 钉着这一点），照旧记成 `Ending::Stopped`。
  **红过一次**：把 `From<CollectionError> for Cutoff` 临时改成「一律洗成
  `Failed(error.to_string())`」（也就是老那条路），这一条与下面第 3 条当场红，
  报的是「把按停记成了「失败：整批排锚按停了：…」」。
  裁决记在 `Q234` `Q235` `Q236`；仍留着的那条口子（有人特意先 `.to_string()`）记在 `Q237`。
  **收尾审查还揪出低一层的同一个形状**：`zh::sync::SyncError` 的
  `Fetch(#[from] FetchError)` 会把 `FetchError` 新长出来的 `Halted` 支一并吞进
  「取数失败」——随手 `?` 一下就把「按停了」洗成失败。已改：那一支去掉 `#[from]`，
  换成手写的 `From<FetchError> for SyncError` 把 `Halted` 路由到 `SyncError::Halted`
  （下载那一处原先手写的 `match` 跟着收掉了）。`dat` 与 `titledb` 那两个同形状的
  `SyncError` 眼下产不出那一支（它们走 `download`，`keep_going` 恒真），
  按票面的范围原样没动，记在 `Q242`。
- [x] 界面刮削那几处不再手写「按停了」，改由类型转换得来。 ——
  `crates/gui/src/scrape.rs` 那四处 `task.step("…").map_err(|_| "按停了".to_string())?`
  全删了，现在就是 `task.step("…")?`（`Halted` 经 `From` 折成 `Cutoff::Halted`）。
  `grep -n '"按停了".to_string()' crates/gui/src/` 一处都没有。
  **顺手收了 `Q217` 点名留给本票的第二个出口**：刮削走到 `scrape::run` 里被按停
  （或者联网源自己收的手：配额、凭据、断网）时照旧返回 `Ok`，从前长着「跑完了」的样子
  进任务台；现在那一支报一句 `Handle::halfway`，记成 `Ending::Halfway`，
  `finished()` 那句回执也不再说「刮削跑完了」。
  测试：`crates/gui/src/scrape.rs` 的
  `被按停的那一趟说得出留下了什么_而且不说跑完了` 与 `真跑完的那一趟不报停在半路`。
  识别那一层同一个形状，本票没碰，记在 `Q238`。
  **收尾审查在这一处抓到两句被改错的话，都已改**：一是 `finished()` 起头写「刮削按停了」
  ——那一趟记的是「停在半路」，而 `Ending::Stopped.render()` 正好就是「按停了」，
  两档撞脸（词表「收场」：四档四句）；改成起头一律「刮削停在半路」，
  「是被你按停的」还是「是它自己收的手」放到后半句。二是 `Panel::settle` 的
  `Ending::Stopped` 那支还写着「已经采到的那些留在中立库里」——这张票之后落到那一档的
  **只剩「还没走到 `scrape::run` 就被 `?` 出来」那个出口**，它一个锚点都没采，
  那句话成了骗人的；改成「这一趟还没开始采——中立库与媒体池一个字节都没动」。
- [x] 有一条界面测试断：按下停下之后，任务屏历史那一行写的是「停了」那一类，**不是**「失败」。这条测试要能在判据改回字符串比对时当场红。 ——
  `crates/gui/tests/task.rs::按下停下之后任务屏历史写的是停了那一类_不是失败`：
  往任务台上排一趟「放进「收藏」· 200 个变体」，`app.tasks_mut().stop(id)`
  （界面上那颗「停下」按的就是它），断 `history()[0].ending == Ending::Stopped`，
  再断屏上那一行正好是「按停了」、**没有任何一行带「失败」**。
  它交上来的是 `CollectionError::Halted`，测试开头先 `assert_ne!` 钉住「这句话与
  `Halted` 那一句差着字」——**判据回到字符串比对它就当场红**，上面那次实测报的是
  「按了停下，历史那一行却写成了「在「为 200 个变体折锚」这一步失败：…」」。
- [x] 中文离线源那 435 MB 下载接上中断信号：按下 Ctrl-C 之后在**当前这一块**读完就收手，不等整份下完。 ——
  `Fetcher` 上加了一条带默认实现的 `download_while(url, to, keep_going)`；
  `HttpFetcher` 覆盖它，走新的 `copy_while`：**一块 1 MiB（`DOWNLOAD_CHUNK`），
  每块之前问一次要不要收手**，收手时把那个 `.partial` 删掉（半截留着的话下一趟会
  以为原件已经在手边）。`zh::sync::sync` 把 `Context::cancel` 折成那个问法递下去，
  收手折成 `SyncError::Halted`（不是「取数失败」）。命令行的 Ctrl-C 与界面那颗「停下」
  接的是同一个信号（`run_zh_sync` 传的就是 Ctrl-C 那个 `CancelToken`），两边一并有了。
  **多久收手**：判据是**搬进去了多少字节**——
  `dat::fetch::那份_435_兆的原件按停之后在当前这一块读完就收手_不等整份下完`
  在一份 435,891,841 字节的合成正文上放行三块就收手，搬进去的**正好 3 MiB**
  （`assert_eq!(落点.搬进去了, 3 * DOWNLOAD_CHUNK)`），实测 **0.5–0.9 ms**，
  而同一份整份搬完是 **16.8–18.7 ms**（合成正文，内存速度）。
  收手的**上界是读完当前这一块**——1 MiB；换算到一条 20 MB/s 的线路上约 **50 ms**，
  而从前要等的是剩下那 430 MB，约 **21 秒**。
  接线那一头由 `zh::sync::那_435_兆的下载按停之后当场收手_不等它下完` 钉：
  **两头都跑**——按了停下的那一趟**一块都没搬**、缓存里不留半截、错误是
  `SyncError::Halted`；没按的那一趟 416 块一块不少地下完。
  实测把递下去的那个问法换成恒真，这一条当场红（「按了停下却还是搬了几块」）。
  一块 1 MiB 是**选的不是量的**，记在 `Q240`。
  收尾审查另补了一条：`copy_while` 换掉的 `io::copy` 本来会吞并重试
  `ErrorKind::Interrupted`，新循环补上了同一档——不补的话一个信号就会让按停显示成
  「读写 … 失败：Interrupted」，正是这条路要消灭的那种误报。
- [x] 取数那一侧的文档改准——现在写着「接得住的只是读那份原件那几分钟，下载那 435 MB 照样停不掉」，这张票之后那句不再成立。 ——
  `crates/core/src/sources.rs::refetch` 的文档改成「**中文离线源那一条整条接住了**
  ——下载那 435 MB 与随后读那份原件的几分钟都收中断信号，按下停下在**当前这一块**
  读完就收手，不等整份下完；缓存里也不留半截。另外两条还接不住（`dat::sync::run` /
  `titledb::sync::sync`），剩下那两段记在挂单 `Q62`」。
  跟着改准的还有三处：`crates/core/src/zh/sync.rs` 的模块文档（多了一段说下载也按同一条
  接住）、`crates/gui/src/roots.rs` 那颗「重取」按钮的悬停（从前一句「开跑之后停不下来
  ——底下那三个入口不收中断信号」现在分开说三条）、`crates/cli/src/main.rs` 那句中断
  回执（从前只说「停在两条记录之间」，那对下载那个出口不成立，改成「停下的地方是干净的：
  库里一个字都没写，缓存里也没留下半截原件」）。
  另外拆掉了一处**靠巧合成立的断言**：`zh::sync` 那条按停测试从前还断
  `error.to_string() == Halted.to_string()`，那是老判据的残留，现在断的是
  `matches!(error, SyncError::Halted(_))`。
- [x] 挂单 `Q151` `Q150` 标 `settled` 并迁回票 `queue-followups/03`。 ——
  两条原文迁进 `.scratch/queue-followups/issues/03-zh-rebuild-path.md` 末尾新开的
  「## 挂单裁决（第三轮）」一节，各带一条「收尾裁决（第三轮）：**settled**」；
  原验收正文一个字没动。`.scratch/PARKING-LOT.md` 的「已收索引」表里两行也标上了
  **settled** 与去处。**`Q151` 没走挂单里写的那个改法**（「那四处改成
  `.map_err(String::from)`」）——那条只把这一处对上，判据还是字符串比对；
  走的是换类型那条，理由写在迁回去的裁决里。

## 范围

- **只收中文离线源这一条下载。** 三条取数入口都停不下来（挂单 `Q62`）是同一件事的全部，另两条这张票不碰——如实记在挂单里，别顺手扩。
