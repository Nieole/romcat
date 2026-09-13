# 08 — 导出铺得出媒体（核心库那条路 ＋ 命令行开关）

**What to build:** 维护者跑 `romcat export` 时加上那个开关，**媒体跟着铺出去**——他用 Pegasus
直接读**主库**时看得见封面。**默认关着**，所以他平常那一趟 2.7 秒的导出一个字不变。

收挂账 `D64`：导出不写 `launch:`，也不铺媒体目录（本票只做媒体那一半）。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **这是这一批最大的一件**（收口时被判成「最便宜」，判反了）。**两条更省事的路都被实测否掉**：
- 让元数据里的资源路径**指向媒体池**——**跨不过适配器**：Pegasus 是内容寻址（路径就是哈希），
  ES-DE 镜像 ROM 的相对路径、文件名是去掉扩展名的 ROM 名。池按哈希存，ES-DE 结构上找不到。
- **无条件铺**——媒体池住在**工作目录**、主库在那块外置盘上，**硬链接跨不了文件系统**，
  于是一趟 2.7 秒的导出会变成一次几十 GiB 的拷贝，而人多半只想要元数据。

⚠️ **布局问题是解决了的**：每个适配器已经说得出自己要的媒体路径（同步那侧就在用它），导出照它铺。
⚠️ **落文件那条路导出今天没有**（链接/拷贝是同步执行器自己写的，不在媒体池上）——这一票要接上，
规矩与同步同一条：**同盘硬链接、失败降级拷贝**。
⚠️ 只写**媒体目录**，ADR-0004 明许（「只写元数据文件、媒体目录与子库」）。

- [x] 不加开关时，导出**盘上一个媒体文件都不多**（今天的行为一字不变）
  - 证据：`crates/core/tests/gamelist.rs::不开铺媒体时_导出目录里一个媒体文件都不多`（池里真有一份封面；多出来的只有两份 gamelist，`--json` 里没有 `media` 键）、`crates/core/tests/pegasus.rs::不开铺媒体时_主库里一个媒体文件都不多`（多出来的只有 `FC.metadata.pegasus.txt`，一个 `assets.` 都没写）、`crates/cli/tests/pegasus.rs::导出加上铺媒体才铺_干跑先说要铺几份多大`（不加 `--media` 时 `media/` 不存在，报告里没有媒体那一节）。
  - 保留：不开时命令行照旧不接 Ctrl-C（与今天一字不变，今天那个「按不停」本身是缺陷），见挂单 `Q587`。
- [x] 加上开关时，媒体按**各自适配器的布局**铺出去，两份适配器各验一条
  - 证据：Pegasus `crates/core/tests/pegasus.rs::开了铺媒体_按内容寻址铺进_media_而且条目里写着那条路径`（`media/<哈希前两位>/<哈希>.png`，条目写着 `assets.boxFront`）；ES-DE `crates/core/tests/gamelist.rs::开了铺媒体_照前端自己的布局铺进_downloaded_media_条目里一个路径都不写`（`downloaded_media/FC/covers/魂斗罗台版/魂斗罗.png`，条目里没有 `<image>`）；命令行 `crates/cli/tests/pegasus.rs::导出加上铺媒体才铺_干跑先说要铺几份多大`。
  - 布局与同步是同一份（`sync::media::lay_for`，`lay` 改成调它），资源槽填法是同一处（`converge::attach_assets`，`sync::frontend::lay` 改成调它）。「要铺几份、多大」交得出：`transfer::media_to_lay`（票 09 用；它是上界，挂单 `Q584` 已托给票 09）。
- [x] 同一块盘上走**硬链接**；跨盘降级**拷贝**，两种都验得到
  - 证据：硬链接 `crates/core/tests/gamelist.rs::铺媒体时同一块盘上走硬链接_不额外占空间`（`placement=硬链接`，两份同一个 inode）；复制 `crates/core/tests/gamelist.rs::铺媒体时媒体池与导出目录不在一块盘上_降级复制`。
  - 本机实测：带 `TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp` 时，媒体池（`target/tmp`，设备号 16777230）与导出目录（cs 卷，设备号 16777240）真在两块盘上。2026-09-14 那一趟带 `--nocapture`，印出「跨了盘：媒体池 …/target/tmp/… → 导出目录 …/cs/tmp/…，这一条验的是降级复制」；断言 `placement=复制`、inode 不同。日志：`/Users/nicoer/dev/game-wt/logs/slot-3-oc08-after-review.log`。
  - 落地复用同步执行器的 `place`（落点闸、`.romcat-part` 再改名）与从它抽出来的 `link_or_copy`，没有另写一份。
  - 保留：造不出两块盘时（CI 的 Linux、本机不带 `TMPDIR`）如实跳过，只印一行「跳过：…这一条什么都没验」。通过的测试 libtest 不印输出，要加 `--nocapture` 才看得见。
- [x] 铺出去的东西只落在**媒体目录**里，主库的 ROM 一个字节没动
  - 证据：`crates/core/tests/pegasus.rs::铺媒体只写进媒体目录_主库里的_rom_一个字节都没动`。导出目录就是主库根；原有文件逐个比字节、修改时间、链接数，一样不差；多出来的只有 `media/` 底下两份与元数据文件。
  - 硬链接探测文件落在媒体目录本身（`execute::place_media`）；落点的键里没有目录段时不探。
  - 词表**导出**词条「只写元数据文件」那半句已改成连同点名要铺的媒体目录。
- [x] 中途按停时，已经铺过的留在盘上、说得出铺到第几份（与导出那一趟同一套**收场**口径）
  - 证据：
    - 铺到一半按停：`crates/core/tests/sync_run.rs::导出铺媒体铺到一半按停_铺过的留在盘上_说得出铺了几份`。`place_media` 停在两份之间，铺过的留在盘上，没有半份文件。
    - 元数据写完、媒体还没铺就按停：`crates/core/tests/pegasus.rs::元数据写完媒体还没铺就按停_记的是没走完而且说得出媒体一份都还没铺`。任务台记 `Ending::Halfway`。
    - 「铺到第几份」那句话：`crates/core/src/adapter/transfer.rs` 的 `tests::铺媒体铺到一半收手时_那句话说得出铺到第几份与一共几份`。
    - 从 `export_task` 一路走到收场：`crates/core/tests/pegasus.rs::铺媒体连着没铺成主动停了_记的是没走完而且说得出铺到第几份`。那句话是「媒体铺到第 11 份（共 12 份）」。
  - 命令行的收场话从 `task::Ending::render` 出：`Ending::of` 从任务台 `settle` 里提出来两边共用；按停退 130。
  - 保留：媒体那一段**人按停**、再一路走到 `Ending::Halfway` 的整条路，没有确定性的端到端测试。导出里没有能把「停在第几份」钉死的接缝，由上面四条拼起来证。

## Comments

**收尾（2026-09-14）：** 收了挂账 `D64` 的媒体那一半；`launch:` 那一半没动。挂单 `Q581`–`Q588`（`Q584` 已托给票 09）。
全量门禁（2026-09-14，带 `TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp`）：fmt 1s、glossary 1s、check 14s、clippy 18s、test 329s、doc 4s，6 条全绿，`EXIT=0`。日志 `/Users/nicoer/dev/game-wt/logs/slot-3-oc08-gate.log`。
