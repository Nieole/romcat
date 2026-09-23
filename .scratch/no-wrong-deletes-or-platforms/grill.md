# 移除根、改选择与裁决过的平台不再出错

**Status:** 待 `/to-spec`
**Opened:** 2026-09-23（`/settle` 第五轮收口 → `/grill-with-docs`）
**来源：** `.scratch/PARKING-LOT.md` 第五轮收口里无家可归的那摊活；条目正文在各原票末尾「挂单裁决（第五轮收口）」一节

## 它建什么

三处**错**，不是取舍：移除一个根时删刮削三表的 SQL 两列写反，一行都没删掉（`Q555`）；子库「改选择」回程把同一台别的规则连名字一起清掉（`Q1184`）；人裁决过的变体在读卡带头之前就短路，平台按目录判错（`Q602`）。
拿主意的人定：**不跟整摊排队，单独一个小 effort 立刻修**，每条带一条先红后绿的测试。

## 3 件（形状已裁定的直接照写）

### `Q555` — 移除一个根时删刮削三张表的那三句把两列写反了，一行都没删

**纯活：** roots.rs 三句 DELETE 的 anchor/subject 对调，加测试钉住。
核查（2026-09-23）：crates/core/src/catalog/roots.rs:538-543 三句仍是 `WHERE subject = '变体' AND substr(anchor,1,length(?1))=?1`；而 crates/core/src/catalog/scrape.rs:1147-1151 写入时 anchor=anchor.label()（「变体」/「作品」）、subject=键——两列写反，一行都删不到
依赖／可并：Q852、Q819（同在核心库 catalog/sync 一带，可同批，不必同票）
（大小 S，来自票 `gui-answers-all-six/04`）

### `Q1184` — `replace_rules` 把名字抹成 `NULL`，「改选择」那条回程会连别的规则一起清掉

**已裁定（2026-09-23 grill）：** 「改选择」回程**只换载进来的那一条**，名字与同一台别的规则一概保留（走现成的逐条替换）。
核查（2026-09-23）：crates/core/src/catalog/sublibrary.rs:858-888 replace_rules 删掉全部读得懂的规则、INSERT 一条 name=NULL；crates/gui/src/browse.rs:2147 「改选择」(Editing.ordinal=None，sublibrary.rs:1544) 走它；逐条那支 replace_rule（:1000-1013）只 UPDATE text，名字留着
依赖／可并：票 gui-looks-like-the-design/12 的「改选择」闭环
（大小 M，来自票 `gui-looks-like-the-design/23`）

### `Q602` — 裁决短路那一步没探卡带头，落下的是目录声明的平台：被裁决过的放错目录的变体，刮削照旧撞不上

**已裁定（2026-09-23 grill）：** 照 Switch 的先例：裁决短路之后先取卡带头缓存，取不到再读头并落库，最后重算平台。「裁决过的一个字节都不读」那句改成「只读判平台所需的头」。
核查（2026-09-23）：crates/core/src/identify.rs:1056 与 :1138 两处 ask_verdicts 都排在 :1239 probe_carts 之前；:2287-2289 注释自认「units 上还没有卡带头，判出来的是目录声明的那个（挂单 Q602）」；platform_of(:3425) 没卡带头就退回 variant.platform。先例：settle_after_verdict(:1343-1370) 已为 Switch 容器头开过「一个字节都不必再读」的唯一例外（Q704）
依赖／可并：Q605（同为『平台按内容判』的收尾）、Q1030
（大小 M，来自票 `one-criterion-per-thing/03`）
