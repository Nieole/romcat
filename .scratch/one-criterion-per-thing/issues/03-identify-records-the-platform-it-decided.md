# 03 — 识别把「内容说的」那个平台落下来，刮削读它

**What to build:** 维护者把一个 `.gba` 放在写着别的平台的目录里，**刮削**照旧撞得上——因为它用的是
识别按**内容**认出来的那个平台，不是目录声明的那个。此前识别认出来了却从不记下来，刮削只好听目录的。

收挂账 `D123`：平台交叉校验用的是目录声明的平台（识别侧早改成看内容了，刮削侧没跟上）。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **这是 ADR-0024 推论 3 的头一个用例：物化结论允许，第二次判断不允许。**
刮削**不重新判一次平台**，它读识别判过的那个。

⚠️ **不回写「目录声明的平台」那一列。** 那一列的语义就是「目录说的」，改它会让**平台冲突**
那张报表失去对照物。新的是**另一列**，纯加，**不升结构版本**（判据：改了已有表的列或含义才加一）。

- [x] 目录写着一个平台、内容是另一个时，刮削撞的是**内容**那个
  - 证据：`crates/core/tests/scrape.rs` 的 `目录写着一个平台而内容是另一个时刮削撞的是内容那个`（`nds/` 下一张真头部字节的 GBA 卡、只收 GBA 的中文条目）。改前红（`left: None`），改后绿。
  - **保留**：被裁决过的变体走裁决短路（`ask_verdicts`），那一步没探卡带头，落下的仍是目录声明的平台。在这些变体上这一条不成立，也没有测试钉，见挂单 `Q602`（已转拿主意的人）。
- [x] 「目录声明的平台」那一列一个字没动，平台冲突那张报表照旧报得出来
  - 证据：`crates/core/tests/cart_header.rs` 的 `识别把内容说的平台落下来而目录声明的那一列一个字不动`。第一趟与第二趟（第二趟卡带头从中立库取回）都查了三件事：`variant.platform` 仍是 GBA，`platform_conflicts` 仍报 GBA→NDS，判定的平台是 NDS。原有的 `内部头与目录声明的平台冲突记下来` 照旧绿。
- [x] 一个都读不出内部头时，照旧退回目录声明的那个（行为不变）
  - 证据：`crates/core/tests/scrape.rs` 的 `一个都读不出内部头时照旧退回目录声明的那个`。改前改后都绿，行为不变。
- [x] 结构版本没升；旧库打开照旧能用
  - 「没升」的证据是 diff：`git diff b69b5c6 -- crates/core/src/catalog.rs` 为空，`SCHEMA_VERSION` 仍是 7。**保留**：这一点没有测试钉。
  - 「旧库照旧能用」的证据：`crates/core/tests/scrape.rs` 的 `没有判定平台那一列的旧库照样打得开而且重跑识别之后刮削就用上了`。删掉那一列后 `Catalog::open` 照样打得开，刮削退回目录声明的平台；重跑识别之后，刮削撞的是内容那个。

门禁（2026-09-14，提交前在分支上跑，带 `TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp`）：`cargo xtask gate -j 3 --test-threads 3 --keep-going`，6 条全绿，`EXIT=0`，71 个测试目标、1889 条通过、0 失败（日志 `/Users/nicoer/dev/game-wt/logs/slot-4-oc03-gate.log`）——fmt 1s、glossary 1s（扫了 11 份 `.rs` 里新写的 336 行，没撞上）、check 20s、clippy 19s、test 645s、doc 12s。

## Comments

收尾（2026-09-14）：
- **`D123`**：没识别过的与没被裁决过的变体上收干净了。**被裁决过的变体上没收干净**（挂单 `Q602`）。
- 本票走过的岔路口记在挂单 `Q601`–`Q606`。
- 物化的列叫 `identification.platform`，读法是 `Catalog::identified_platforms`，留给票 `gui-looks-like-the-design/28` 用。

## 挂单裁决（第五轮收口，2026-09-23）

从 `.scratch/PARKING-LOT.md` 迁来（`/settle`，范围 `Q449`–`Q1185`）。每条顶上多一行**裁决**：落哪一档、去了哪儿。
编号 `Qn` 留着占号、不复用。

### Q601 — 旧库补识别判定的平台那一列走 `ALTER TABLE … ADD COLUMN`，没另开一张表

- **裁决（第五轮收口，2026-09-23）：** **记** —— 另开表要各处补 DELETE，翻案是改结构不是编辑。记下，无事可做

