# 人写下的留得住，写到卡上的前端读得到

**Status:** 待 `/to-spec`
**Opened:** 2026-09-23（`/settle` 第五轮收口 → `/grill-with-docs`）
**来源：** `.scratch/PARKING-LOT.md` 第五轮收口里无家可归的那摊活；条目正文在各原票末尾「挂单裁决（第五轮收口）」一节

## 它建什么

人拍过的板还住在可再生的中立库里、或记不全（首选变体、修订、例外、标题叫法）——删库重扫就丢（第一组）；同步写到卡上的东西落点不对或缺一样（ES-DE 读不到、没有 `.m3u`、连着纠正要等 N 趟）（第二组）；设置屏上画着的几样背后核心库没有能力（账号落盘、ffmpeg、占用多大）（第三组）。
⚠️ 核查顺带发现：删库那句话（`catalog.rs:370-376`）说「沉淀库里的裁决一条不丢」，而详情页的字段修改住在中立库、没被列进会丢的那几项——等于说了假话，归 `Q921`。

## 23 件（形状已裁定的直接照写）

## 第一组：沉淀库收下人手写的东西

### `Q921` — 根因：作品详情页上的手动修改、手加的名称、首选变体裁决都落在中立库，删库重扫就丢

**上游已答，只差动手：** ADR-0001 修订「不可再生的东西不住在中立库里」（理由对三样逐字适用，且写明「这条前提以后要主动守」）；CONTEXT.md:159 中立库「不可再生的东西一旦放进来……该问它是不是本该住在沉淀库」；CONTEXT.md:262 沉淀库装「裁决、收藏、人工纠正」。键的先例：沉淀库 title_suppression（verdict.rs:373，锚作品名）、not_same_work（:455，主库标识+作品名）。。
核查（2026-09-23）：三样人手写的仍落中立库：字段裁决 crates/gui/src/browse/work.rs:434 save_meta→:469 put_verdict_value（crates/core/src/catalog/scrape.rs:1123，中立库刮削值表）、work.rs:1873 apply_meta；手加名称 crates/gui/src/browse.rs:2426 add_title→:5320 write_title→catalog/title.rs:180 put_titles；首选变体 browse.rs:2513→catalog/frontend.rs:248 set_preferred_variant。沉淀库 verdict.rs:229-470 的表里没有这三样。另：删库那句话 crates/core/src/catalog.rs:370-376 只逐项列了「首选变体与亲手加的叫法」，没列详情页的**字段裁决**，反而说「沉淀库里的裁决……一条不丢」——对这一样说了假话。
依赖／可并：Q725（同一件事，Q725 是其中两样+键的选择题，先裁它）、Q682（变体锚若走内容锚，要先定代表那一份）
（大小 L，来自票 `gui-looks-like-the-design/15`）

### `Q725` — 中立库里还住着两样人亲手定的东西：首选变体、亲手加的叫法

**已裁定（2026-09-23 grill）：** 路径锚（与人工纠正同族）：熬得过删库重扫、熬不过改名挪目录，默认不导出。
核查（2026-09-23）：catalog/frontend.rs:248 set_preferred_variant 写中立库 preferred_variant(work,platform,variant_key)；title 表 source=裁决 的行由 browse.rs:5320 write_title 写；catalog.rs:103-106 文档与 :375 删库那句话点名它俩「得重新定一遍」。
依赖／可并：Q921（并成一张票）、Q682（若选 B）
（大小 M，来自票 `one-criterion-per-thing/07`）

### `Q964` — 「已经就地裁掉哪一组」只活在这个进程里：窗口关掉再开，那几项不再标「已通过」，细分方式也不再锁着

**上游已答，只差动手：** CONTEXT.md:216「一部分」：「只要还有一部分裁过，这一批的细分方式就锁在当初那个轴上」——无「同一个窗口内」的条件；否则「屏上的数当场变成假话」。B（塞进 summary 再 parse）被 ADR-0024（D142 那个形状）否掉。只剩 A：批表加结构化作用范围列（沉淀库顺序迁移）。。
核查（2026-09-23）：crates/gui/src/queue.rs:152 parts: Parts 只在进程里，:786 refresh_records 只 keep 在册批号；沉淀库批表 crates/core/src/verdict.rs:299-311 verdict_batch / :872-887 Batch 没有「作用范围（形状+轴+组名）」这一格。
依赖／可并：Q973（都想给「一批」加一格；表不同——Q964 是沉淀库 verdict_batch，Q973 是中立库 sublibrary_exception——不必同票）
（大小 M，来自票 `gui-looks-like-the-design/19`）

