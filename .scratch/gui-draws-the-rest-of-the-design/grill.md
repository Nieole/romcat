# 界面把设计稿剩下的画完

**Status:** 待 `/to-spec`
**Opened:** 2026-09-23（`/settle` 第五轮收口 → `/grill-with-docs`）
**来源：** `.scratch/PARKING-LOT.md` 第五轮收口里无家可归的那摊活；条目正文在各原票末尾「挂单裁决（第五轮收口）」一节

## 它建什么

`gui-looks-like-the-design` 34 张票之后稿上还差的地方，外加收口时裁出来的 10 件。
**先做共用控件那一组**（小标题加粗、单选、勾选框、两格令牌）：它是后面几件的前置，而且让最大那次截图基线重批并成一次点头。

## 34 件（形状已裁定的直接照写）

## 第一组：共用控件（先做）

### `Q770` — 小标题（设计稿 `.sec`）没照稿加粗，画得与帮助字一样

**已裁定（2026-09-23 收口）：** 小标题 `.sec` 照稿加粗。
核查（2026-09-23）：look.rs:1216 section 和 1221 help 函数体一模一样（small().weak()），没加粗。section 共 34 处调用，分在 9 个文件（queue 8、merge 6、browse 5、shaping 4、sublibrary 3、browse/sublibrary 3、opening 2、work 2、keys 1）——改一行，受影响的基线远不止开场，库屏成型、合并、待确认、浏览、子库、设置快捷键都要重批
依赖／可并：Q1043 / Q946（最好一次重批）
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/05`）

### `Q946` — 待确认屏与刮削弹层里的单选框还是 egui 自带的黑点，没换成共用的 `look::radio_option`

**纯活：** queue/scrape 五处单选换 look::radio_option 重出截图；check_tokens.py 考虑核版式令牌。
核查（2026-09-23）：egui 自带单选还剩五处：queue.rs:2347、2372、2374（手工指定表单）、scrape.rs:644（补缺/重采）、browse/merge.rs:771（合并第二步「首选」RadioButton，挂单写时还没有）。共用件 look.rs:916 radio_option 是「圆点+名字+说明」整块，行内短单选需要先拆出一枚单独的圆点
依赖／可并：Q1015 与 Q1183①（同样要把 radio_option 拆成圆点+自由内容）、Q1043（共用控件一趟重批基线）
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/21`）

### `Q1043` — 勾选框是 egui 默认那一枚，不是稿上那枚圆形 `.ckb`

**已裁定（2026-09-23 grill）：** **自画稿上的 `.ckb`**（圆角方框、强调色填满），19 处都换，立几格令牌。
核查（2026-09-23）：全仓 19 处 ui.checkbox（browse 3、merge 3、scrape 4、queue 2、sublibrary 2、table 2、roots/shaping/stages 各 1），没有一处自己画的。⚠️挂单的前提写错了：稿 .ckb 不是圆形，是 15px、圆角 4 的方框，选中时强调色填满+一圈内白，不画勾（prototype.html:350–351）；稿的 .opt 里用的又是原生 checkbox 加 accent-color（:373）。稿自己就有两种画法
依赖／可并：Q770、Q946（共用控件一次重批）
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/29`）

### `Q1015` — 合并向导第一步那枚「保留」标签在 help 之后，稿上在作品名之后

**纯活：** 拆 look::radio_option，把「保留」标签摆到作品名后。
核查（2026-09-23）：browse/merge.rs:548–553 先画 look::radio_option（圆点+名字+说明一整块），「保留」tag 接在整块后面；稿 .mwit 的 <b>${w.t}</b> 后面紧跟 keepb。「保留」这个词定死了（CONTEXT.md:61）
依赖／可并：Q946 / Q1183①（同样拆 radio_option：圆点单独一枚，内容由调用方摆）
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/16`）

### `Q813` — 容量条的高度、「未知」那一段多长、卡片两列的门槛，令牌里没有对应的格

