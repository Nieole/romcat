# 12 — 中文覆盖率数的是标题集合

**What to build:** 维护者看报告里那个「中文覆盖率」，它回答的是**刮削到底采到了多少中文**
——不被「我用哪个当显示名」这个后续选择抹掉。报告与详情面板从此说同一件事。

眼下一部作品的显示标题被裁成非中文之后，报告那一侧数「有中文标题的作品」与详情面板
摆出来的那一行会各说一套，维护者会以为其中一处是 bug。（挂账 `D163`）

**Blocked by:** None — can start immediately

**Status:** done

- [x] 报告口径改成数「**标题集合里有中文的作品**」，不是「显示标题是中文的作品」。
      `crates/core/src/title/report.rs` 的 `TitleReport::build`：判据从
      `chosen.language == Language::Chinese` 换成 `best_chinese(set, priorities)`
      ——**与详情面板同一个判据**。中文那几栏（按类型 / 按置信度 / 官方译名的来路 /
      低置信那一档 / 两组例子）跟着一起换口径，好与总数对得上（挂单 `Q306`）。
- [x] 详情面板**不动**——`crates/core/src/catalog/detail.rs` 与
      `crates/gui/src/browse.rs` 一个字节都没改（`git diff --stat` 里没有这两个文件）。
      **保留**：票正文说面板「摆的是显示标题」，实际摆的是
      `title::best_chinese`（标题集合里中文那一档的第一名）——见挂单 `Q305`。
      结论不变：面板确实不用动，而且新口径正好与它一致。
- [x] 报告里那个数旁边说清它数的是什么，一句话，用词表的词。
      `render_text` 的「中文覆盖」一节，实测印出来是：
      「**2 个作品的标题集合里有中文叫法**（一共 3 个作品）。数的是**标题集合**里有没有
      中文，不是**显示标题**是不是中文——显示标题是后一步的选择（裁决可以把它定成英文名），
      而这个数要回答的是**刮削到底采到了多少中文**，不该被后一步抹掉。」
      用的是词表里的「作品」「标题集合」「叫法」「显示标题」。
      新加的 `chinese_not_displayed` 再补一句：「其中 **1 个作品的显示标题不是中文**——
      那条中文叫法照旧在集合里，详情面板摆的也照旧是它。所以下面「显示标题的回退链」
      那一栏的中文数**与这里对不上，而那是对的**：那一栏问的是「最后挑了哪一条」，
      这里问的是「采到了没有」。」
      （`/code-review` 抓到这句原先写成「因此比这里少 N 个」——那是一条**不成立**的等式：
      标题集合是空的时候显示标题退回作品名，作品名带汉字就按中文计，那种作品进得了
      「显示标题按语言」的中文那一栏却一条中文叫法都没采到。已改成不作减法，
      字段文档里也写明了另一半从哪来。）
- [x] 有测试钉着：一部作品采到了中文名但显示标题被裁成英文，报告仍然把它算进覆盖率。
      `crates/core/tests/titles.rs::显示标题被裁成英文之后中文覆盖照旧算它`。
      **实测数**：人裁一个英文显示标题之后，`chosen.display == "Contra"`、
      `chosen.language == English`，而 `chinese_works` 照旧是 **2**
      （旧口径这时会掉到 **1**），`chinese_not_displayed` 是 **1**；
      同一趟里 `VariantDetail::chinese_title()` 仍是「魂斗罗」——
      **报告与详情面板说的是同一件事**。先红（`chinese_not_displayed` 不存在，编译不过）后绿。
- [x] 有测试钉着：一部作品一条中文名都没采到，报告不把它算进去。
      `crates/core/tests/titles.rs::一条中文叫法都没采到的作品不算进中文覆盖`：
      Zelda 的标题集合里一条中文都没有 → `chinese_works == 2`（3 个作品里只算 2 个），
      而 `VariantDetail::chinese_title()` 也是 `None`。
- [x] 挂账 `D163` 标 `settled` 并迁回票 `rom-metadata-automation/25`。
      `.scratch/rom-metadata-automation/deferred.md` 的 `D163` 状态改成 **settled（走 A）**，
      决定与证据迁进 `.scratch/rom-metadata-automation/issues/25-gui-browse-and-sublibraries.md`
      末尾「挂账裁决（第三轮收口迁回）」一节。

**挂单**：`Q304`（真库上改口径后那个数没量过，`docs/library-facts.md` 的 6,501 原样留着）、
`Q305`（票正文把详情面板摆的东西说错了）、`Q306`（子栏跟着换口径）、`Q307`（口径变了、
字段名与 JSON 键没变）。号段 `Q308`–`Q313` 留空。