### `Q1010` — 合并作品落成的裁决把 DAT 条目名里的**修订标记**丢了（`release.revision` 过不来）

**已裁定（2026-09-23 grill）：** 带：裁决多记一列 DAT 修订，沉淀库顺序迁移，导出格式升一版。
核查（2026-09-23）：crates/core/src/triage/merge.rs:313-356 facts_of 从 release 抄平台/地区/序列号/语言、version 取 catalog.decided_edition（identification.edition），无 revision；verdict::Facts（crates/core/src/verdict.rs:724-741）没有 revision 格；crates/core/src/identify.rs:3767 verdict_release 在 :3808 写 revision: None。详情页 crates/core/src/catalog/detail.rs:235-239 edition() 回退链第二档读 release.revision，于是合并后退回「—」。
依赖／可并：Q994（同一条「第几版」链，先答 Q994 的领域题再定列名）
（大小 M，来自票 `gui-looks-like-the-design/16`）

### `Q994` — 卡带内部头读出来的版本号仍只躺在 JSON 里，与「第几版」那条链不通

**已裁定（2026-09-23 grill）：** 不接「第几版」那条链，只作为修订的证据写进依据与详情（词表**第几版**条已补）。
核查（2026-09-23）：三件事在代码与词表里各住一处：①卡带/光盘头版本号——crates/core/src/identify/cart.rs:233 Facts::version（:457 GBA 0xBC、:549 GB 0x14C mask ROM version、:705 SFC 头+0x1B 等，一律折成 "v{byte}"）、光盘 identify/disc.rs:429（NGC/Wii 头第 7 字节）、identify/ident.rs:111 Ident::version；落 content_cart.facts / content_switch.facts JSON（catalog/identify.rs:164,190），无独立列、不进任何链；词表无名，且 CONTEXT.md:248 第几版 _Avoid_ 了「版本、Version、修订号」。②DAT (Rev N)——identify/naming.rs:14,57 revision→release.revision；词表 CONTEXT.md:242「第几版」上一层＝发行版的**修订**。③汉化第几版——沉淀库 verdict 表 version 列（verdict.rs:251）/ verdict::Facts::version（:739）→ identification.edition（catalog/identify.rs:2902 decided_edition）；词表 CONTEXT.md:243 下一层＝变体的第几版，只有人说得出。链在 catalog/detail.rs:235-239（裁决 > 修订 > 说不出）。另有第四个被明令排除的：filename::Parsed::version（detail.rs 文档、CONTEXT.md:245）。结论：①与②是**同一层（发行版的修订）的两种证据**——头里那个字节是官方重发时改的，No-Intro 的 (Rev 1) 多数正对应头版本 1；但编码不一（v0 在 DAT 里是「没有标记」，Rev A/v1.1 各平台写法不一），汉化补丁通常不改它，所以在汉化变体上它说的是「打在哪一版原盘上」，与③完全不是一件事。
依赖／可并：Q1010（同一条链；先定此题再定 Facts 加不加 revision）
（大小 S，来自票 `gui-looks-like-the-design/34`）

### `Q682` — 「谁代表这个变体」（裁决的内容锚）照旧可能挑中捎带的 BIOS

**已裁定（2026-09-23 grill）：** 直接改、不迁：代表那一份先跳过非游戏资产（真库 0 条裁决）。
核查（2026-09-23）：crates/core/src/identify.rs:2171-2184 ordered()：主文件优先、同为主文件取大的，未跳过非游戏资产；消费者 representative（:2168）→content_print / find_verdict。
依赖／可并：Q725（若首选变体选内容锚）、Q921
（大小 S，来自票 `one-criterion-per-thing/04`）