**纯活：** tokens 添 card-min-width，子库卡片两列门槛改取它（不再借 dialog_width[0]）。
核查（2026-09-23）：其余几格已进令牌；只剩两列门槛借 dialog_width[0]（sublibrary.rs:2058）与空态卡宽借 dialog_width[1]（sublibrary.rs:4790）。稿 .devs（prototype.html:381）固定两列、没有断点，所以门槛值本身是我们定的，加一格令牌照旧 520 不改观感
依赖／可并：Q824（同改 tokens.toml [layout]，一趟做）
（大小 S，来自票 `gui-looks-like-the-design/20`）

### `Q824` — 库屏两栏的比例写成界面里的一个常量，没进令牌

**纯活：** tokens.toml [layout] 加 library-left-share，roots.rs 从令牌取，check_tokens.py 核稿。
核查（2026-09-23）：roots.rs:144 const LEFT_SHARE: f32 = 1.25 / 2.25；tokens.toml [layout] 没有这一格；稿 .libgrid minmax(0,1.25fr) minmax(0,1fr)（prototype.html:248）
依赖／可并：Q813
（大小 S，来自票 `gui-looks-like-the-design/06`）

## 第二组：作品详情页与浏览屏

### `Q928` — 作品详情页头上那一句（`.hsub`）写作品名，稿上写副标题与别名

**已裁定（2026-09-23 grill）：** 照稿：副标题 = 标题集合里「官方名称」那一条，别名 = 其余几条；核心库一问，详情页与合并向导共用。
核查（2026-09-23）：browse/work.rs 的 hero：显示标题≠作品名时写作品名，没认出的写「名称取自文件名」（work.rs:1347 附近）。稿 w.sub 多为罗马字/英文官方名（prototype.html:1638–1646），w.aka 是其余叫法；核心库没有「官方名称一条+其余叫法」这一问。词表「标题集合」条（CONTEXT.md:320）已有类型「官方名称/译名/别名/汉化组译名」
依赖／可并：Q1013②
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/15`）

### `Q929` — 侧边详情：文件那一块拿掉了；头上「N 个变体 · 汉化」没画

**纯活：** 侧边详情头上接「· 汉化／官中」（Catalog::work_chinese_mark）。
核查（2026-09-23）：browse.rs:5012 只写「N 个变体」；稿 prototype.html:1834 是「${w.v} 个变体 · ${w.zh}」，平台那行还接「· 开发商」。核心库那一问现成：Catalog::work_chinese_mark（core/src/catalog/browse.rs:2382，详情页 work.rs:588 已在用）
依赖／可并：Q810（同一块侧边详情）
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/15`）

### `Q961` — 作品详情页上收藏的按钮与标签、合集那一行没画，留给票 13

**上游已答，只差动手：** 第五轮收口把 Q1108 并进 Q961 归「活」，意思是补，不是认下批量那条路。
核查（2026-09-23）：一半已被票 13 做掉：状态块「合集」那一行和「×」移出已经有了（work.rs:3882）。剩下：头上没有「☆ 收藏」按钮和「★ 已收藏」标签（work.rs 里找不到 ☆/★）；动作行与合集那一行都没有「加入合集…」（work.rs:3815 注释明写没摆；稿 prototype.html:2103/2109/2133）。只对一部作品排活现成：browse.rs 的 queue_collection_keys（右键菜单在用）
依赖／可并：Q1108（已并入）、Q1110（同一个加入合集弹层）、Q1182（unfavorite 要删，收藏按钮得走新路）
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/15`）

### `Q925` — 元数据那一面帮助里「数据源优先级」没做成按钮

**纯活：** 元数据帮助里「数据源优先级」换成行内按钮，打开优先级弹层并停在对应字段。
核查（2026-09-23）：browse/work.rs:1093/1096 「按数据源优先级选用」是普通字；优先级弹层已经有了（priority.rs Editor::open，priority.rs:155；设置屏 settings.rs:209、刮削面板各开一份），但没有「开到某个字段」的入口
依赖／可并：Q785（同一个 Editor）
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/15`）

### `Q926` — 六个面那一排标签不吸顶

**纯活：** 作品详情页六面标签吸顶（头与标签分段摆）。
核查（2026-09-23）：browse/work.rs:300 一个 ScrollArea 里依次画 hero、tabs_ui、正文，标签跟着滚走；稿 .tabs{position:sticky;top:0}
（大小 M，来自票 `gui-looks-like-the-design/15`）

