# 20: 主文件转了格式，卡上的前端条目指转出来的那一份

**What to build:** 子库同步时主文件照能力档案转了格式，写到卡上的前端条目却还指着主库里那份原来的名字（挂单 `Q1829`）。例如 `PS1/最终幻想7.zip` 落到卡上是 `PS1/最终幻想7.chd`（`crates/core/tests/conversion.rs::光盘类不吃归档_把镜像解出来成裸文件`），可 Pegasus 的 `files:` 与 ES-DE 的 `<path>` 照旧写 `最终幻想7.zip`，前端里点下去找不着文件。

根子在 `crates/core/src/adapter/converge.rs` 的 `build_game`：`files` 取每个变体主文件的键剥掉根名，`DesiredFile::convert` 那一格它根本见不着。对照 `crates/core/src/sync.rs` 的 `Footprint::desired`：转格式那一支的落点已经换成产物的名字。

票 `18` 立了一条口子：同步那一侧对每个变体交「条目在卡上启动哪一份」（`playlist::Laid::launching` → `converge::Launch`），播放列表只是其中一种。本票让它把「主文件转了格式」也交出去：条目写主文件在卡上的落点，也就是期望状态里那一份，要转的就是产物。

来源：拿主意的人 2026-10-04 在票 18 回报之后裁（挂单 `Q1829`）：另开一张票，这一轮做。

收挂单 `Q1829`：条目照期望状态里主文件在卡上的落点写。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`；ADR-0004（主库只读：导出到主库那一侧不转、条目照旧）、ADR-0017（能力档案）。

⚠️ 本票、`19`、`10` 与 `core-answers-once/03` 都动同步落点，串着做。本票与 `19`（逐碟转格式）挨得最近：`19` 先落的话，多碟的每张碟转出来的名字也要进播放列表与条目，两张票之间按合并次序接上。

**Blocked by:** 18

**Status:** done

- [x] 先写在今天 `main` 上红的测试（真盘同步那份测试）：档案要把 `.zip` 解成裸镜像 / `.chd` 的子库，同步到 ES-DE 目标后 `<path>` 指卡上真有的那一份；同步到 Pegasus 目标后 `files:` 同理
  - `crates/core/tests/sync_run.rs::主文件从归档里解出来的变体同步到卡上_条目指卡上转出来的那一份_两家都是`：走真的那条线（`sync::prepare` → `execute::run`），档案是内置「独立模拟器-exfat」（PS1 那一行是 DuckStation，透明容器一律不吃）。主库 `ps/最终幻想7.zip` 里只装一份 `最终幻想7.chd`；同步后卡上是 `ps/最终幻想7.chd`、没有 `.zip`。ES-DE 的 `gamelists/ps/gamelist.xml` 照适配器读回来正好一条，`<path>` 是 `最终幻想7.chd`；Pegasus 的 `ps.metadata.pegasus.txt` 正好一条，`files:` 是 `ps/最终幻想7.chd`。两家都不藏，`x-romcat-variant` 照旧是主库里的键 `库/ps/最终幻想7.zip`。**先在 `3ea891d` 上红过**：left `["最终幻想7.zip"]`、right `["最终幻想7.chd"]`（日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-20-red1.log`，`EXIT=101`）。
  - 编排者点名的另一种：`没生成播放列表的多碟变体_条目指头一张碟转出来的那一份_两家都是`。DC 两张 `.zip` 碟，同一份档案里 DC（Flycast）不吃 `m3u`：两张碟都解开，没有播放列表；条目头一行指碟 1 解出来的那一份（ES-DE `某游戏/游戏 (Disc 1).chd`、Pegasus `dc/某游戏/游戏 (Disc 1).chd`），一条都不藏。只断头一行，别的碟列不列进 `files:` 是挂单 `Q1828` 的事（审查 Spec 轴提的，照改）。还有一条退路：`播放列表撞车没放行_条目退回指碟_1_转出来的那一份`（一套 `.zip` 碟、一套 `.chd` 碟，播放列表撞在 `ps/某游戏/游戏.m3u`、都没放行；`.zip` 那一套的条目指 `某游戏/游戏 (Disc 1).bin`，`.chd` 那一套照旧指它的 `.chd`）。**这两条是实现之后写的**：红是拿一个临时开关验的，开关让 `Footprint::launching` 只交播放列表那一半，也就是 `3ea891d` 上的行为，验完删掉。开着开关时两条都红：DC 那条 left `["某游戏/游戏 (Disc 1).zip"]`，撞车那条 left `["某游戏/游戏 (Disc 1).zip"]`。
  - 做法：「条目在卡上启动哪一份」收成一处，`sync::Footprint::launching(&Desired, &playlist::Laid)`，照**筛过之后**的期望状态定：
    - 卡上有播放列表的，启动播放列表（票 18 那一半；`playlist::Laid::launching` 降成 `pub(super)`，不再对外，免得调用方只取一半）。
    - 没点名、主文件转了格式的，`Launch { file: 期望状态里转出来那一份的落点, hidden: [] }`。
    - 原样搬的不点名，收敛照旧写主文件的键剥掉根名。
    转不转、转成什么只在 `Footprint::desired` 判一次（ADR-0024），这里只读它判出来的落点。`sync::prepare_selected` 与 `sync_run.rs` 的夹具改调它。`prepare_selected` 外层那道 `uses_playlists()` 拿掉了，`playlist::lay` 里自己会判，行为不变。
  - 播放列表与转格式的先后：`几张碟都转了格式而卡上有播放列表_条目照旧指播放列表_藏起来的是转出来的那几份`。两张 `.zip` 碟都解开、卡上有播放列表时，ES-DE 唯一可见的那一条照旧指 `某游戏/游戏.m3u`，藏起来的两条是 `.bin`。这条在 `3ea891d` 上本来就绿，钉的是「转格式不抢播放列表」。
  - **保留**：转出来那一份放不进目标时，不在期望状态里，条目照旧写原名。两种写法都指着卡上没有的文件，记在挂单 `Q1847`，谁来裁：拿主意的人。