### `Q713` — 标题这一侧的待确认队列眼下只是报告里一个数与十个例子，没有一条能一条条裁的列表

**已裁定（2026-09-23 grill）：** 待确认屏加一档「标题」，一条 = 一串还没人裁过的中文叫法（肯定/否定整串字）；词表**待确认队列**已放宽成两档。
核查（2026-09-23）：crates/core/src/title/report.rs:133-135 queue_works / queue_examples 只在报告里（:201-202 累计，:362-364 印），读它的只有 crates/core/tests/titles.rs；gui/cli 源码 0 引用。
依赖／可并：Q714（先改 ADR-0002/词表口径，再定队列）
（大小 L，来自票 `one-criterion-per-thing/02`）

### `Q714` — ADR-0002 与词表「待确认队列」的原话按置信度分档，两侧队列的判据却已不看置信度

**上游已答，只差动手：** 判据已由票 one-criterion-per-thing/02、规格六与 grill 第 79 行（D128）裁定；只差把 ADR-0002 补一段修订、给词表「待确认队列」补半句。。
核查（2026-09-23）：docs/adr/0002-confidence-tiers-and-triage-queue.md:3 仍按高/中/低置信分档（无相关修订段）；CONTEXT.md:199-200「置信度不足以自动通过的候选的集合」；代码 crates/core/src/triage.rs:741-743 in_queue = accepted==0；crates/core/src/title/report.rs:382 judged（没人裁过就进）。
依赖／可并：Q713（先做它）
（大小 S，来自票 `one-criterion-per-thing/02`）

### `Q973` — 手动例外只撤得掉一条，撤不掉「刚加进来的那一整个作品」

**已裁定（2026-09-23 grill）：** 手动例外按作品折一层，行尾一颗撤整组；分组判断放核心库，不动表结构。
核查（2026-09-23）：crates/gui/src/sublibrary.rs:4923 exception_table_ui 一行一颗撤销，:4466 undo_exception→Catalog::clear_exception（crates/core/src/catalog/sublibrary.rs:1158，一次一个变体键）；加入侧 :4407 add_exception / 票 23 的「加入子库…」→ set_exceptions（sublibrary.rs:1115）整批落。票 23 之后一次可记几十上百条。
依赖／可并：Q1014（同一处 set_exceptions 入口与例外表，宜同票或先做 Q1014）
（大小 M，来自票 `gui-looks-like-the-design/22`）

### `Q1022` — 「被修改过」那一栏稿上有一颗「以设备上的版本为准，更新清单」，没做

**已裁定（2026-09-23 grill）：** 做**收回清单**（词表新词）：带审计（谁、何时、哪几份），审计住沉淀库，屏上先确认一层。
核查（2026-09-23）：crates/gui/src/sublibrary.rs:2922 surprises_ui 没有「以设备上的版本为准，更新清单」；全仓 grep「以设备上的版本为准」0 命中。ADR-0015:17「只删清单里记录过的、由工具自己导出的文件」；CONTEXT.md:432-433 清单是工具在目标设备上的行为边界。
依赖／可并：Q1033（都改清单语义/落点，写清单的同一处）
（大小 M，来自票 `gui-looks-like-the-design/24`）

### `Q1014` — `sublibrary/exceptions-{light,dark}` 是一对**挂钟依赖的脆基线**：例外那两行的次序随机器忙不忙换

**上游已答，只差动手：** 条目自带修法（照 mark_exported_at 加 set_exceptions_at，夹具钉互异定值），编排者已排给票 22 线。。
核查（2026-09-23）：crates/core/src/catalog/sublibrary.rs:1127 set_exceptions 每次调 now_secs()；:1228 排序 b.at.cmp(&a.at) 再按键；夹具 crates/gui/tests/snapshot.rs:3446-3474 逐条调 :3057 记一条例外→set_exception。带显式时刻的先例 crates/core/src/catalog/export.rs:261 mark_exported_at。全仓无 set_exceptions_at。
依赖／可并：Q973（同一入口）
（大小 S，来自票 `gui-looks-like-the-design/16`）

### `Q703` — 票据说不清的 Switch 补丁照旧导出：没有票据的更新包、装着不止一档的整合包

