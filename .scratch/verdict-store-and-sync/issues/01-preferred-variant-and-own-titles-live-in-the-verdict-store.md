# 01: 首选变体与亲手加的叫法住进沉淀库

**What to build:** 人定过的**首选变体**、亲手给作品加的**叫法**，删库重扫之后还在。两样搬进沉淀库，锚在**路径锚**上（主库标识 + 位置，与人工纠正同族：熬得过删库重扫、熬不过改名挪目录，默认不导出）。
中立库里原来那两张表降为投影，开现场时照沉淀库重建（与合集、收藏同一个做法）；旧中立库里已有的，开现场时救进沉淀库一次、记下「搬过了」（照人工纠正那一次的先例）。

⚠️ 动沉淀库的顺序迁移清单：与 01→02→03→04→08→12 这条链上的票串着做，不并行。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q725`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写在今天 `main` 上红的测试（照「改名之后路径锚一个字节没动、沉淀库里的裁决照旧对得上」那条的写法）：定一个首选变体 → 删掉中立库重扫 → 首选变体还是它
  - 证据：`crates/core/tests/verdict_store.rs::定过的首选变体_删掉中立库重扫之后还是它`——中立库落在真实工作目录，删文件连 `-wal` / `-shm`，重开走 `Site::open_file`，扫描与识别真跑（裁决钉内容锚，规则挑汉化版、人定日版）。
    先红：编不过（`Site::set_preferred_variant` 不在）；临时换成今天的写入口 `site.catalog.set_preferred_variant` 跑过一遍，红在「删库重扫之后，前端默认启动的还该是人定的那一个」（`left: 汉化`、`right: 日版`），前提那几句都绿。
  - 真入口：`crates/cli/tests/pegasus.rs::导出时定的首选变体_删掉中立库重扫之后还在`（`export --prefer` → 删掉 `catalog/` → `scan` → 中立库里那份投影回来了）；把 `--prefer` 改回只写中立库验过红（`left: None`）。
  - 做法：沉淀库第 10 条迁移建 `preferred_variant`（键 = 主库标识 + 作品名 + 平台，值 = 变体的键）；人定走 `Site::set_preferred_variant` / `clear_preferred_variant`，先落沉淀库、再改投影；
    中立库那张表降为投影，`site::reconcile`（开现场与命令行 `scan` / `shape` 那两条路共用）照沉淀库整份重建，与眼下一样就一个字不写（`Catalog::replace_preferred_variants`）。写它的四处（界面点那一行、撤裁决、合并时人挑的平台、命令行 `export --prefer`）全部改走现场。
- [x] 同一写法：亲手加一个叫法 → 删库重扫 → 叫法还在
  - 证据：`crates/core/tests/verdict_store.rs::亲手加的叫法_删掉中立库重扫之后还在`（先红：编不过，`Site::add_own_titles` 不在）；`…::删掉亲手加的叫法_删库重扫之后它也不回来`（`title::suppress` 删裁决来源的叫法先删沉淀库那一条，先红：编不过）。
  - 做法：第 10 条迁移建 `own_title`（键 = 主库标识 + 中立库 `title` 表的键，源一律裁决不另存）；中立库里 `source = 裁决` 的行降为投影（`Catalog::replace_verdict_titles`，清与写同一个事务）。
    写裁决来源叫法的每一处都改走 `Site::add_own_titles`：「加进集合」、详情页改显示标题那一格、`UseTitle`、合并的 `keep_aliases` / `adopt`；撤走 `Site::remove_own_title` → `title::suppress`（挂单 `Q1549`）。`crates/core/tests/merge.rs::被合并作品的名字留作别名_源是裁决所以重折标题不碰它` 加断言：别名落进了沉淀库。
- [x] 挪目录之后这两样对不上（如实，不静默挂到别处）
  - 证据：`crates/core/tests/verdict_store.rs::挪了目录之后首选变体如实对不上_不挂到挪过去的那一个上`——日版挪进子目录（内容锚照样认得出是《魂斗罗》），首选照规则回到汉化版、`VariantDetail::preferred` 是 `None`、`preferred_unmatched()` 交出原来那个键，沉淀库那一条原样留着。
    界面作品页「首选变体」那一行对不上时说「裁决指定的「X」已不在这个作品底下」（挂单 `Q1548`，没有界面测试单独钉）。
  - **保留**：只对**首选变体**成立。**叫法**锚在「主库标识 + 作品名」上（中立库那张表的键），挪目录不改作品名，所以它照旧挂在那个作品上——同一条测试末尾断言它还在。票上「这两样对不上」对叫法是写错了，见挂单 `Q1547`（建议改成「首选变体对不上」）。
- [x] 旧中立库里已有的首选变体与叫法：开现场救进沉淀库一次，第二次开不重复搬
  - 证据：`crates/core/tests/verdict_store.rs::旧中立库里的首选变体与叫法_开现场时救进沉淀库一次_撤掉的不再回来`（先红：`left: None`，开现场之后沉淀库里没有）——撤掉之后再开，沉淀库与投影里都没有，旧行被记号挡住。
    结构版本对不上、开不进去的那一份：`…::结构版本对不上的旧库_列出来那一下就救进沉淀库_照提示删库重扫之后还在`（照 `Q726` 的先例；关掉救援那一支验过红）。
  - 做法：`site::carry_over_preferred_and_titles`（只读地读旧行 `Catalog::stranded_preferred_and_titles`，已有的不盖，记元数据键 `preferred_and_titles_carried_at`，旧行一行不删）；`site::rescue`（原 `rescue_shaping_overrides`，三样一起救，三处调用方跟着改）。
  - 沉淀库迁移：`crates/core/src/verdict.rs` 单元测试 `第九版的老库升上来_首选变体与亲手加的叫法按主库分开存得住也撤得掉`。
- [x] 导出沉淀库默认不带这两样
  - 证据：`crates/core/tests/verdict_store.rs::导出沉淀库不带首选变体与亲手加的叫法`（`include_path` 真假两档都不带，照 `Q722` 的裁决；两条内容锚的裁决作对照在导出里；别人导入之后一样都没有）。
  - **保留**：到达即绿，没经过红——`Store::export` 本来只折裁决与匹配裁决两张表，这条钉的是日后改格式时别顺手带上。
- [x] 不碰主库一个字节
  - 证据：`crates/core/tests/verdict_store.rs::定过的首选变体_删掉中立库重扫之后还是它` 在定首选变体、删库重扫一整趟前后逐字节比主库，一个字节不差。写的只有工作目录里的中立库与沉淀库。
- [x] 门禁全绿（本机 `sync_run` 那五条已知红除外，`Q479`）
  - 证据：`cargo xtask gate --keep-going -j 3 --test-threads 3`，日志 `/Users/nicoer/dev/game-wt/logs/q8-vs-01-gate2.log`（开头两行 `/Users/nicoer/dev/game-wt/slot1`、`q8/vs-01`），末行 `EXIT=0`、「7 条全绿」——
    fmt 2s、glossary 1s、check 0s、clippy 0s、test 915s、numbers 18s、doc 10s；test 那一步 88 个目标合计 2655 passed / 0 failed。本机默认盘上 `sync_run` 那几格如实跳过（`Q479` 的裁决之后本来就绿），没有已知红。
  - **保留**：头一趟 `q8-vs-01-gate1.log` 红在 `romcat-gui` 的 `snapshot`：开场屏「版本不兼容」那一行印的就是删库那句话，本票改了字、多折了一行，
    `opening/catalogs-light.png` / `catalogs-dark.png` 两张基线照新字重拍（`UPDATE_SNAPSHOTS=1`，只动这两张，拍出来核过：只是那段话长了一行）。字是删库那句话本身，没有别的改动；拿主意的人要过眼的话看这两张。

**删库那句话改成实话**（`CatalogError::Version`）：首选变体与亲手加的叫法挪到「一条不丢」那边，并说清人工纠正与首选变体钉在本机的位置上、改名挪目录之后对不上；
「会丢的」第三项换成还住在中立库里的「详情页上改过的年份、发行商、简介这类字段」（票 `02` 的活；显示标题那一格不在此列，见 `Q1549`）。`crates/core/tests/catalog_list.rs::结构版本对不上的库照列并说清是哪个版本对哪个版本` 按「会跟着丢的」切两半断言（先红）。
`crates/core/src/catalog.rs` 模块文档那段「那张挂账数漏了两样」同改。

**收挂单 `Q725`**（原票 `one-criterion-per-thing/07` 里第五轮已标 settled）：照裁定做了——首选变体与亲手加的叫法搬进沉淀库，按主库标识分开、默认不导出；中立库那两份降为投影、开现场照沉淀库重建；旧库里的救进来一次、记下「搬过了」。叫法那一半的锚与「熬不过挪目录」那半句见 `Q1547`。

**给票 02 的做法**（照抄）：写在 `crates/core/src/site.rs` `reconcile` 的文档「一样人定的东西怎么从中立库搬进沉淀库（做法）」四步里——沉淀库追加一条迁移（中立库那张表的列 + `library` + `decided_at`）、`Site` 上一个先落沉淀库再改投影的写入口、`reconcile` 里一行 `replace_*` 投影、一个搬家记号键加 `carry_over_*` 与 `rescue` 里一支、删库那句话挪边。

挂单：`Q1547`–`Q1551`。