- **来自：** 票 `one-criterion-per-thing/03`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/identify.rs` 的 `IDENTIFY_SCHEMA`（`identification.platform`）与 `add_columns`
- **为什么没停线：** 「纯加一列、不升结构版本、旧库打开照旧能用」三条同时做得到：同一个文件的 `add_columns` 早就这么给 `content_hash` 补过两列 SHA-1，判据是「旧数据会不会被读错」。
- **这张票实际做了什么：** 新库建表时带上 `platform TEXT`；旧库在 `Catalog::open` 里由 `add_columns` 补一句 `ALTER TABLE identification ADD COLUMN platform TEXT`，老行上是 NULL，读的那一侧（`Catalog::identified_platforms`）当「识别没判过」退回目录声明。`SCHEMA_VERSION` 一个字没动。测试 `scrape.rs` 的 `没有判定平台那一列的旧库照样打得开而且重跑识别之后刮削就用上了` 删掉那一列再开。
- **没走的那条：** 另开一张 `identification_platform(variant_key, platform)` 表，`CREATE TABLE IF NOT EXISTS` 白拿——`content_disc` / `content_cart` 那几张表的注释写的就是「加表不加列」。
- **建议留：** 这条。规格原话是「落在识别结论那张表上」；另开一张表要跟着 `clear_identifications`、`drop_variant_orphans`、`drop_stale_conclusions`、按根删那几处各补一句 `DELETE`，漏一处就留下一行过期的平台。`content_disc` 那句「加列要删库重扫」早被票 10 的 `add_columns` 推翻了。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q602 — 裁决短路那一步没探卡带头，落下的是目录声明的平台：被裁决过的放错目录的变体，刮削照旧撞不上

- **裁决（第五轮收口，2026-09-23）：** **活** —— 无家可归（全仓没有一张开着的票占这片地），进 `/grill-with-docs`，束「缺陷：先修，别等整摊」。

- **来自：** 票 `one-criterion-per-thing/03`
- **类别：** 规格没说
- **在哪：** `crates/core/src/identify.rs` 的 `ask_verdicts`——`identify_variant` 里两处调它，答出结论就返回，都排在 `probe_carts` 之前
- **为什么没停线：** 四条验收的测试都绿，但**验收 1（「刮削撞的是内容那个」）在被裁决过的变体上不成立**，也没有测试钉这条路。改它要在下面两条都站得住的路之间挑：短路那一步不读盘是 `identify_variant` 那段注释许的（「裁决过的东西一个字节都不必再读」），不是 ADR 原文，要不要为卡带头破例是拿主意的人的事。
- **这张票实际做了什么：** `ask_verdicts` 答出结论时照样调 `platform_of`，但那时 units 上还没有卡带头，判出来的是目录声明的那个。**`D123` 在被裁决过的变体上没收干净**：`psp/` 目录下那批 FC / GB / SFC 整理包（`D123` 与 `platform_of` 文档里举的正是它）一旦在待确认队列里被裁过，从下一趟识别起判定的平台又回到 PSP，刮削的中文名照旧被平台交叉校验挡掉。
- **没走的那条：** (a) 短路之前从 `content_cart` 取回算过的卡带头（不读盘）再判——删库重扫之后那张表是空的，而短路让它永远填不上，只治一半；(b) 短路那一步也探卡带头（每份 0x400 字节到 64 KiB，ExHiROM 4 MiB），与「一个字节都不必再读」那句相悖。
- **建议留：** 不留现状，开一张票走 (a)+(b)：先取缓存，取不到才读卡带头。量级与光盘那一层「只读几百字节」同档，但要不要为它松那句话，是拿主意的人的事。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q603 — 不说平台的结论写回去时用 `COALESCE` 留住判过的那个，没让快照表带上这一列

- **裁决（第五轮收口，2026-09-23）：** **岔** —— 实现走的就是条目建议的那条；追认：维持现状（2026-09-23 拿主意的人整批追认）。

- **来自：** 票 `one-criterion-per-thing/03`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/identify.rs` `write_identifications` 的 upsert（`platform = COALESCE(excluded.platform, platform)`）；`crates/core/src/identify.rs` `Projector::project` 里的 `platform: None`
- **为什么没停线：** 裁决落成的结论（`triage.rs` 两处 `write_identifications`）没看过内容，手上没有平台可写；只是「写 NULL 还是留旧值」之间挑一个。
- **这张票实际做了什么：** 结论不说平台时不盖掉库里那一行已有的；撤销走 `restore_conclusions`，那条 upsert 本来就不碰这一列，于是裁决与撤销前后判定的平台都不变。测试 `cart_header.rs` 的 `不说平台的结论写回去不盖掉识别判定的那个`。
- **没走的那条：** 照写（写成 NULL，读的那一侧退回目录声明），`verdict_batch_shadow` 补一列 `platform`，快照与放回两处跟着带上——那样与 `Q602` 眼下的短路行为（重跑识别之后是目录声明的）处处一致。
- **建议留：** 这条。`Q602` 修好之后重跑识别判出的也是内容那个，`COALESCE` 与它自然一致，不必回头改；照写那条要多补一张快照表的列，而且在 `Q602` 修好之前人一裁完就撞不上。代价如实记下：`Q602` 修好之前，裁完到下一趟识别之间用的是内容那个，重跑之后回到目录声明的——「眼下」与「重算一遍」差这一处。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q604 — 「识别没判过就退回目录声明」放在中立库的读法里，没让刮削自己退