**已裁定（2026-09-23 grill）：** 不当——Switch 补丁判据维持只认票据。
核查（2026-09-23）：crates/core/src/identify/scope.rs:227-244 title_kind：只认票据（Base 优先；恰一份 .cnmt.nca 且票据一致才照票据），否则 None 退回名字那条→可运行照旧导出。
（大小 M，来自票 `one-criterion-per-thing/05`）

## 第二组：同步落点

### `Q945` — ES-DE 2.0 起默认只读 `~/ES-DE/gamelists/`：子库写在卡上的 `gamelists/<平台目录>/gamelist.xml` 默认可能不被读

**已裁定（2026-09-23 grill）：** 先在真掌机上核实 ES-DE 默认读哪、`LegacyGamelistFileLocation` 开没开；核实后让设备档案记「ES-DE 数据目录」写过去。**核实那一步要拿主意的人拿设备做**。
核查（2026-09-23）：crates/core/src/adapter/gamelist.rs:497-502 metadata_path = gamelists/<平台目录>/gamelist.xml，相对子库/导出根落在卡上；设备档案 capability/profiles.toml:324 起 ES-DE 段无「ES-DE 数据目录」一项。
依赖／可并：Q1033（落点一类）
（大小 M，来自票 `gui-looks-like-the-design/21`）

### `Q1046` — 多碟合成之后的主文件只能是已有的成员，稿上那份 `.m3u` 播放列表没有

**已裁定（2026-09-23 grill）：** `.m3u` 只在同步时生成到卡上（清单照管），导出到主库那一侧不生成。
核查（2026-09-23）：crates/core/src/shape/fix.rs:70 merge 取键最小成员的主文件；adapter/sync 无「多碟生成播放列表」概念（m3u 只在 crates/core/src/adapter/gamelist.rs:534 的注释示例与 capability/profiles.toml:71,261 扩展名表里出现）。
依赖／可并：Q1033（同一落点层，宜同票）、Q1045（多碟合成的上游）
（大小 M，来自票 `gui-looks-like-the-design/29`）

### `Q1045` — 按一下纠正就**全库重新成型**一趟，没有「只重算这一处」

**已裁定（2026-09-23 grill）：** 攒着：成型纠正先落沉淀库，屏上累计「N 处待生效」，人按一下跑一趟全库。
核查（2026-09-23）：crates/gui/src/shaping.rs:80 RESHAPE_TASK="重新成型 · 全库"；crates/core/src/shape.rs:242 plan 是整份条目表的纯函数；crates/core/src/catalog/content.rs:650 replace_variants 整批换。
依赖／可并：Q1046（同来自票 29 的多碟合成）
（大小 M，来自票 `gui-looks-like-the-design/29`）

### `Q1029` — 勾一下「补回」会让台上那一趟白跑；「放不进目标」那一行的容量把撞车的几份各算一遍

**上游已答，只差动手：** 条目自带三处修法，谁来裁写的是编排者；(3) 由 ADR-0024（消费者不许再判一遍）定了方向：进 --json。。
核查（2026-09-23）：(1) crates/gui/src/sublibrary.rs:968-977 set_restore_missing 先 invalidate 再 preview，被弃那趟不停；(2) crates/core/src/sync.rs:680 rejected_tally 撞车几份各算一遍、屏上无口径；(3) sync.rs:748-749 Collision derive Serialize 但 :698 collisions() 是方法，--json 里没有。
（大小 S，来自票 `gui-looks-like-the-design/24`）

### `Q587` — `romcat export` 只在开着 `--media` 时接 Ctrl-C，不开时照旧按不停

**上游已答，只差动手：** CONTEXT.md:573-581 收场四档/部分完成；元数据那一段已有「没走完」口径（transfer::export_task，界面在用）。条目自带修法。。
核查（2026-09-23）：crates/cli/src/main.rs:2812 run_export，约 :2881-2884 `if args.media { Handle::with_cancel(cancel.clone()) } else { Handle::new() }`；main 的中断处理 :1069 对导出也印「正在收尾并保存断点」。
依赖／可并：Q588（同一口径：收场档）
（大小 S，来自票 `one-criterion-per-thing/08`）