- [x] 不转格式的变体，条目一个字不变（一条测试钉住）
  - `crates/core/tests/sync_run.rs::同一个子库里不转格式的变体_条目一个字不变_两家都是`：同一趟同步、同一份会转格式的档案，PS1 那一份解开了，FC 的卡带包 `魂斗罗.zip` 档案吃得下、原样搬。`gamelists/FC/gamelist.xml` 与 `FC.metadata.pegasus.txt` 整份逐字等于票 18 那条 `单碟变体的条目一个字不变_两家都是` 的字面量（`<path>./魂斗罗.zip</path>` / `files: FC/魂斗罗.zip`）。票 18 那条（不作声称的档案）照旧绿。这条在 `3ea891d` 上本来就绿：原样搬的变体不点名、收敛走原来那条路，钉的是「隔壁转了格式，它不跟着变」。
- [x] 媒体名字照旧找得着（ES-DE 照主名找，扩展名换了不影响——测试断言一次）
  - `crates/core/tests/sync_run.rs::主文件转了格式_es_de_的封面照条目指着的那一份的名字铺_找得着`，两种各断一次：
    - 只换了扩展名（`最终幻想7.zip` 里装 `最终幻想7.chd`）：封面照旧落在 `downloaded_media/ps/covers/最终幻想7.png`，字节是收进池的那一份。
    - **票没想到的一种**：容器里那一份另有名字（`最终幻想7.zip` 里装 `Final Fantasy VII (Japan).bin`）。转格式解出来就叫那个名字（`capability` 的 `unpack` 取内部条目名），条目 `<path>` 指 `Final Fantasy VII (Japan).bin`，封面落在 `downloaded_media/ps/covers/Final Fantasy VII (Japan).png`；照主库原名铺的 `最终幻想7.png` 不在，ES-DE 照 `<path>` 去掉扩展名找媒体，那一份它找不着主人。
    做法不另写：铺媒体本来就照 `Launch::file` 起名（票 18），转了格式的进了 `Launch`，封面跟着条目走。开着上面那个临时开关时这条红（left `["最终幻想7.zip"]`）。
