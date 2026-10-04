# 18: 多碟那一条前端条目改指 `.m3u`，前端里换碟不用手动

**What to build:** 票 11 让多碟变体同步到卡上时多生成一份 `.m3u`，但前端条目照旧启动头一张碟（挂单 `Q1647`），用户故事 27「前端里换碟不用手动」没全成立：
- **ES-DE**：自己扫目录，`.m3u` 在它那儿是另一条没元数据的条目，封面多半找不着（ES-DE 照 ROM 文件名找媒体，铺的是照头一张碟的名字）。开着「只显示 gamelist 里的游戏」时这一条也看不见。
- **Pegasus**：一份 `.m3u` 都不生成（`uses_playlists` 答用不上），`files:` 只写头一张碟，第二张碟在 Pegasus 上点不到。

同步那一侧让多碟变体那一条改指播放列表，设计稿那句是「前端用 .m3u 播放列表启动并换碟」：
- **ES-DE**：`<path>` 换成 `.m3u`。媒体照 `.m3u` 的名字铺（或者播放列表改叫头一张碟的名字，二选一）。几张碟本身不再各自列成条目：用 `<hidden>`、挪进子目录，或者别的 ES-DE 认的办法。先查 ES-DE 3.x 的官方文档，查到什么写进票。
- **Pegasus**：`files:` 那一行换成 `.m3u`，于是 Pegasus 也答用得上；或者退一步把几张碟都写进 `files:`（Pegasus 自己那套多碟表达）。先查 Pegasus 文档。
- 两家各有不止一条站得住的路：做到岔路口照挂单规矩记，挑最站得住的走；要拿主意的人点头的，列成选择题回来问。

来源：拿主意的人 2026-10-04 在票 11 回报之后裁（挂单 `Q1647`）：另开一张票，这一轮做。

收挂单 `Q1647`：多碟那一条前端条目改指播放列表。

规格：`../spec.md`（用户故事 27）；已裁定的形状与核查见 `../grill.md`；ADR-0004（主库只读，导出那一侧照旧不生成）、ADR-0017（能力档案）。

⚠️ 本票、`19`、`10` 与 `core-answers-once/03` 都动同步落点（`sync/prepare.rs`、`sync/frontend.rs`、适配器），串着做。

**Blocked by:** 11

**Status:** done

- [x] 先写在今天 `main` 上红的测试（真盘同步那份测试）：两碟变体同步到 ES-DE 目标后，gamelist 里那一条的 `<path>` 指 `.m3u`，媒体找得着（按 ES-DE 的命名规矩断），各张碟不再各成一条可见条目
  - `crates/core/tests/sync_run.rs::两碟的变体同步到_es_de_的卡上_条目指播放列表_封面照播放列表的名字铺_各张碟藏起来`：走真的那条线（`sync::prepare` → `execute::run`）。卡上 `gamelists/ps/gamelist.xml` 照 ES-DE 适配器读回来正好三条：一条 `<path>./某游戏/游戏.m3u</path>` 带着名字、不藏；`./某游戏/游戏 (Disc 1).cue`、`./某游戏/游戏 (Disc 2).cue` 各一条 `<hidden>true</hidden>`。封面落在 `downloaded_media/ps/covers/某游戏/游戏.png`（ES-DE 照 `<path>` 那份文件去掉扩展名的名字找媒体），照头一张碟的名字铺的那一份不在了。**先在 `f3ba507` 上红过**：`该有一条指播放列表`（left 0 / right 1）。
  - 适配器那一层：`adapter::gamelist::tests::条目指着播放列表时_几张碟各写一条藏起来的条目`（整份字面量：藏起来的那几条只有 `<path>` 与 `<hidden>`，名字缺省时 ES-DE 拿文件名补）。
  - 做法照拿主意的人 2026-10-04 裁的甲：碟**不挪地方**（卡上的布局照旧照搬键）。条目在卡上启动哪一份由同步那一侧在播放列表**筛过之后**交出来（`sync::playlist::Laid::launching` → `converge::Launch`），收敛照它写条目（`Game::files` 那一条写播放列表、几张碟进新字段 `Game::hidden_files`）、铺媒体照它起名——放不进目标的播放列表不在卡上，条目照旧指头一张碟（`sync::playlist::tests::条目启动哪一份只交筛过之后还在的播放列表`）。为此 `sync::prepare_selected` 改了次序：先筛 ROM → 播放列表（再筛）→ 铺媒体 → 折前端元数据（再筛，`Desired::add_and_screen`），步数没变。
  - **保留**：ES-DE 的「Show hidden games」**默认开着**（源码 `es-core/src/Settings.cpp` 的 `ShowHiddenGames = {true, true}`；FAQ 明说故意不默认关）。默认设置下藏起来的那几张碟照旧各列一条、只是变淡，**要用户在 ES-DE 的 Other settings 里把「Show hidden games」关一次**，才真的只剩指播放列表的那一条。另一条路（「目录当文件」，碟挪进 `游戏.m3u/` 目录，默认就只剩一条，但改卡上的目录形状）与它的代价记在挂单 `Q1827`，谁来裁：拿主意的人。`.cue` 碟旁边的 `.bin` 照旧各列一条（单碟 cue 游戏今天也这样，不归本票）。
