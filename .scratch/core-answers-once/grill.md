# 平台、拒绝话与读不动，都由核心库答一次

**Status:** 待 `/to-spec`
**Opened:** 2026-09-23（`/settle` 第五轮收口 → `/grill-with-docs`）
**来源：** `.scratch/PARKING-LOT.md` 第五轮收口里无家可归的那摊活；条目正文在各原票末尾「挂单裁决（第五轮收口）」一节

## 它建什么

三件事同一个病：**同一个问题在几处各答各的**。平台有目录声明、识别判定、人工纠正三种说法，报告、落点、体检各取一种（第一组）；核心库的拒绝话里夹着 `romcat …` 命令，画到屏上就是终端命令，界面只好自己再写一句（第二组）；「读不动」在好几处被压成「没有」或「坏了」，人会走错下一步（第三组）。

## 31 件（形状已裁定的直接照写）

## 第一组：平台跟着内容走

### `Q1030` — 「目录与内容平台不符」在核心库有**两处**判据，而稿上打头的 GB↔GBC 两组今天一条都出不来

**已裁定（2026-09-23 grill）：** 算，照稿：新判据 = 扩展名 ∪ 卡带头家族 ∪ CGB-only；库体检那一格的数与基线要重出。
核查（2026-09-23）：两处判据仍在：crates/core/src/scan/aggregate.rs:658 conflicting_platform（按扩展名，库体检/平台纠正用）与 crates/core/src/catalog/identify.rs:724-726 CONFLICT_FROM（按卡带头家族，识别报告用）；GB/GBC 两处都明说不算冲突（identify.rs:174、:724），稿上的 GB↔GBC 两组出不来
依赖／可并：Q605、Q606、Q602、Q1031
（大小 L，来自票 `gui-looks-like-the-design/28`）

### `Q605` — 识别与两份报告里还有四处拿目录声明的平台，没跟着改

**已裁定（2026-09-23 grill）：** 刮削报告与识别报告按**识别判定**的平台分组，报告头写明口径。
核查（2026-09-23）：crates/core/src/identify.rs:3489-3491 rank 的 platform_matches 仍比 variant.platform；crates/core/src/catalog/scrape.rs:909 source_by_platform 按 COALESCE(v.platform,…) 分组；crates/core/src/catalog/identify.rs:2559 for_each_identification 取 v.platform
依赖／可并：Q602、Q1030、Q606
（大小 M，来自票 `one-criterion-per-thing/03`）

### `Q606` — 「识别判定的平台」词表里没有，没往 `CONTEXT.md` 加

**纯活：** CONTEXT.md 平台条下立目录声明/识别判定/人纠正的平台三个说法及各处按哪个算。
核查（2026-09-23）：CONTEXT.md:102-104 平台条仍只一句「目录只是强先验，文件内容可以推翻它」，没立「目录声明的平台 / 识别判定的平台」；平台纠正已成词（:74、:273、:422），三个说法齐了
依赖／可并：Q605（报告按哪个算定了再写）、Q1030
（大小 S，来自票 `one-criterion-per-thing/03`）

### `Q1031` — 平台清单：界面一律用内置那一份，命令行走工作目录那条链

**上游已答，只差动手：** ADR-0024（链长在命令行、界面够不着：提进核心库一处，如 Manifest::in_workspace）；读不动时 ADR-0021（第三态，不静默退回内置）。
核查（2026-09-23）：命令行 crates/cli/src/main.rs:486-494 ManifestArgs 先 --manifest、再工作目录 platforms.toml、再内置；界面一律 Manifest::builtin()，已扩散到 7 处：crates/gui/src/health.rs:285、:1342，stages.rs:1610，shaping.rs:609，tokens.rs:1323，browse/work.rs:619、:630，browse.rs:4485。核心先例：capability::Roster::in_workspace
依赖／可并：Q976（同为「工作目录里那份配置读不动就静默退回内置」，一起改）、Q1030（库体检那格的数依赖清单）
（大小 M，来自票 `gui-looks-like-the-design/28`）

### `Q1033` — 导出与同步还不读平台纠正：照旧按目录放置