- **裁决（第五轮收口，2026-09-23）：** **记** —— 另一条正是 ADR-0024 要挡的形状。记下，无事可做

- **来自：** 票 `one-criterion-per-thing/03`
- **类别：** 规格没说
- **在哪：** `crates/core/src/catalog/identify.rs` `Catalog::identified_platforms`；`crates/core/src/scrape.rs` `Plan::build`
- **为什么没停线：** 几条路交出来的平台一模一样，差在「识别没判过」那一步写在哪。
- **这张票实际做了什么：** 存进 `identification.platform` 的是 `platform_of` 的完整结论（连它自己退回目录的那一步）；读法是 `COALESCE(i.platform, v.platform)`（`variant LEFT JOIN identification`），只管还没识别、或者加这一列之前识别的那些。刮削只 `platforms.get(&variant.key)`，自己一句平台逻辑都不写。
- **没走的那条：** 读法只交判过的那些，刮削自己 `.or(variant.platform)`；或者列里只存内容读出来的那个，读的时候再退。
- **建议留：** 这条。刮削那一侧多一句 `.or(variant.platform)` 正是 ADR-0024 要挡的形状，下一个调用方（票 `gui-looks-like-the-design/28`）会照抄一句；只存内容那条让「退回目录」写在两处（`platform_of` 与读法）。代价：这个读法分不出「判过、判的就是目录那个」与「没判过」，要分就看 `identification_of` 有没有结论。
- **谁来裁：** 收尾
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q605 — 识别与两份报告里还有四处拿目录声明的平台，没跟着改

- **裁决（第五轮收口，2026-09-23）：** **活** —— 无家可归（全仓没有一张开着的票占这片地），进 `/grill-with-docs`，束「核心库该多交一样东西、或把一个判据收到一处」。

- **来自：** 票 `one-criterion-per-thing/03`（`for_each_identification` 那一处是 `/code-review` 的 Spec 轴补的）
- **类别：** 路过发现，不在范围内
- **在哪：** `crates/core/src/identify.rs` 的 `rank`（`platform_matches` 比的是 `variant.platform`）；`has_ammo` / `has_sha1_ammo`（按目录声明决定读不读盘）；`crates/core/src/catalog/scrape.rs` 的 `source_by_platform`（刮削报告按 `v.platform` 分平台）；`crates/core/src/catalog/identify.rs` 的 `for_each_identification`（识别报告 `identify/report.rs` 按 `v.platform` 分平台）
- **为什么没停线：** 票与 `D123` 只点了刮削取平台那一处（`scrape.rs:1335`）。
- **这张票实际做了什么：** 没动。
- **没走的那条：** `rank` 改用 `platform_of`（排序那一步 units 已经探完），刮削报告按 `Catalog::identified_platforms` 分组。
- **建议留：** `rank` 跟着改：与 `D123` 同一个形状——放错目录的变体上「平台对得上」那一档仍然听目录的，DAT 候选的次序会偏。两份报告按哪个平台分组是**口径**：按目录声明分，读者对得上盘上的目录；按判定的分，数得出「这个平台真有几个」——至少要在报告里说清是哪一个。`has_ammo` 两处留着：它们在读盘之前判，那时内容还没读，目录声明是判断的前提而不是第二个判据。开一张票。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）
### Q606 — 「识别判定的平台」词表里没有，没往 `CONTEXT.md` 加

- **裁决（第五轮收口，2026-09-23）：** **活** —— 无家可归（全仓没有一张开着的票占这片地），进 `/grill-with-docs`，束「核心库该多交一样东西、或把一个判据收到一处」。

- **来自：** 票 `one-criterion-per-thing/03`（`/code-review` 的 Standards 轴指出）
- **类别：** 规格没说
- **在哪：** `CONTEXT.md` 的 **平台** 条只说「平台由目录给出……文件内容可以推翻它」；代码里如今有两个说法并存：`variant.platform`（目录声明的）与 `identification.platform` / `Catalog::identified_platforms`（识别判定的），文档与注释里用的是「目录声明的平台」「识别判定的平台」
- **为什么没停线：** 两个说法都是 **平台** 条那句话的直译，没发明新概念；`docs/agents/domain.md` 说真缺口「记下来交给 `/domain-modeling`」。
- **这张票实际做了什么：** 没动 `CONTEXT.md`，只在代码文档里把两个说法各自讲清。
- **没走的那条：** 当场在 **平台** 条下补一句，把「目录声明的平台」与「识别判定的平台」立成两个说法。
- **建议留：** 这条，等票 `gui-looks-like-the-design/28`（平台纠正）一起立：那张票会带来第三个——人纠正过的平台——三个说法与「刮削、报告各按哪一个算」该一次定下来，现在只立两个，下一张票还得回头改这一条。
- **谁来裁：** 拿主意的人
- **状态：** settled（第五轮收口，2026-09-23，见顶上裁决）