- [x] 同一个变体同步到 Pegasus 目标后，元数据里那一条启动得了每张碟（指 `.m3u`，或者 `files:` 列全几张碟，照落定的那条路断）
  - 落定的是甲（拿主意的人 2026-10-04 裁）：`files:` 那一行写 `.m3u`，`Pegasus::uses_playlists` 改答用得上。`crates/core/tests/sync_run.rs::两碟的变体同步到_pegasus_的卡上_条目的_files_指播放列表_碟本身不列`：卡上 `ps/某游戏/游戏.m3u` 逐字是两张碟，`ps.metadata.pegasus.txt` 里正好一条、`files` 就是 `ps/某游戏/游戏.m3u`。**先在 `f3ba507` 上红过**（Pegasus 的卡上没有播放列表）。适配器那一层：`adapter::pegasus::tests::条目指着播放列表时_几张碟一个字都不写`。
  - 票 11 那两条照实改：`用不上播放列表的前端_同一套多碟游戏一份都不生成` 改成 `两家前端都用得上播放列表_同一套多碟游戏两张卡上各一份_都进了清单`；`用不上播放列表的前端连平台清单都不读_那份清单写坏了也排得出计划` 改成 `两家前端都要读平台清单_那份清单写坏了照实说读不动`（`prepare_selected` 里「用不上的前端不读平台清单」那道闸留着，给答「用不上」的适配器——trait 的默认；内置两家都用得上，这道闸眼下没有测试走得到）。`romcat adapters` 表里 Pegasus 那一行改印「同步时生成」（`crates/cli/tests/gamelist.rs::能力档位对用户可见`）。
  - **保留**：没生成播放列表的那几套（档案说不吃 `.m3u`、有一张碟放不进或落成吃不下的形态、播放列表本身被筛掉）照旧只指头一张碟，Pegasus 上别的碟点不到——挂单 `Q1828`，谁来裁：拿主意的人。
- [x] 单碟变体的条目一个字不变（一条测试钉住）
  - `crates/core/tests/sync_run.rs::单碟变体的条目一个字不变_两家都是`：同一份主库里那个单碟的 FC 游戏，ES-DE 的 `gamelists/FC/gamelist.xml` 与 Pegasus 的 `FC.metadata.pegasus.txt` 整份逐字等于字面量（`<path>./魂斗罗.zip</path>` / `files: FC/魂斗罗.zip`，没有藏起来的条目）。**保留**：这一条没在 `f3ba507` 上跑过（这棵树上没法单独编那一版）；字面量照本仓那两家既有的写法（`<path>`、`<name>`、`x-romcat-variant`；中文标题排不动不写排序标题），单碟那条路的代码只多了「启动表里没点名就照旧写主文件」一个分支。