**已裁定（2026-09-23 grill）：** 只认人工平台纠正：落点平台 = 有纠正听纠正，否则照目录；在核心库立成一处。
核查（2026-09-23）：导出：crates/core/src/adapter.rs:432 与 crates/core/src/sync.rs:1642 用 path::platform_of_key（crates/core/src/path.rs:355，键里那级目录）；同步：crates/core/src/sublibrary.rs:1006、:1191 platform: variant.platform（目录声明）。判定平台已有物化结论 Catalog::identified_platforms（crates/core/src/catalog/identify.rs:2933）。屏上已改口 crates/gui/src/health.rs:78「导出与同步眼下仍按目录放置」。
依赖／可并：Q1046（同一处 adapter/sync 落点）、Q945（同为落点）、Q1022（清单旧落点清理）
（大小 L，来自票 `gui-looks-like-the-design/28`）

### `Q783` — 街机那条标题覆盖管不到已经认出作品的显示标题，屏上照实说了

**已裁定（2026-09-23 grill）：** 要：有平台语境的地方（导出、详情按变体平台）按平台重挑显示标题，ADR-0010 的 MAME 权威落到认出作品的街机上。
核查（2026-09-23）：crates/core/src/title.rs:486-487 仍「平台传 None……按平台的覆盖是导出到某个平台的子库时才用得上」；crates/core/src/adapter/converge.rs:735-740 标题那支有 chosen 就不看平台
依赖／可并：Q976（同一份优先级表）
（大小 M，来自票 `gui-looks-like-the-design/30`）

## 第二组：核心库只说事实，两个壳各补去处

### `Q622` — 被后来的一批盖住时那句话原样画，里面带着一句命令行指路

**已裁定（2026-09-23 grill）：** 核心库那句只说事实与去处的名字、不带命令，结构化字段交出来；命令行在自己那层补命令，界面补屏上去处。**一族一起收**（`Q643` `Q615` `Q851` 同形）。
核查（2026-09-23）：crates/core/src/triage.rs:120 TriageError::CoveredBy 的措辞仍带「（`romcat triage undo --batch {by}`）」，界面原样画；同形的还有 crates/core/src/sync/prepare.rs:133、:275（`romcat capability` / `romcat sublibrary set …`）与 Q643 的 transfer.rs:952
依赖／可并：Q643、Q851、Q615（同一族核心措辞，一票收）
（大小 M，来自票 `gui-answers-all-six/06`）

### `Q643` — 撞上时每一份底下画的「为什么」，有一种把人支回终端：先 `romcat import`

**上游已答，只差动手：** ADR-0023:32（import 是一次性迁移，明文留在命令行）——所以「界面补导入」这条路已被否，剩下只是那半句由谁说。
核查（2026-09-23）：crates/core/src/adapter/transfer.rs:952 仍写「先 `romcat import` 把它读进来…再导出」；界面 crates/gui/src 里没有一处调导入
依赖／可并：Q622 的题（核心库拒绝的话里带命令行指路，怎么拆）
（大小 S，来自票 `gui-answers-all-six/05`）

### `Q851` — 核心库「目标不在位」那句带着命令行命令与系统错误原文，界面该不该一律用自己的话

**上游已答，只差动手：** ADR-0024（界面 target_absent 是第二个判据苗头：判断提到核心库，Fit::Unknown 带结构化原因）。
核查（2026-09-23）：crates/core/src/sublibrary.rs:610-618 Fit::Unknown 只有 `why: String`；:733 Cutoff::Failed(why) 直接塞进去；界面 crates/gui/src/sublibrary.rs:5599 target_absent 自己再查一眼「在不在位」来挑措辞（:795、:1256、:1614）
依赖／可并：核心库措辞去命令行指路那一族（Q622 的题）、Q797
（大小 M，来自票 `gui-looks-like-the-design/20`）

### `Q615` — 向导第一步拦下时说的是核心库原话，「换个名字，或者去开那一份」那半句没了

**纯活：** 核心 AlreadyExists 补「换个名字或去开那一份」，空白那句建库时不带路径。
核查（2026-09-23）：crates/core/src/catalog.rs:418 AlreadyExists 仍是「中立库 {path} 已经在了——建库不会去开现成的那一份」无下一步；:405 BlankLibraryName 仍带按空名字折出的文件路径；向导 crates/gui/src/claim.rs:436 照印。等的票 gui-looks-like-the-design/05 已合
依赖／可并：Q841（同改 CatalogError 措辞）、Q622 的题
（大小 S，来自票 `no-mute-spots-opening-a-catalog/04`）

### `Q841` — 旧库版本对不上那句核心库原话，在开场上折成六行，比别的行高出一截