### `Q588` — 同步连着失败主动停了时不报「没走完」，任务台记成完成；导出铺媒体那一边记成没走完

**上游已答，只差动手：** CONTEXT.md:579 部分完成：「活自己收的手（刮削撞上配额、同步连着失败太多次）是另一种」——逐字点名同步。。
核查（2026-09-23）：crates/core/src/sync/execute.rs:483 out.gave_up=true，但 :494-498 只在 out.interrupted 时 task.halfway；导出铺媒体那边已记 halfway（crates/core/tests/pegasus.rs 铺媒体连着没铺成…）。
依赖／可并：Q587
（大小 S，来自票 `one-criterion-per-thing/08`）

## 第三组：设置屏背后的核心能力

### `Q1063` — ScreenScraper：账号存不下来、连接测不了、两条配额留不住

**已裁定（2026-09-23 grill）：** ScreenScraper 账号存工作目录里一份仅本机可读（0600）的文件，只一套；「测试连接」扣不扣配额另问。
核查（2026-09-23）：crates/core/src/scrape/online.rs:312-321 Credentials::from_env 只认四个环境变量、无落盘；无 ping/verify；:249-251 requests_left/ko_left 只存剩余、只在联网刮削跑着时有（:554-557 observe）；:35「没有任何一处能装第二套凭据」。
依赖／可并：Q1062、Q1066（程序级落盘同一处）
（大小 M，来自票 `gui-looks-like-the-design/31`）

### `Q1066` — ffmpeg：状态与重新检测接上了，**路径还指不了**

**已裁定（2026-09-23 grill）：** 核心库找 ffmpeg 时除 PATH 外再找几处常见位置，不落盘。
核查（2026-09-23）：crates/core/src/scrape/preview.rs:375 probe(program) 只在进程 PATH 里找；:550 Loader::set_program、crates/core/src/scrape/pool.rs:153 MediaPool::probing_with 是测试接缝，无 getter、不落盘。注意：从 Finder 启动的 macOS .app 的 PATH 不含 /opt/homebrew/bin、/usr/local/bin，Homebrew 装的 ffmpeg 在 GUI 里会报「没找到」。
依赖／可并：Q1062、Q1063（若选 A，共用程序级落盘）
（大小 S，来自票 `gui-looks-like-the-design/31`）

### `Q1064` — 工作目录与媒体池的占用明细算不出来，改成报条数

**上游已答，只差动手：** ADR-0024：「工作目录里哪些算数」是领域判断，只许在核心库；条目已写明做法：核心库加「量工作目录各件多大」的面。。
核查（2026-09-23）：全仓无目录求和（无 walkdir；read_dir 只用于扫描与大小写探测）；MediaPool 公开面无「几份、多大」；GUI 设置屏改报条数（票 31）。
依赖／可并：Q995（同一遍历媒体池）
（大小 S，来自票 `gui-looks-like-the-design/31`）

### `Q995` — 老库里入过池的媒体那三格永远空着，没有补齐的入口

**上游已答，只差动手：** 条目自带做法：任务台上一条「量媒体池尺寸」的活，不放开库路上。。
核查（2026-09-23）：crates/core/src/catalog/scrape.rs:168-170 add_column media.width/height/duration_ms 不回填；Catalog::put_media 只「空着才补」；只在 crates/core/src/scrape/pool.rs:416 adopt_into 入池时量。
依赖／可并：Q1064（都要把媒体池走一遍，可共用一个遍历面）
（大小 S，来自票 `gui-looks-like-the-design/34`）

### `Q1062` — 数据源那两颗「每周自动检查更新」开关：核心库没有这条路

**已裁定（2026-09-23 grill）：** 不做；把设置屏稿上那两颗「自动检查更新」开关划掉。
核查（2026-09-23）：crates/core/src/sources.rs:256 只有手动 refetch、:126/143 survey；全仓 auto_check/自动检查 0 实现；crates/gui/src/settings.rs:24-25 文档与 :629 屏上现话「还不会自动检查更新」。
依赖／可并：Q1063、Q1066（都要「程序级设置落盘」，若做应共用一处）
（大小 M，来自票 `gui-looks-like-the-design/31`）