- [x] 移出子库后条目、播放列表、媒体一起清干净
  - `crates/core/tests/sync_run.rs::多碟那套移出子库_条目_播放列表_媒体一起清干净`：两家各跑一遍。头一趟卡上有元数据文件、`ps/某游戏/游戏.m3u` 与封面（ES-DE 是 `downloaded_media/ps/covers/某游戏/游戏.png`，Pegasus 是 `media/<哈希前两位>/<哈希>.png`）；规则换成只要 FC 再同步一趟，元数据文件（PS1 一个条目都不剩，整份删掉）、播放列表、封面、两张碟都从卡上与清单里没了。这条写在实现之后；它头一半断的「卡上有 `游戏.png`、Pegasus 卡上有播放列表」与上面两条先红过的是同一件事。票 11 那条 `那套多碟游戏移出子库_播放列表跟着清单一起删掉` 照旧绿。
- [x] 导出到主库照旧不生成 `.m3u`，也不改条目
  - `crates/core/tests/sync_run.rs::导出到主库那一侧不生成播放列表_主库里只多出元数据文件` 加了一段：两家导出到主库之后，那套多碟游戏的条目照旧指碟 1（ES-DE `某游戏/游戏 (Disc 1).cue`、Pegasus `ps/某游戏/游戏 (Disc 1).cue`），一条藏起来的条目都没有；主库里一份 `.m3u` 都没有，原有每一份大小与修改时间都没变（ADR-0004）。**钉子**：导出那一侧交空的启动表（`converge::run` 与 `transfer::media_to_lay`），它在 `f3ba507` 上本来就是这样——钉的是「以后谁往导出里接播放列表就红」。
- [x] 两家文档查到的依据写进票
  - **ES-DE**（gitlab `es-de/emulationstation-de` master，2026-10-04 取）：
    - 用户指南 `USERGUIDE.md`「Multiple game files installation」：「It's highly recommended to create `.m3u` playlist files for multi-disc images as this normally automates disk swapping in the emulator. It's then this .m3u file that should be selected for launching the game.」
    - 同一份「Directories interpreted as files」：目录名带已配置的扩展名就当一个文件（`~/ROMs/psx/Final Fantasy VII.m3u/` 里放几张碟与同名 `.m3u`）；源码 `es-app/src/SystemData.cpp` 的 `populateFolder` 见到这样的目录就建成游戏、不往里扫。本票**没走**这条（会改卡上的目录形状），记在 `Q1827`。
    - 同一份媒体那一段：「The media files must correspond exactly to the game files」——`~/ROMs/c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u` 的截图是 `downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg`；「目录当文件」的例外是媒体名带着那个「扩展名」（`dig.scummvm.png`）。源码 `es-app/src/FileData.cpp` 的 `getMediafilePath`：媒体目录 + 平台 + 类型 + 子目录 + `getDisplayName()`（文件名去掉扩展名）。
    - 同一份「Show hidden games」：「…or for multi-disc games where you may only want to show the .m3u playlists and not the individual game files」；元数据「Hidden」：「…or to hide the actual game files for multi-disc games. If a file or folder is flagged as hidden but the _Show hidden games_ is enabled, then the opacity for the entry will be lowered significantly」。
    - `FAQ.md`「When I hide a game using the metadata editor it's not really getting hidden, is this a bug?」：默认不删、只变淡，要关掉「Show hidden games」；「The reason this option is not disabled by default is that new users could very easily make a mistake by hiding some files accidentally」。`FAQ.md`「I have many games with multiple files, is there a way to show these as single entries?」指向「目录当文件」。
    - 源码 `es-core/src/Settings.cpp`：`ShowHiddenGames = {true, true}`、`ParseGamelistOnly = {false, false}`。`es-app/src/GamelistFileParser.cpp`：条目没写 `<name>` 时拿文件名补（「Make sure a name gets set if one doesn't exist」）；只有「Show hidden games」关着时才把藏起来的条目整条删掉。
    - 用户指南「Only show games from gamelist.xml files」：「only games that have metadata saved to the gamelist.xml files will be shown」——指播放列表那一条现在在 gamelist 里，开着它也看得见。
  - **Pegasus**（官方文档 https://pegasus-frontend.org/docs/user-guide/meta-files/ 与 https://pegasus-frontend.org/docs/user-guide/meta-assets/，2026-10-04 取）：
    - Games 一节 `file, files`：「The file or list of files (eg. disks) that belong to this game. Paths can be either absolute or relative to the metadata file. If there are multiple files, you'll be able to select which one to launch when you start the game.」——几张碟都写进去是另一条路（启动前挑一张，换碟要退出重开），本票没走。
    - 启动命令参数：`{file.path}`「Absolute path to the file」——条目指什么就交什么，`.m3u` 能不能启动看模拟器（能力档案那一问，`Q1648`）。
    - Collections 一节：合集只收 `extension`/`files`/`regex` 圈进来的文件；收敛写的合集段一样都不写，没进任何条目的碟文件在 Pegasus 里不成条目。
    - 资源文件：`<dir>/media/<gamename>/`，`<gamename>` 是标题或「one of the game's files」去掉扩展名——本仓 Pegasus 走内容寻址、路径写进 `assets.*`，不受条目改指影响。