**纯活：** CatalogError::Version 交出短话与细节，开场行印短话、细节挂悬停并重批基线。
核查（2026-09-23）：crates/core/src/catalog.rs:370-384 CatalogError::Version 仍是一整句五分句长话（带「会跟着丢的有三样」）；crates/gui/src/opening.rs:532 row_ui 照印
依赖／可并：Q615（同为 CatalogError 措辞，一处改）、开场有库那两张基线重批
（大小 S，来自票 `gui-looks-like-the-design/05`）

### `Q797` — 子库记着的前端格式这一版没有适配器，照旧在排差量预览开跑之后才判出、记失败

**已裁定（2026-09-23 grill）：** 做：核心库给子库长 `adapter()`，排差量预览按下那一刻问它、屏上说、不排；界面两处各自 `find` 的地方改问它。
核查（2026-09-23）：crates/core/src/sync/prepare.rs:338 仍在排差量预览里才 adapter::find 失败；Sublibrary 没有 adapter()（只有 crates/core/src/catalog/export.rs:161 ExportSetup::adapter）。但条目前提已变：界面「前端格式」现在是 adapter::names() 摆的分段选择（crates/gui/src/sublibrary.rs:3705-3726），不再手打；命令行 sublibrary set 在 crates/cli/src/main.rs:4342 已当场拦错格式——只剩「旧库里存着这版没带的格式」一种来路
依赖／可并：Q851（同在 prepare_selected 一带）
（大小 S，来自票 `gui-looks-like-the-design/07`）

### `Q944` — 命令行 `romcat sublibrary set` 没走新的目标路径判断：只拦主库，工作目录与别的子库占着都放行