### `Q922` — 识别依据那一面的帮助只照稿写前半句：稿上后半句与 ADR-0002 对不上

**已裁定（2026-09-23 收口）：** 识别依据帮助字的后半句照 ADR-0002 改写。⚠️ 核过：`browse/work.rs:1018` 实际把稿上那句对不上的后半句也印到了屏上，比挂单记的更糟。
核查（2026-09-23）：⚠️比挂单记的更糟：browse/work.rs:1015 注释说只印前半句，可 1018–1019 的字符串把稿上那后半句「高置信自动通过；中、低置信进入待确认队列。」也印出来了——和 ADR-0002:3「中置信通过但标记」对不上的那句现在就在屏上
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/15`）

### `Q962` — 平台表里只有设计稿给过全名的十个平台写了全名，其余二十四个屏上写代号

**已裁定（2026-09-23 收口）：** `platforms.toml` 补其余 24 个平台的官方英文全名。
核查（2026-09-23）：core/src/platform/platforms.toml 只有 10 个平台写了「全名」（:122–266）；其余 24 个（FDS、3DS、N64、NGC、WII、WIIU、Mega-CD、SMS、GG、32X、SS、DC、PCE、PS2、PS3、VB、WS、NGPC、Lynx、MSX、3DO、SWITCH、XBOX360、街机）没写。另外 :108–109 那段注释「眼下只写了设计稿给过全名的那几个」要跟着改
（大小 S，来自票 `gui-looks-like-the-design/15`）

### `Q810` — 侧边详情里那颗「在待确认中处理」没有票接

**已裁定（2026-09-23 grill）：** 只在变体在待确认队列里时摆「在待确认中处理」。
核查（2026-09-23）：browse.rs:4866–4874 认不出作品那一行只印 note_box，没有「在待确认中处理」按钮；稿 prototype.html:1837 与 :2165（标题面空态）各有一颗（data-act=goobo）。queue.rs:1075 pick_row(key) 能定位；但待确认队列是「候选的集合」（CONTEXT.md:199），一条候选都没有的变体不在里面
依赖／可并：Q929（同一块侧边详情）
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/09`）

### `Q1097` — 一个拉丁标题都没有时，排序标题装的就是那串中文

**已裁定（2026-09-23 grill）：** 库屏「整理标题」那一行接一句「其中 N 个没有拉丁标题」，数由核心库给。
核查（2026-09-23）：一半已经不在：详情页标题那一面「排序标题」那一行在 SortFrom::None 时已经说「没有拉丁标题：只能照显示标题排…得人工补一个」（browse/work.rs:3083 sort_note，票 15 做的）。剩下「库屏要不要给个数」：TitleReport 在任务台 Titled 那一格（task.rs:90），库屏没有
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/11`）

### `Q1104` — 默认窗口下左栏那句「有 N 条筛不出东西」在折线以下，眼下余量是零

**已裁定（2026-09-23 grill）：** 提示贴着条件组框里出问题的那一条子句。
核查（2026-09-23）：tests/browse.rs:4130 够高的一帧()（1280×1200）仍在兜底；browse.rs:1622/3675 那几句仍摆在条件组框底下，默认 1280×800 首屏看不到
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/13`）

### `Q1142` — 上下键只在表格那一路挪得动高亮，卡片墙挪不动