- [x] 门禁全绿
  - `/Users/nicoer/dev/game-wt/logs/q8-vs-18-gate1.log`：开头两行 `/Users/nicoer/dev/game-wt/slot3`、`q8/vs-18`；末行 `EXIT=0`；汇总表七条全绿（fmt 2s / glossary 0s，扫了 13 份 `.rs` 新写的 648 行、没撞上 / check 20s / clippy 22s / test 998s，带 `--no-fail-fast`、90 个测试目标 / numbers 4s / doc 19s）。`CARGO_INCREMENTAL=0`、`-j 3 --test-threads 3`。
  - README 的数在这棵树上写回过（`cargo xtask numbers --write`，`/Users/nicoer/dev/game-wt/logs/q8-vs-18-numbers.log`，`EXIT=0`：票数 186/217 → 187/217，测试条数 2,879 → 2,886），合并时由编排者在 `main` 上重写。

**收挂单 `Q1647`**（`.scratch/PARKING-LOT.md` 里那一条已由拿主意的人 2026-10-04 裁乙、托给本票；本票在它的状态行上补了「照裁定做完」）：多碟那一条前端条目改指播放列表——ES-DE 的 `<path>` 指 `.m3u`、封面照它的名字铺、每张碟的主文件各写一条 `<hidden>true</hidden>`；Pegasus 的 `files:` 指 `.m3u`、`Pegasus::uses_playlists` 改答用得上；导出到主库照旧不生成、不改条目。留下的两处在 `Q1827`（ES-DE 默认还显示藏起来的条目；「目录当文件」那条路）、`Q1828`（没生成播放列表的那几套照旧指头一张碟）。

本票用掉的挂单号：`Q1827`–`Q1829`（`Q1829` 是路过发现：主文件转了格式，同步写到卡上的条目还指着原来的名字）。

## Comments

**拿主意的人 2026-10-04 裁**（实现者查完两家文档后停下列了选择题，两题都选甲）：
1. **ES-DE：甲。** 碟不挪地方，每张碟的主文件各写一条 `<hidden>true</hidden>`；条目 `<path>` 指 `.m3u`，媒体照 `.m3u` 的名字铺。「Show hidden games」默认开着这一层写进票，验收那一格带保留勾，写清要用户在 ES-DE 里关一次；另记一条挂单（谁来裁：拿主意的人），把乙（「目录当文件」）和它的代价写进去，留作日后换路的依据。
2. **Pegasus：甲。** `files:` 那一行换成 `.m3u`，`Pegasus::uses_playlists` 改答用得上；票 11 那条「Pegasus 一份都不生成」的测试照实改；没生成播放列表的那几套照旧指头一张碟，记挂单。