**上游已答，只差动手：** ADR-0024（命令行改调同一处 target::vet）。
核查（2026-09-23）：crates/cli/src/main.rs:4397 起 sublibrary set 那一支没调 target::vet / vet_name（全文件零处 vet( ），只在同步时走 refuse_target_in_library(:5039)；判断在 crates/core/src/sublibrary/target.rs:140、:186
依赖／可并：Q797（同是命令行/界面在 sublibrary set 一处对齐）
（大小 S，来自票 `gui-looks-like-the-design/21`）

### `Q574` — 命令行只把长命令收场那一句接到 `Ending::render`，报告里的「已中断」「这一趟被中断了」没改

**上游已答，只差动手：** CONTEXT.md 部分完成条（:581 `_Avoid_: 停在半路、中断、半截…`）。
核查（2026-09-23）：crates/core/src/report/render.rs:259、:349、:800，report/duplicates.rs:90，sync/report.rs:667「这一趟被中断了」，crates/cli/src/main.rs:1537 仍用「中断」
依赖／可并：Q626、Q655、Q454
（大小 M，来自票 `gui-looks-like-the-design/03`）

### `Q626` — 一批裁决的时刻，命令行与界面各有一份折法

**上游已答，只差动手：** ADR-0024；human_time 自己的文档（render.rs:138-142）。
核查（2026-09-23）：crates/cli/src/main.rs:4001 fn when 自折公历（注释「整个仓库一个时刻都不往外印」已失实）；crates/core/src/report/render.rs:144 human_time 注释「公式只有一处」
依赖／可并：Q655、Q574、Q454（都是命令行输出一票）
（大小 S，来自票 `gui-answers-all-six/06`）

### `Q655` — 铺媒体那句回执界面与命令行各写一份，已经差着一截

**纯活：** MediaReport 上立一句渲染，界面与 CLI 导出收场都读它。
核查（2026-09-23）：crates/gui/src/stages.rs:131 media_laid 与 crates/cli/src/main.rs:2911 「媒体铺出去 … 份」各写一份；MediaReport（crates/core/src/adapter/report.rs）上没有一句话的渲染
依赖／可并：Q574 / Q626（同为命令行输出改走核心库那一份，可一票）
（大小 S，来自票 `one-criterion-per-thing/09`）

### `Q454` — 命令行有两句把主库标识（带哈希）当成给人看的名字印出来

**上游已答，只差动手：** CONTEXT.md 主库标识条（不是给人看的名字）。
核查（2026-09-23）：crates/cli/src/main.rs:3894 与 :4101「主库「{}」上还没有落过一批裁决」填的仍是 site.library_identity；crates/core/src/site.rs:179 已有 display_name()
依赖／可并：Q626、Q574、Q655
（大小 S，来自票 `no-mute-spots-opening-a-catalog/01`）

### `Q1067` — 「关于」那一节的许可没有结构化字段，只说得出「以各家自己的说明为准」

**纯活：** registry::Source 加 license 字段（数据取自 sources.toml），设置屏「关于」印出来。
核查（2026-09-23）：crates/core/src/dat/registry.rs:139-149 Source 只有 name/origin/shape/convention/note，无 license；许可散在 crates/core/src/dat/sources.toml:68、:111-112 等注释
依赖／可并：Q1074（同为「关于/设置」那一屏由核心库出词）
（大小 S，来自票 `gui-looks-like-the-design/31`）

### `Q1074` — 「未下载 / 读不动」那两个词两处各写一份

**上游已答，只差动手：** ADR-0005 修订段 / look「词由核心库挑」先例。
核查（2026-09-23）：crates/gui/src/settings.rs:861-866 与 crates/gui/src/roots.rs:1374-1375、:1453 各自 match crates/core/src/sources.rs:30 SourceState 写「未下载/读不动」
依赖／可并：Q1067
（大小 S，来自票 `gui-looks-like-the-design/31`）

### `Q949` — `Accepts::takes` 眼下一个调用方都没有，而且多段扩展名会判错

**纯活：** 删掉 Accepts::takes，只留 takes_key。
核查（2026-09-23）：crates/core/src/capability.rs:166 `pub fn takes` 仍在，全仓零调用方（grep `.takes(` 无结果）
（大小 S，来自票 `gui-looks-like-the-design/21`）

### `Q984` — 两轴审查挑出的三处形状，本票按住没动（都与别的树在改的文件重叠）

**纯活：** impact 并成 impact(ui,tone,parts)、(字,强调)具名类型、Entries::of 答 EntryKey。
核查（2026-09-23）：crates/gui/src/look.rs:1232 impact 与 :1241 impact_warn 仍两支、parts 仍是 `&[(&str,bool)]`；crates/core/src/adapter/converge.rs:602 Entries::of 仍答 `(String, Anchor)`，crates/core/src/catalog/roots.rs:447 当 BTreeMap<(String,Anchor),bool> 键，converge.rs:512 另摆成 BTreeMap<String,BTreeMap<Anchor,_>>。等的票 22 已合
（大小 S，来自票 `gui-looks-like-the-design/26`）

### `Q852` — 删减建议只带变体的键，没有作品名与平台

**上游已答，只差动手：** ADR-0024（界面凑作品名是另写一份「这个变体叫什么」的判断）。
核查（2026-09-23）：crates/core/src/sublibrary.rs:578-588 Trim 仍只有 variant/bytes/cumulative；界面 trim_ui 照印键
依赖／可并：Q819（同一处 plan 折 trim_suggestions，可一票）、浏览屏「认不出作品显示正题」那一处取法
（大小 M，来自票 `gui-looks-like-the-design/20`）

### `Q819` — 删减建议的「累计腾出」按计划器记账，作品共用的媒体记在头一个变体名下，「排除到这一项就能放下」可能偏乐观

**已裁定（2026-09-23 grill）：** 改准：计划器把作品级媒体记在作品名下，该作品的变体全被排除才算腾出。
核查（2026-09-23）：crates/core/src/sublibrary.rs:657-665 fits_after 文档自认「计划器口径下的估计」；作品级媒体记在头一个变体名下（crates/core/src/sync/media.rs:105 lay_for）
依赖／可并：Q852（同改 Trim / plan 折建议那段）
（大小 M，来自票 `gui-looks-like-the-design/20`）

### `Q1101` — `!=` 上认不出的值等于「这一条没起作用」，屏上眼下不说

**已裁定（2026-09-23 grill）：** 要：`!=` 白写时屏上说「这一条没排除掉任何东西」。
核查（2026-09-23）：crates/core/src/sublibrary.rs:918 thin_clause 仍 `if clause.op != Op::Is` 直接不报；:872-879 ThinClause 只有 UnknownPlatform/NoCollection/NoSource 三支，没有「这一条没排除任何东西」
（大小 S，来自票 `gui-looks-like-the-design/12`）

### `Q764` — 稿上主库那一行的「N 个根」「硬盘未连接，仍可打开」「在文件系统中打开」没做

**已裁定（2026-09-23 grill）：** 只补「N 个根」（核心库多交一个数）；「硬盘未连接」与「在文件系统中打开」不做。
核查（2026-09-23）：crates/core/src/workspace.rs:321-329 CatalogFacts 只有 variants/scanned_at，没有根数；opening.rs row_ui 三样都没画；界面无「在文件管理器里打开」
依赖／可并：Q841（同一 row_ui，基线一起重批）
（大小 S，来自票 `gui-looks-like-the-design/05`）

### `Q751` — 界面上的识别只认默认那一副提问指纹：命令行拨过 `--model-id` / `--model-effort` 问出来的答案界面用不到

**已裁定（2026-09-23 grill）：** 不做——界面维持只认默认那一副旋钮。
核查（2026-09-23）：crates/gui/src/stages.rs:1661 model: DEFAULT_MODEL、:1667 Limits::default()；model_call 表（crates/core/src/catalog/identify.rs:228-238）只记 model 名，力度/候选数/上限不记
（大小 L，来自票 `gui-answers-all-six/02`）

## 第三组：读不动不是没有

### `Q613` — 一份中立库文件自己读不动（权限、被锁），落在「文件坏了」那一支

**已裁定（2026-09-23 grill）：** 分出来：每一份中立库四态（开得了／结构版本对不上／读不动／文件坏了），ADR-0021 已再修订。
核查（2026-09-23）：crates/core/src/workspace.rs:607-631 entry_of：Version→SchemaMismatch，其余 Err→CatalogState::Broken（含权限、被锁、被挪走）。ADR-0021 修订段写死三态「开得了／结构版本对不上／文件坏了」。
依赖／可并：Q614、Q616（同一条「读不动≠没有」线）
（大小 S，来自票 `no-mute-spots-opening-a-catalog/04`）

### `Q614` — `Site::open` 先问一句 `catalog.exists()`，目录读不动时说的是「还没有这份库，先跑一次 scan」

**上游已答，只差动手：** ADR-0021 修订（开场三态：读不动 ≠ 空的/没有）；workspace::DirUnreadable 那句已有。条目自带修法：try_exists。。
核查（2026-09-23）：crates/core/src/site.rs:123 Site::open 与 :142 open_file 都是 `if !catalog.exists()`→NoCatalog/not_found（:78），目录读不动时说成「还没有这份库，先跑一次 scan」。CLI 那六处预问（Q523）已裁「维持现状」。
依赖／可并：Q613、Q616
（大小 S，来自票 `no-mute-spots-opening-a-catalog/04`）

### `Q616` — 「不许写进任何一份主库」那道守卫在中立库目录读不动时照旧放行

**已裁定（2026-09-23 grill）：** 查不出主库的根时只放行新建，目标已存在就拒绝并说明（ADR-0004 已修订）。
核查（2026-09-23）：crates/cli/src/main.rs:6199-6203 refuse_writing_into_any_library：catalogs(..).entries() 为 Err 就 return Ok(())；它**只守一处**——:4250 `romcat triage export --out`（把沉淀库导成 JSON）；写法是 :6221 write_file→fs::write，会**覆盖已存在的文件**。中立库在 工作目录/catalog/（workspace.rs:243），沉淀库在 工作目录/verdict/（:748），所以 catalog 目录读不动时沉淀库照样开得了、导出照样走到这一步。
依赖／可并：Q613、Q614
（大小 S，来自票 `no-mute-spots-opening-a-catalog/04`）

### `Q976` — 开窗时优先级表读不动**静默退回内置那份**，屏上一个字都不说

**上游已答，只差动手：** ADR-0021（读不动是第三态）+ stages.rs 导出那段「优先级表读不出来就停下，不退回内置那份」。
核查（2026-09-23）：crates/gui/src/app.rs:230-232 `if let Ok(priorities) = …priorities(None,&workspace)` 读不动静默不设，浏览屏留着 Priorities::builtin()
依赖／可并：Q1031（同形：工作目录配置读不动）、Q783（同一份表）
（大小 S，来自票 `gui-looks-like-the-design/22`）

### `Q996` — 详情页「子库」「导出」两行读不动时退回「—」与「还没导出」，把不可读压成了「没有」

**上游已答，只差动手：** ADR-0021（不可读是第三态，不压成「没有」）。
核查（2026-09-23）：crates/gui/src/browse/work.rs:604 sublibrary::holding、:609 entry_exported 出错只设 self.error，行上照画「—」/「还没导出」(:3939)；同文件收藏那行同样写法
（大小 M，来自票 `gui-looks-like-the-design/34`）