**上游已答，只差动手：** 稿已经答了：高亮存作品身份，表格和卡片墙共用（prototype.html:479/1934/3368），只差动手（focused 换成 WorkAnchor）。
核查（2026-09-23）：browse.rs:460 focused: Option<u64> 还是行序号；app.rs:1026 browse_keys 靠 showing_cards（browse.rs:752）把卡片墙那六下整个关掉。稿 :479 .gcard[aria-selected] 与 :1934 都按作品号 S.sel
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/14`）

### `Q1143` — 键盘入口眼下有三处，这一票只并了自己那一份

**已裁定（2026-09-23 grill）：** 只把「有弹层／有浮层／光标在框里」那三道判断抽成共用函数，焦点那条无障碍路留着。
核查（2026-09-23）：三处都在：app.rs:956 App::shortcuts（加上 1026 browse_keys）、queue.rs:2400 Screen::keyboard、browse.rs 卡片 has_focus 那一处 Enter/空格；「算不算快捷键」那三道门在①②各抄一份
依赖／可并：Q1142（A 需要先做）
（大小 M，来自票 `gui-looks-like-the-design/14`）

### `Q798` — 浏览屏「加入合集」「移出合集」靠禁按钮拦空名，排活那个入口不拦

**上游已答，只差动手：** ADR-0005 修订段「拒绝写在排活那一个入口里」加「再修订」（弹层里已经常驻理由，可以同时画灰）：只差在 join_collection 起手调 check_name、带理由拒绝。
核查（2026-09-23）：入口换了：浏览屏「加入合集…」现在先过 collections::Join 弹层（browse/collections.rs:410–417 走 collection::check_name，按不动时 hover/行内说原因），但排活那个入口 browse.rs:2866 join_collection / 2872 leave_collection → 2914 queue_collection 仍然不查名字，空名要排上去才在核心 CollectionError::Nameless 记失败
依赖／可并：Q1182（leave_collection 要删，只剩 join 这一支）
（大小 S，来自票 `gui-looks-like-the-design/07`）

### `Q1110` — 「加入合集」那块「只钉得住本机路径」的警告无条件常驻

**已裁定（2026-09-23 收口）：** 「加入合集」那块警告换中性色，认下常驻。
核查（2026-09-23）：browse/collections.rs:492 用的是 look::warn_box（黄色），无条件常驻；换成中性色的 look::note_box 就行
依赖／可并：Q961（同一个加入合集弹层）
（大小 S，来自票 `gui-looks-like-the-design/13`）

### `Q1182` — `Screen::save_as_sublibrary` 与 `SaveDraft` 从此只有测试在用

**已裁定（2026-09-23 收口）：** 删掉只剩测试在用的 `save_as_sublibrary` / `SaveDraft`（连票 13 的 `unfavorite` / `leave_collection`），测试搬到新路上——先核新路的重名口径。
核查（2026-09-23）：browse.rs:374 SaveDraft、:2003 save_draft_mut、:4713 save_as_sublibrary 只剩 tests/browse.rs:1439–1704 那五处在用；票 13 的 unfavorite（browse.rs:2861）和 leave_collection（:2872）只剩 tests/browse.rs:2112/2156 在用。删之前得先核：新路（子库屏目标设置）上「重名当场拦下」归谁管、口径是不是一个，target::vet_name 看着就是
依赖／可并：Q798（leave_collection 一起删）、Q961（收藏按钮走的新路）
（大小 M，来自票 `gui-looks-like-the-design/23`）

### `Q1183` — 「加入子库」那一层有三处没照稿画

**已裁定（2026-09-23 grill）：** 照稿：目标设置弹层做成两屏共用，在浏览屏原地叠上，建完回到「加入到」并选中新那台。
核查（2026-09-23）：browse/sublibrary.rs:332「加入到」每一台用 radio_option，没有 120×8 的容量条（look::gauge_bar 现成）；:508 三格() 是一行三列裸字（font::strong+help），没有 .estbox 的格子/边框/等宽；:342「新建子库…」把人送去子库屏。①②稿上画了，照稿做就行；③要选
依赖／可并：Q946 / Q1015（①要拆 radio_option）、Q950（同一层目标设置弹层）
（大小 L，来自票 `gui-looks-like-the-design/23`）

### `Q1185` — 「只加入勾选的」在「全选筛选结果」那一档按得动，稿上是禁的

**上游已答，只差动手：** 稿（pk=S.pickAll?0）+ ADR-0016「例外处理规则表达不了的个人口味、永久记住」+ ADR-0005「再修订」（守卫在入口、要带理由，屏上常驻理由，可以同时画灰）——都指向①照稿禁掉，并写明「全选请用『作为规则加入』」。
核查（2026-09-23）：browse/sublibrary.rs:385–393「只加入勾选的 N 个作品」的 N 取 facts.picked，全选那一档走 Picked::count(total)，按得动；稿 pk=S.pickAll?0:S.picked.size，那一档 aria-disabled（prototype.html:2379）
依赖／可并：Q1183（同一层）
（大小 S，来自票 `gui-looks-like-the-design/23`）

## 第三组：合并向导

### `Q1013` — 合并向导与「移出此作品」还差设计稿上的三样：封面缩略图、副标题、「由别处移入的那一档」

**已裁定（2026-09-23 grill）：** 「由别处移入」那一档不做；封面缩略图与副标题照稿做。
核查（2026-09-23）：browse/merge.rs:520 step_pick 每一行只有 radio_option 加 help，没有 44 点封面（merge.rs 里没有 Shelf/cover）；没有副标题；「由别处移入」这一档不区分（核心库答不出「这个变体是哪一批裁决搬来的」，只能顺着 verdict_batch_row 反查）
依赖／可并：Q928（副标题口径）、Q1015（同一行）
（大小 L，要重批截图基线，来自票 `gui-looks-like-the-design/16`）

## 第四组：子库屏

### `Q950` — 目标设置弹层照稿还差两处：容量填错时按钮照样按得动、选择集空时「设备上的位置」整格不画

**已裁定（2026-09-23 grill）：** 空选择集时照新建那一档拿示例名画。
核查（2026-09-23）：sublibrary.rs:2021 form_ready 只判名字和目标路径两道，不判容量；sublibrary.rs:3798 `if let Some(landing)`：改一台而选择集是空的时候，Footprint::landing 交 None，第六格整格不画（新建那一档 3553 行拿示例名画）。并件 Q1025：按钮该不该变灰
依赖／可并：Q1025（已并入）
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/21`）

