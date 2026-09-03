# 27 — Switch 识别

**What to build:** Switch 平台纳入管理。库里是 `.xci` 9 个、`.nsp` 82 个、`.nsz` 48 个、`.xcz` 7 个，合计 223.84 GiB。

**这票的好消息是不用碰解密。** Switch 的内容主体加密，但**容器层（PFS0 / HFS0 的文件名表）全是明文**，免密钥就能识别到「哪个游戏的哪个版本」。**这个工具绝不内置、绝不分发任何密钥。**

坏消息有两个，都得在这票里正面处理。

**Blocked by:** 07

**Status:** done

- [x] 解析 PFS0（`.nsp`）与 HFS0（`.xci`）的明文文件名表，不做任何解密
      —— `identify::switch::probe`（`crates/core/src/identify/switch.rs`）。PFS0 与 HFS0
      共用一份 `rows()`，只切条目大小（`0x18` / `0x40`）；XCI 走卡带头 → 根 HFS0 →
      `secure` 分区三跳，两处 `"HEAD"`（`0x100` / `0x1100`）都探、不硬编码。
      **代码里没有一个密钥常量、没有一次解密调用。** 单测
      `明文文件名表读得出_不必解密` / `xci_的两套偏移都探得到` /
      `scene_风格的卡带三个分区都在`。真机：`romcat switch read` 读一份 6.80 GiB 的
      `.xci` 只读 **5,632 字节**（走到 368 MB 处的 `secure` 分区），一份 479 MiB 的
      `.nsp` 读 **5,008 字节**。

- [x] 从 `.tik` 文件名前 16 位十六进制取出 TitleID（调研实测 71,182 个样本零反例）
      —— `switch::rights_id`：前 16 位是 TitleID、末 2 位是 KeyGeneration，**中间 14 位
      不是全零就整条拒**。单测 `票据文件名的前十六位就是_title_id`。真机 134 份容器里
      **70 份**靠它读出 TitleID。

- [x] 由 NCA content ID 反查 (TitleID, 版本)——调研实测 173,502 个 content ID **100% 唯一映射**到单个 (titleId, version)
      —— `titledb::store::Store::content`，数据来自 `cnmts.json`。**真机取过真数据**：
      `romcat switch sync` 读出 **173,639 条** ContentId → (TitleID, 版本)，涉及 39,657
      个 TitleID；识别时 **66 / 134 份**反查出精确版本。
      ⭐ 自动通过还有第二道闸，是真机数据逼出来的：67 条反查里 **60 条恰好漏一个**
      ContentId（Meta NCA 按构造不在自己的清单里），而漏 5 / 6 / 13 / 24 个的那 7 条
      逐条看过全是「一个容器里装着不止一档内容」（`(1G+1U+9D)(MOD14)` 整合卡、双游戏
      合集、本体加更新打包的三部曲）——它们落进中置信进队列，不自动通过。
      集成测试 `容器里另有一批查不到的_content_id_就不自动通过`。

- [x] `.nsz` / `.xcz` 能识别，不要求先解压还原
      —— 容器层与 `.nsp` / `.xci` 完全相同，压缩只把内部条目改成 `.ncz`，32 位 hex 的
      词干原样保留，所以 `content_id()` 一条判据认四种写法。单测
      `压缩过的照样认_一个字节都不解压`。
      ⭐ **真机上补了一个洞**：`nsz` / `xcz` 不在 `classify::BARE_FILES` 里，那 55 个文件
      （74.67 GiB、Switch 容量的 33.4%）**此前连变体都不是**——SWITCH 变体因此从 91 涨到
      **146**。单测 `switch_的四种扩展名都是裸文件`。

- [x] **No-Intro 的每日镜像里没有 Switch DAT**（调研把 334 个 DAT 全查过，命中 0 个），因此这票的识别依据来自第三方 TitleID 数据库而非 DAT——数据源的许可与可自动化获取性要在实现时确认
      —— 数据源是 [blawar/titledb](https://github.com/blawar/titledb)：**许可 MIT**、
      每日推送、`raw.githubusercontent.com` 单文件 `GET`（**绝不 `git clone`**，仓库约
      44 GB）。**走票 06 那道闸门，一个主机都没往白名单里加**——`raw.githubusercontent.com`
      本来就在上面，`datomatic.no-intro.org` 照旧被 `dat::guard` 拦死。真机实测：
      取一次 **18 秒 / 276 MB**，再取一次 `ETag` 没变、**5 秒整件跳过、零字节**。
      「DAT 库里 SWITCH 平台一条记录都没有」这条无判据理由**从 53 归零**。

- [x] 中文按**语言属性**处理而不是独立发行版（ADR-0019）：调研实测官中覆盖 8,447 个 TitleID，而港服与美服 **69.4% 共用同一个 TitleID**
      —— `Title::region()`：一个 TitleID 出现在几个区就写 `World`，只在一个区出现的才写
      那个区；`languages` 取**几个区的并集**（只取一个区，中文就没了）。名字折成
      `正题 (地区) (语言)`，于是下游 `naming::parse` → `release.languages` →
      `title::Seam::LanguageField` 一处都不必为 Switch 分叉。
      真机复现：完整数据上**官中 9,141 个 TitleID，其中 5,971 个（65.3%）多区共用**；
      落到这个库上，SWITCH 变体挂着的发行版 **`World` 47 个（带 `Zh` 的 45 个，走语言
      属性）／`Hong Kong` 11 个（亚洲独占 SKU，走独立发行版）／`USA` 2 个**——裂缝两侧
      都真的存在。SWITCH 命中里**官中 129 个（88.4%）**。
      单测 `多区共用同一个_title_id_时地区是多区不是港版` /
      `同一个_title_id_出现在几个区就合成一条多区共用的发行`。

- [x] 导出时标记 `.nsz` / `.xcz` 的可见性问题：**ES-DE 不认这两种扩展名**，用户库里 33.4% 的 Switch 容量在前端默认是隐形的。这属于能力档案的范畴（ADR-0017），本票只需保证信息传得到那里
      —— 能力档案那一档票 21 就写好了（`capability/profiles.toml` 的 `es-de` × SWITCH
      只吃 `nsp / xci / nca / nro / nso`）。**信息此前传不过去的原因不在能力档案，而在
      那些文件根本不是变体**；上面第 4 条补上之后链路自己就通了。集成测试
      `压缩过的那些走到能力档案时_es_de_说它吃不下` 走完整条：扫描 → 成型 → 变体 →
      `capability::decide` 报 `Unsupported`（要 `nsp`），并用没压缩的那一份作对照组。
      识别侧另外把这件事写进**依据**、报告与 CLI 各一行——真机 **44 份**。

## 从挂账转来的前瞻

无。

## 挂账裁决

无（本票新开 D142–D146，见 `deferred.md`）。