- [x] 导出到主库那一侧条目照旧写主库里的原名
  - `crates/core/tests/sync_run.rs::导出到主库那一侧不转格式_条目照旧写主库里的原名`：同一份主库（`最终幻想7.zip` 里装另有名字的 `.bin`），两家都导出到主库。条目照旧写 `最终幻想7.zip` / `ps/最终幻想7.zip`；主库原有每一份的大小与修改时间都没变，也没多出解开的 `.bin`（ADR-0004）。钉子：导出那一侧交空的启动表（`converge::run`、`transfer::media_to_lay`），`Footprint::launching` 只有同步那条线调。这在 `3ea891d` 上本来就是这样，钉的是「以后谁往导出里接转格式就红」。票 18 那条 `导出到主库那一侧不生成播放列表_主库里只多出元数据文件` 照旧绿。
- [x] 门禁全绿
  - `/Users/nicoer/dev/game-wt/logs/q8-vs-20-gate1.log`：开头两行是 `/Users/nicoer/dev/game-wt/slot3`、`q8/vs-20`，末行 `EXIT=0`。带 `CARGO_INCREMENTAL=0`、`-j 3 --test-threads 3`。汇总表七条全绿：
    - fmt 1s
    - glossary 0s：扫了 8 份 `.rs` 里新写的 440 行，没撞上
    - check 15s
    - clippy 17s
    - test 1140s：带 `--no-fail-fast`，90 个测试目标；截图门在这一步里，照旧绿
    - numbers 4s
    - doc 17s
  - README 的数在这棵树上写回过：`cargo xtask numbers --write`，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-20-numbers.log`，`EXIT=0`。票数 190/219 → 191/219，测试条数 2,911 → 2,918（`sync_run.rs` 新加 7 条）。合并时由编排者在 `main` 上重写。
  - 审查用 `/code-review`，两轴只读，审的都是这棵树上的九份改动。采纳了这几条：
    - DC 那条只断条目头一行，不替 `Q1828` 钉死。
    - 补了「播放列表撞车被筛掉、退回碟 1 转出来那一份」的测试。
    - 首条测试名里「解成裸镜像」改成「从归档里解出来」：装的是 `.chd`，词表管它叫压缩镜像。
    - 改过的文件里三处真库数换成量级词：`adapter.rs` 的「12 个平台」、`converge.rs` 的「11 个」非游戏资产（指台账），`prepare.rs` 的「10 TiB」（指台账）。
    - `media.rs` 的「593/128」换成量级词。这组数台账没收，出处写成票 `rom-metadata-automation/20` 落地那次提交 `3aa8121`，挂单 `Q1256`。
    - `Q1829` 标 settled。
  - 没采纳的，都是审查自己标的判断题：
    - `Footprint::launching` 里「ROM 的源 → 落点」那张表与 `playlist::lay` 里的形状相近。两处筛的不一样（一处只要转了格式的），各五行，不值得为它在 `Desired` 上另立一问。
    - 「播放列表或转出来那一份 / 导出不转」那句话在几处文档里各说一遍。沿用票 18 的写法，每处都指向 `Footprint::launching`。
    - `sync_run.rs` 夹具照抄 `prepare` 的编排次序。这是票 18 起就有的样子，本票两边一起改了。

**收挂单 `Q1829`**（`.scratch/PARKING-LOT.md` 里那一条已由拿主意的人 2026-10-04 裁乙、托给本票；本票在它的状态行上补了「照裁定做完」）：条目照期望状态里主文件在卡上的落点写，四种都算：单碟转了格式的、没生成播放列表的多碟、播放列表撞车被筛掉的多碟、卡上有播放列表的（照旧指播放列表）。ES-DE 的封面照条目指着的那一份起名，导出到主库照旧写原名。`Q1828`（别的碟在前端里点不到）没碰。

本票用掉的挂单号：`Q1847`（转出来那一份放不进目标时条目写原名；根子是主文件没上卡的变体照样写进卡上的元数据）。`Q1848`–`Q1856` 没用。