### `Q814` — 看过目标之后，卡上「合计」与容量条「选中」是两个数

**已裁定（2026-09-23 收口）：** 子库卡合计那一行旁补「不含元数据与转换」。
核查（2026-09-23）：sublibrary.rs:2603「合计 {} 个变体 · {}」，旁边没有注
（大小 S，要重批截图基线，来自票 `gui-looks-like-the-design/20`）

### `Q974` — 手动例外弹层里搜作品搜不到「还没识别」的那几份，也没有翻页

**已裁定（2026-09-23 收口）：** 手动例外弹层摆满 6 行时补「还有 N 个，去浏览屏筛」并给跳转。⚠️ 那条跳转走的是「改选择」回程，要等 `Q1184` 修好。
核查（2026-09-23）：sublibrary.rs:131 SEARCH_HITS=6、:4558 work_page_with_titles(.., 0, 6, ..)，不说还有多少。数的那一问现成：Catalog::work_total(&WorkQuery)（core/src/catalog/browse.rs:2166）。跳转挂单指的是 edit_selection（sublibrary.rs:1532），它走的是「改选择」那条回程，会碰到 Q1184（replace_rules 清掉别的规则）；最好改成带着搜索字去浏览屏，不进改选择
依赖／可并：Q1184（不归这一束：别走改选择那条回程）
（大小 S，来自票 `gui-looks-like-the-design/22`）

### `Q1028` — 排差量那趟活没有稿上那句副标题「只读取库和设备上的目录，不写入文件」

**已裁定（2026-09-23 grill）：** 核心库给任务加一个可选副标题槽，八种长活都照稿填——**这一条翻掉了收口时 `Q835` 的「先不画」**。
核查（2026-09-23）：sublibrary.rs:1264 只给任务台一个标题「排差量预览 · 名字」；core/src/task.rs:689 Tasks::queue 只收标题，没有副标题槽。稿的 TASKS 每一种长活都有 sub（prototype.html:1473–1497：扫描、下载、识别、刮削、整理标题、导出、差量、同步），差量那一条是「只读取库和设备上的目录，不写入文件」
依赖／可并：Q883（库屏那一行也要从任务台取进度）
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/24`）

## 第五组：库屏

### `Q881` — 工序那几行按钮上的字：设计稿「开始扫描 / 运行 / 去处理」，这一版沿用「开跑 / 去待确认队列」

**上游已答，只差动手：** 挂单 Q881 状态栏：拿主意的人已裁 B 照稿（「下载/运行/重新运行/开始扫描/去处理」，不再用「取回」）。
核查（2026-09-23）：大半已做：stages.rs:1472 go_label 已是「开始扫描/运行」、stages.rs:79 TO_QUEUE=「去处理」。剩两处：①扫到一半的根没分出「继续扫描」（稿 prototype.html:1688 S.half.scan?'继续扫描'）；断点文件已有 workspace::checkpoint_path，界面查一眼有没有即可（ADR-0005「盘上缺一样东西」留界面层）；②任务台那一趟仍叫「取回 · 某某」（roots.rs:683），拿主意的人裁过不再用「取回」
依赖／可并：Q883（同一个 row_button）
（大小 S，来自票 `gui-looks-like-the-design/06`）

### `Q883` — 正在台上跑的那一行：设计稿是空圆点、实时进度、弱化的「查看任务」，这一版是禁着的「跑着呢」

**上游已答，只差动手：** 挂单 Q883 状态栏：拿主意的人已裁照稿（转圈空心圆点、实时进度、「查看任务」跳任务屏），只差动手。
核查（2026-09-23）：stages.rs:1145 与 1210 仍是禁着的「跑着呢」；paint_stage_dot（stages.rs:1422）没有「在跑」这一档；票 32 已合（9163cbb），任务屏路由在 app.rs（View::Tasks）现成
依赖／可并：Q881（同一函数）、Q1028（都要从任务台那一趟取进度/副标题）
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/06`）

### `Q1041` — 调整成型那一层里变体那几行没写**变体简称**

**已裁定（2026-09-23 grill）：** 简称拼得出就照稿写两行，拼不出只画路径一行。
核查（2026-09-23）：shaping.rs:414–430 一行只有勾选框、root_and_path（等宽）、「N 个文件 · 大小」。variant_short_names（core catalog/browse.rs:2340）只收 WorkDetail，成型存疑那几行按变体键来、不一定挂着作品，需要一支按变体键问的。拼不出简称时会退回文件名，和底下路径那一行说的是同一件事——稿只画了识别过的那种
（大小 M，要重批截图基线，来自票 `gui-looks-like-the-design/29`）

## 第六组：待确认、刮削与设置

### `Q966` — 就地落下那一批的作用范围记下来了，可裁决记录那一行**屏上不画**，只在悬停里

**已裁定（2026-09-23 收口）：** 裁决记录副行照稿接「· 备注」，有备注才接。
核查（2026-09-23）：queue.rs:997–1021 的副行还是「时刻 · summary」，record.note 只拼进悬停。注意：note 里除了 scope_note 写的作用范围，还有人在「手工指定」表单里自己写的备注（queue.rs:2381），这两种都会接到副行上
（大小 S，来自票 `gui-looks-like-the-design/19`）

### `Q664` — 刮削弹层按下「开始刮削」之后不自己关上

**已裁定（2026-09-23 收口）：** 刮削弹层按下「开始刮削」排上就关，回执交给任务台。
核查（2026-09-23）：scrape.rs:577–578 排上之后弹层还开着，底下写「这一趟在任务屏里跑着。」；跑完的回执也画在弹层里（scrape.rs:452–467）。关上之后回执去哪还没定，挂单建议交给任务台历史（任务屏现成）
（大小 S，来自票 `gui-looks-like-the-design/04`）

### `Q785` — 「恢复默认」之后保存，写一份与内置相同的文件，而不是删掉工作目录那份

**已裁定（2026-09-23 收口）：** 优先级弹层「恢复默认」后保存改成删掉工作目录那份。⚠️ 核过：`priority.rs:266` 的 `can_save` 会让这一下按不动，要一起改。
核查（2026-09-23）：priority.rs:260 reset 只换草稿，273 save 照常写 workspace::priorities_path。注意：can_save（priority.rs:266）是 draft!=current||unreadable——工作目录那份内容和内置一样时，按「恢复默认」之后保存按不动，也就删不掉。改的时候要把「文件在、草稿等于内置」也算成可保存
依赖／可并：Q925（同一个 Editor）
（大小 S，来自票 `gui-looks-like-the-design/30`）
