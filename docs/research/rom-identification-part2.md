# 模拟器 ROM 自动识别技术调研 · 第二部分（补齐 8 个平台）

> **调研日期**：2026-08-31
> **本文定位**：`rom-identification.md`（第一部分）覆盖了 18 个平台（NES、SNES、GB/GBC、GBA、NDS、3DS、N64、MD、SMS/GG、PCE、PS1、PS2、PSP、Saturn、DC、GC/Wii、街机/MAME）。**本文补齐它没有覆盖的 8 个平台**，不重复第一部分已有内容。第一部分的第 B/C/D 章（DAT 库通论、汉化识别通论、分层流水线设计）对本文 8 个平台同样适用，本文只写它们各自的差异点。
>
> **来源约束**：仅采用一手来源 —— 硬件/格式文档、模拟器与工具源码、数据库项目自身说明。每条结论给 URL。
>
> **标注约定**：
> - **【事实】**（不加标记的正文默认为此类）= 有一手来源 URL 直接支撑
> - **【推断】** = 本文基于事实的推理，**未经来源直接证实**
> - **【实测】** = 本次调研中直接下载数据文件 / 读取源码验证的结果，附可复现命令
> - **【未找到权威来源】** = 明确没找到，**不猜**
>
> ⚠ **本次调研全程未访问 `datomatic.no-intro.org`**。所有 No-Intro DAT 均通过 GitHub 镜像 [`hugo19941994/auto-datfile-generator`](https://github.com/hugo19941994/auto-datfile-generator) 的 release 产物（每 24 小时自动重建）与 [`libretro/libretro-database`](https://github.com/libretro/libretro-database) 获取。

---

## 目录

- [第 0 章 · 核心结论速览](#第-0-章--核心结论速览)
- [第 1 章 · 数据库覆盖全景（实测）](#第-1-章--数据库覆盖全景实测)
  - [1.1 No-Intro](#11-no-intro8-个平台的全部相关-dat实测) · [1.2 ⚠ Redump 必须走 `redump.info`](#12-redump必须走-redumpinfo旧镜像少-46-个系统实测) · [1.3 TOSEC](#13-tosec8-个平台共-53-个软件-dat实测) · [1.4 中文汉化覆盖](#14-中文汉化覆盖全-8-平台合计-4-条--3-个游戏实测) · [1.5 MAME software list](#15-mame-software-list-覆盖实测) · [1.6 RetroAchievements](#16-retroachievements--rcheevos-支持实测)
- [第 2 章 · 高成本组：现代平台](#第-2-章--高成本组现代平台)
  - [2.1 PlayStation Vita](#21-playstation-vita) —— SFO / work.bin / PKG / 8 种成型
  - [2.2 Wii U](#22-wii-u) —— WUD 明文头 / TMD / meta.xml / 7 种成型
  - [2.3 Xbox 360](#23-xbox-360) —— XEX / XDVDFS / STFS / GOD
- [第 3 章 · 低成本组：老掌机与老光盘机](#第-3-章--低成本组老掌机与老光盘机)
  - [3.1 WonderSwan / WonderSwan Color](#31-wonderswan--wonderswan-color) —— 头在**文件末尾** 16 字节
  - [3.2 Neo Geo Pocket / Color](#32-neo-geo-pocket--color) —— 开头 64 字节，最容易的一个
  - [3.3 Atari Lynx](#33-atari-lynx) —— `.lnx` / `.lyx` / `.bll` 三套并存
  - [3.4 N-Gage](#34-n-gage) —— 建议跳过
  - [3.5 3DO Interactive Multiplayer](#35-3do-interactive-multiplayer) —— Opera 卷头，证据链最完整
- [第 4 章 · 汇总与优先级建议](#第-4-章--汇总与优先级建议)
  - [4.1 ⭐ 八平台总表](#41--八平台总表) · [4.2 行动建议](#42-一句话行动建议) · [4.3 ⚠ 被推翻的三个判断](#43--本次调研中被推翻的三个判断务必记住) · [4.4 与第一部分的衔接](#44-与第一部分的衔接)
- [附录 · 一手来源清单](#附录--一手来源清单)
- [调研方法与局限](#调研方法与局限)

## 第 0 章 · 核心结论速览

### 0.1 一句话结论

**这 8 个平台分成截然不同的两类**：老掌机（WS/NGPC/Lynx）与 3DO 的识别成本极低、DAT 覆盖完整、可以直接复用第一部分的哈希流水线；而 PSV / Wii U / Xbox 360 的"ROM"根本不是一个文件，而是**一整棵目录树**，识别的主战场从"算哈希"转移到了"**先认出这是哪种成型形态，再从内部元数据里读出 TitleID**"。N-Gage 则是一个近乎空白的平台，**不值得投入**。

### 0.2 五个最有操作价值的发现

1. **⭐ 现代三平台的识别主键都是 TitleID，且都能在不解密的前提下读到**
   - PSV：`sce_sys/param.sfo` 的 `TITLE_ID` 键（`PCSE-00844` 形态）
   - Wii U：`meta/meta.xml` 的 `<title_id>` / TMD 内的 Title ID（`0005000101004b000` 形态）
   - Xbox 360：`default.xex` 的 Execution Info 可选头（4 字节 TitleID，如 `5841098F`）
   **No-Intro 的 DAT 自己就把这三个字段作为 `<game_id>` / `serial` 属性存了下来**【实测】，等于官方背书了"TitleID 是主键"这个设计。

2. **⭐⭐ `redump.org` 是 2026-06-20 起冻结的旧镜像，Redump 已迁至 `redump.info` —— 差 46 个系统**【实测】
   `old.redump.info` 首页公告逐字：
   > **Announcement: Official move to redump.info!** — Posted by Deterous at June 20 2026, 01:49:45 — Redump has officially moved to the redump.info website and **this mirror will no longer be updated.**

   | | 系统数 | Wii U | Xbox 360 | 3DO | Symbian | PS Vita |
   |---|---|---|---|---|---|---|
   | 旧站 `redump.org`（及所有据其抓取的镜像） | **60** | ❌ 无 | 3691 | 672 | ❌ 无 | ❌ 无 |
   | **现站 `redump.info`** | **106** | ✅ **541** | **3707** | **669** | ✅ **3** | ❌ 无 |

   ⚠ **`hugo19941994/auto-datfile-generator` 的 `redump.py` 硬编码 `URL_DOWNLOADS = "http://redump.org/downloads/"`，因此它产出的 `redump.xml` / `redump.zip` 同样是旧数据。** 旧站对游客的表现极具误导性：`redump.org/discs/system/wiiu/` 返回 HTTP 200、系统条目存在、却显示 "No discs found."
   **→ 抓取 Redump 必须直接走 `https://redump.info/datfile/<CODE>`。**（No-Intro 侧的 GitHub 镜像不受影响，仍然可用。）

   **修正后的结论：Redump 有 Wii U（541 条）与 Xbox 360（3707 条）；PS Vita 确实没有**（106 个系统短码中无 `PSV`/`VITA`，因为 Vita 卡带是 MMC 闪存卡而非光盘）。

3. **⭐ 三个平台的"成型"是目录树，No-Intro 逐文件哈希，条目数爆炸**【实测】
   | DAT | game 数 | rom 数 | rom/game |
   |---|---|---|---|
   | `Unofficial - Sony - PlayStation Vita (NoNpDrm)` | 452 | **431 605** | 954 |
   | `Nintendo - Wii U (Digital) (CDN)` | 3682 | **168 387** | 46 |
   | `Microsoft - Xbox 360 (Digital)` | 17438 | 35 124 | 2.0 |
   对这类平台，**"整目录逐文件比对"在工程上不可行**，必须退化为"读 TitleID + 少量关键文件哈希"。

4. **⭐ 3DO 有现成的、经两个独立来源交叉验证的结构化识别算法**
   Opera 文件系统的卷头就在第 0 扇区，**前 132 字节**，`0x28` 处 32 字节是卷标（Volume Label），`0x48` 处 4 字节是卷唯一标识符。RetroAchievements 的 `rc_hash_3do()` 用 `MD5(132 字节卷头 ‖ LaunchMe 文件内容)` 作为哈希 —— 这是一个**天然抗"只改数据文件"的汉化**的结构不变量。见 [3.5](#35-3do-interactive-multiplayer)。

5. **⭐ 这 8 个平台的中文汉化覆盖几乎为零：TOSEC 全库只有 4 条、3 个游戏**【实测】
   下载 TOSEC 官方 2025-03-13 release 中这 8 个平台的**全部 53 个软件 DAT**（2240 个 game 条目）并解析 `[tr xx]` 标记，`[tr zh]` 合计只有 **4 条**：
   | 平台 | 条目 | CRC32 |
   |---|---|---|
   | NGPC | `Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]` | `4e3191c6` |
   | NGPC | `Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]` | `8aded757` |
   | NGPC | `Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh][a]` | `c2c5bcb4` |
   | WSC | `Kidou Senshi Gundam Seed (2003-03-15)(Bandai)[tr zh][v.20040120]` | `b338cae2` |
   其余 81 条翻译版是俄语 27、英语 38、葡语 7、西语 4、法语 4、德语 1。
   **Lynx / 3DO / N-Gage / PSV / Wii U / Xbox 360 六个平台的 `[tr zh]` 全部为 0。**

### 0.3 必须接受的现实

| 事实 | 影响 |
|---|---|
| **RetroAchievements 对这 8 个平台里的 4 个完全没有哈希实现**【实测】 | `rc_consoles.h` 里 `RC_CONSOLE_WII_U = 20`、`RC_CONSOLE_XBOX = 22`、`RC_CONSOLE_NOKIA_NGAGE = 61` 三个 ID 已定义，但 `hash.c` 里**一个 case 分支都没有**；PS Vita **连 enum 都没有**（Sony 系只到 PS3）。第一部分推荐的"复用 rc_hash 作结构不变量"策略在现代平台上不可用 |
| **N-Gage 的数据库覆盖近乎为零**【实测】 | No-Intro `Nokia - N-Gage (WIP)` 全文只有 `Worms World Party (USA)` **1** 条（33 546 240 字节 `.bin`，停更于 2022-02-20）；TOSEC 的 2 个 N-Gage 集共 **5** 条，全是同一款原型 `8 Kings`；libretro-database **完全没有** N-Gage。约 60 款商业游戏基本无覆盖 |
| **PSV 与 Wii U 数字版的"ROM"没有稳定文件哈希** | VPK 是第三方重打包（打包器版本、压缩参数一变哈希就变）、NoNpDrm 是逐文件目录树、Wii U CDN 是 NUS 内容目录、GOD 是分片 `Data0000..` 目录。**光盘侧例外**：Redump 的 Xbox 360 ISO（3707 条）与 Wii U ISO（541 条，全部恰为 25 025 314 816 字节）都有稳定的整文件 CRC32/MD5/SHA-1 |
| **WS 头的两份主要文档相差 6 个字节，且存档容量码有真实分歧**【实测】 | `wstech24.txt`（Mednafen 随附）只定义**最后 10 字节**且把 `$9` 写成 `??`；WSdev Wiki 定义的是**最后 16 字节**并给出 `$9` = Game version / Safe mode。存档类型码 `$01/$02/$05` 上 WSdev 与「wstech24 + MAME + Mednafen」三家不一致，见 [3.1.5](#315-存档类型码b--存在真实分歧) |
| **No-Intro 的 WonderSwan DAT 一个 `serial` 属性都没有**【实测】 | 257 + 242 条 WS/WSC 条目中 `serial=` 出现 **0 次**。反观 NGPC 有 118/128 条带 serial。WS 的"厂商 ID + 卡带 ID"没有被 No-Intro 当作序列号收录 |

### 0.4 立即可做的四件事

| # | 行动 | 收益 |
|---|---|---|
| 1 | **先做 WS / NGPC / Lynx / 3DO 四个平台** | 四者合计约 3 人日，DAT 覆盖完整，直接复用第一部分的哈希流水线 |
| 2 | **对 Lynx 实现 64 字节 LNX 头剥离** | 同一游戏在 No-Intro 里以 `.lnx`(含头) / `.lyx`(去头) / `.bll` 三套并存，不剥头会漏掉一半 |
| 3 | **对 PSV/Wii U/X360 只做"TitleID 识别"，放弃整目录哈希** | 三个平台各 1–2 天即可拿到主键，而整目录比对是 10 倍工作量且收益为负 |
| 4 | **N-Gage 直接跳过** | No-Intro 仅 **1** 条（且停更于 2022-02），TOSEC 仅 **5** 条（全是同一款原型 8 Kings），约 60 款商业游戏基本无覆盖 |

---

## 第 1 章 · 数据库覆盖全景（实测）

> **⭐ 正确的复现方法**（三个库各有一个坑，都已踩过并修正）
> ```bash
> # ① No-Intro —— 禁止访问 datomatic，走每日自动重建的 GitHub 镜像（334 个 DAT）
> curl -sL -o no-intro.xml https://github.com/hugo19941994/auto-datfile-generator/releases/latest/download/no-intro.xml
> curl -sL -o no-intro.zip https://github.com/hugo19941994/auto-datfile-generator/releases/latest/download/no-intro.zip
>
> # ② Redump —— ⚠ 必须直连 redump.info（106 个系统）。
> #    不要用 auto-datfile-generator 的 redump.xml/redump.zip：它硬编码了已冻结的 redump.org（仅 60 系统，缺 Wii U）
> curl -sL https://redump.info/downloads/                    # 列出全部系统短码
> curl -sL -o WIIU.zip https://redump.info/datfile/WIIU      # 单个系统的 DAT
>
> # ③ TOSEC —— ⚠ 必须用 git/trees 递归列举（4502 个 DAT）。
> #    GitHub 的 /contents API 在 1000 条处硬截断且不报错
> SHA=$(gh api repos/smesgr9000/TOSEC-DAT/commits/main --jq .commit.tree.sha)
> gh api "repos/smesgr9000/TOSEC-DAT/git/trees/$SHA?recursive=1" --jq '.tree[] | select(.type=="blob") | .path'
> ```
> 实测时间：**2026-08-31**。No-Intro **334** 个 DAT；Redump（`redump.info`）**106** 个系统；TOSEC **4502** 个 DAT（TOSEC 3111 + TOSEC-ISO 300 + TOSEC-PIX 1091）。

### 1.1 No-Intro：8 个平台的全部相关 DAT【实测】

| DAT 名称 | 版本 | game 数 | rom 数 | rom 形态 |
|---|---|---|---|---|
| `Atari - Atari Lynx (LNX)` | 20260625-122811 | 18 | 18 | `.lnx`（含 64 字节头） |
| `Atari - Atari Lynx (LYX)` | 20260625-122811 | 155 | 155 | `.lyx`（裸 ROM） |
| `Atari - Atari Lynx (BLL)` | 20260625-122811 | 3 | 3 | `.bll` |
| `Bandai - WonderSwan` | 20260525-011654 | 257 | 257 | `.ws` |
| `Bandai - WonderSwan Color` | 20260525-011610 | 242 | 242 | `.wsc` |
| `SNK - NeoGeo Pocket` | 20250904-215533 | 13 | 13 | `.ngp` |
| `SNK - NeoGeo Pocket Color` | 20260626-085623 | 128 | 128 | `.ngc` |
| `Nokia - N-Gage (WIP)` | **20220220-010530** | **1** | **1** | `.bin`（MMC 卡镜像） |
| `Mobile - Symbian` | 20220516-232715 | 27 | 27 | `.sis` |
| `Non-Redump - Panasonic - 3DO Interactive Multiplayer` | 20250115-113934 | 12 | 21 | `.iso` 或 `.cue`+`.bin` |
| `Source Code - Panasonic - 3DO Interactive Multiplayer` | 20250820-041043 | 7 | 11 | 源码 |
| `Sony - PlayStation Vita (PSN) (Content)` | 20260116-223543 | 17505 | 33740 | `.pkg` + `work.bin` |
| `Sony - PlayStation Vita (PSN) (Updates)` | 20260415-165728 | 3976 | 3976 | `.pkg` |
| `Unofficial - Sony - PlayStation Vita (VPK)` | 20260703-183310 | 240 | 240 | `.vpk` |
| `Unofficial - Sony - PlayStation Vita (NoNpDrm)` | 20260703-183310 | 452 | **431605** | 整目录逐文件 |
| `Unofficial - Sony - PlayStation Vita (PSVgameSD)` | 20260703-183310 | 383 | 470 | — |
| `Unofficial - Sony - PlayStation Vita (BlackFinPSV)` | 20260703-183310 | 67 | 67 | — |
| `Unofficial - Sony - PlayStation Vita (PSN) (Decrypted) (NoNpDrm)` | 20220715-105412 | 1 | 89 | 整目录 |
| `Unofficial - Sony - PlayStation Vita (PSN) (Decrypted) (VPK)` | 20220715-105412 | 50 | 50 | `.vpk` |
| `Nintendo - Wii U (Digital) (CDN)` | 20260618-192853 | 3682 | **168387** | NUS 内容目录 |
| `Nintendo - Wii U (Digital) (CDN) (Dev)` | 20220718-071500 | 1137 | 146252 | 同上 |
| `Nintendo - Wii U (Digital) (CDN) (Lotcheck)` | 20220718-071500 | 159 | 13227 | 同上 |
| `Nintendo - Wii U (Development Kit Hard Drives)` | 20250424-141246 | 3 | 5 | `.img` 整盘 |
| `Non-Redump - Nintendo - Wii U` | 20260312-235110 | **3** | 4 | `.wud` + `.key` |
| `Unofficial - Nintendo - Wii U (Digital) (Deprecated)` | 20191222-002825 | 142 | 20946 | 已废弃 |
| `Microsoft - Xbox 360 (Digital)` | 20260626-214314 | 17438 | 35124 | 内容目录树 |
| `Microsoft - Xbox 360 (Development Kit Hard Drives)` | 20260509-195531 | 126 | 155 | 整盘 |
| `Non-Redump - Microsoft - Xbox 360` | 20260525-160908 | 33 | 5365 | — |
| `Unofficial - Microsoft - Xbox 360 (Title Updates)` | 20220623-103723 | 1409 | 1409 | TU 文件 |

> 📌 **No-Intro 的 DAT 格式**：本次实测的所有 No-Intro DAT 均使用自有 XSD（`schema_nointro_datfile_v3.xsd` / `v4.xsd`，与第一部分 B.1.3 一致），`<rom>` 上带 **crc + md5 + sha1 + sha256** 四套哈希；`Sony - PlayStation Vita (VPK)` 与 `Nintendo - Wii U (Digital) (CDN)` 等还带 `<game_id>` 元素与 `serial` 属性。

### 1.2 Redump：必须走 `redump.info`，旧镜像少 46 个系统【实测】

> ⚠⚠ **本次调研中修正的第二个错误，也是最严重的一个。**
> 最初通过 `hugo19941994/auto-datfile-generator` 的 `redump.xml` 得到「Redump 只有 60 个系统，没有 Wii U 也没有 PS Vita」。**这是错的** —— 该生成器的 `redump.py` 硬编码 `URL_DOWNLOADS = "http://redump.org/downloads/"`，而 **`redump.org` 自 2026-06-20 起已是冻结镜像**。
> `old.redump.info` 首页公告逐字：
> > **Announcement: Official move to redump.info!** — Posted by Deterous at June 20 2026, 01:49:45 — Redump has officially moved to the redump.info website and **this mirror will no longer be updated.**
>
> 旧站对游客的表现极具误导性：`redump.org/discs/system/wiiu/` 返回 **HTTP 200**、系统条目存在、但显示 "No discs found."
> **复现正确做法**：
> ```bash
> curl -sL https://redump.info/downloads/          # 106 个系统短码
> curl -sL -o WIIU.zip https://redump.info/datfile/WIIU
> ```

| | 系统数 | Wii U | Xbox 360 | 3DO | Symbian | PS Vita |
|---|---|---|---|---|---|---|
| 旧站 `redump.org`（及一切据其抓取的镜像） | **60** | ❌ | 3691 | 672 | ❌ | ❌ |
| **现站 `redump.info`**（2026-08-31 实测） | **106** | ✅ **541** | **3707** | **669** | ✅ **3** | ❌ |

**本文 8 个平台在 `redump.info` 上的现状**：

| DAT | 版本 | game | rom | 形态 |
|---|---|---|---|---|
| **`Nintendo - Wii U`** | 2026-08-28 18-36-53 | **541** | 541 | ⭐ **全部 `.iso`，且尺寸全部恰为 `25 025 314 816`（1 : 1，无 sidecar）** |
| **`Microsoft - Xbox 360`** | 2026-08-30 14-20-37 | **3707** | 3716 | 3698 `.iso` + 9 `.cue` + 9 `.bin` |
| **`3DO Interactive Multiplayer`** | 2026-08-27 13-28-57 | **669** | 1338 | `.cue` + `.bin`，恰好各 669 |
| **`Handheld - Symbian`** | 2026-08-26 23-28-46 | **3** | 6 | `.cue` + `.bin`，见下 |
| `Handheld - Psion` | 2026-08-13 18-09-14 | 3 | 6 | 参照 |
| PS Vita | — | **0** | — | **106 个系统短码中无 `PSV`/`VITA`** |
| WonderSwan / NGPC / Lynx | — | 0 | — | 卡带机，不在 Redump 范围 |

⚠ **`Handheld - Symbian` 的 3 条不是 N-Gage 游戏卡**，而是光盘（`.cue`+`.bin`）：
```xml
<game name="Force Unleashed, The (UK)" id="117964">   <category>Games</category>
<game name="N-Gage (Europe)" id="136809">             <category>Applications</category>
<game name="N-Gage QD (Europe)" id="137375">          <category>Applications</category>
```
后两条 `category` 是 **Applications** —— 是随机附送的 **PC 配套软件光盘**，不是游戏本体。**对 N-Gage 游戏识别没有帮助。**

Redump DAT 一律使用 **Logiqx DTD**（`http://www.logiqx.com/Dats/datafile.dtd`），只有 **crc + md5 + sha1，没有 sha256**（与 No-Intro 的四套哈希形成对比），也没有 `serial` 属性。

### 1.3 TOSEC：8 个平台共 53 个软件 DAT【实测】

> ⚠ **本次调研中修正的一个错误**：最初用 GitHub `contents` API 列举 TOSEC 镜像的目录，该 API **在 1000 条处硬截断且不报错**，导致误判「TOSEC 没有 NGP/NGPC/N-Gage/PSV 集」。改用 `git/trees?recursive=1` 后拿到完整的 **4502 个 DAT**（TOSEC 3111 + TOSEC-ISO 300 + TOSEC-PIX 1091），与 TOSEC 官方 2025-03-13 release 的规模吻合。
> 复现：`gh api "repos/smesgr9000/TOSEC-DAT/git/trees/$(gh api repos/smesgr9000/TOSEC-DAT/commits/main --jq .commit.tree.sha)?recursive=1" --jq '.tree[].path'`

| 平台 | 软件 DAT 数 | game 条目 | `[tr *]` | `[tr zh]` | 主要 DAT |
|---|---|---|---|---|---|
| **Atari Lynx** | 11 | 677 | 1 | 0 | Games [LNX] 347 / Demos [LNX] 191 / Homebrew [LNX] 103 / Games [LYX] 10 / Games [O] 5 / Applications 9 / Firmware 2 / Compilations 2+2 / Demos [Multipart] 2 / Homebrew [O] 4 |
| **3DO** | 15 | 679 | 32 | 0 | Games 459 / Multimedia 48 / Educational [ISO] 40 / Samplers 39 / Coverdiscs [ISO] 24 / Educational [IMG] 17 / Coverdiscs [IMG] 10 / Firmware 9 … |
| **WonderSwan Color** | 4 | 211 | 17 | **1** | Games 172 / Demos 31 / Applications 7 / Firmware 1 |
| **WonderSwan** | 4 | 208 | 7 | 0 | Games 182 / Applications 22 / Demos 3 / Firmware 1 |
| **NGPC** | 7 | 371 | 26 | **3** | Games 311 / Demos 37 / Multimedia 8 / Samplers 8 / Applications 5 / Firmware 1 / Educational 1 |
| **PS Vita** | 6 | 41 | 0 | 0 | Homebrew Applications [VPK] 27 / Homebrew Games [VPK] 8 / Demos 2 / Homebrew Applications [VELF] 2 / … **全是 homebrew 与 demo，无商业游戏** |
| **N-Gage** | 2 | **5** | 0 | 0 | Games [SIS] 4（**全是同一款原型 `8 Kings`**）/ Games [Multipart] 1（2933 个文件级条目） |
| **NGP** | 2 | 31 | 2 | 0 | Games 29 / Firmware 2 |
| **Xbox 360** | 2 | 17 | 0 | 0 | Samplers 15 / Coverdiscs 2 —— **没有游戏集** |
| **Wii U** | **0** | — | — | — | 只有 TOSEC-PIX 的手册/宣传片扫描（5 个 DAT） |
| **合计** | **53** | **2240** | **85** | **4** | |

要点：
- **Lynx 在 TOSEC 里以带头 `.lnx` 为主**（Games [LNX] 347 vs [LYX] 10），与 No-Intro 主集是无头 `.lyx` **正好相反**。
- **3DO 是 `.cue` + `.iso` 配对**（Redump 是 `.cue` + `.bin`）。
- **TOSEC 的 WS 命名把卡带序列号写进文件名**，如 `Chocobo no Fushigi na Dungeon for WonderSwan Rev 1 (1999)(Bandai)[tr en][SWJ-BAN002]` —— 这是**唯一**能把印刷序列号与 ROM 哈希关联起来的公开数据库。
- **Wii U 与 Xbox 360 在 TOSEC 里没有可用的游戏集。**

### 1.4 中文汉化覆盖：全 8 平台合计 **4 条 / 3 个游戏**【实测】

对上表全部 53 个 DAT 解析 `<game name>` 中的 `[tr xx]` 标记，`[tr zh]` 的**全部条目**：

| 平台 | 条目名 | size | CRC32 | MD5 | SHA-1 |
|---|---|---|---|---|---|
| **NGPC** | `Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]` | 2097152 | `4e3191c6` | `9b09713f…455106ec` | `df62c67041b1953a1f09abb55fb1b0f1438a98f2` |
| **NGPC** | `Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]` | 2097152 | `8aded757` | `a50f9197…640e3b2d` | `76d31e04613d145dbaa20c5377c01c91fd87c02a` |
| **NGPC** | `Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh][a]` | 2097152 | `c2c5bcb4` | `83c8ca84…4546228e` | `4ab9c1a974c41b1a9a13b57cbff510627e98e92a` |
| **WSC** | `Kidou Senshi Gundam Seed (2003-03-15)(Bandai)[tr zh][v.20040120]` | 4194304 | `b338cae2` | `b5cf3074…79aff66` | `be172f59176af9841cc6c2d9fcd019f01f76a265` |

即 **3 个不同游戏**（Metal Slug 1st Mission 有 `[a]` 备选 dump）。

**这 8 个平台合计 85 条翻译版的语言分布**：

| 语言 | 条数 | 主要来源 |
|---|---|---|
| `en` 英语 | 38 | WSC 15、WS 6、NGPC 14、3DO 3 |
| `ru` 俄语 | 27 | **全部来自 3DO**（如 `Alone in the Dark (1994)(Pony Canyon)(JP)[tr ru aliast - aspyd - FantasyNik]`） |
| `pt` 葡语 | 7 | NGPC 6、WSC 1 |
| `es` 西语 | 4 | NGP 2、Lynx 1（`Switchblade II (1992)(Atari Corp)[tr es Wave]`）、3DO 1 |
| `fr` 法语 | 4 | NGPC 3、3DO 1 |
| **`zh` 中文** | **4** | NGPC 3、WSC 1 |
| `de` 德语 | 1 | WS 1 |

**明确结论**：
- **Atari Lynx / 3DO / N-Gage / PS Vita / Wii U / Xbox 360 六个平台的中文汉化条目数为 0**，在 No-Intro、Redump、TOSEC、libretro-database、MAME software list 中均无一条。
- **唯二有中文条目的是 NGPC（3 条）与 WSC（1 条）**，全部来自 TOSEC。
- No-Intro / Redump 在政策上不收录修改版，所以这两个库对全部 8 个平台的汉化覆盖恒为 0。
- **例外：PS Vita 的「官方中文版」覆盖良好** —— 那不是汉化，是 Sony 官方本地化，No-Intro 用 `(Zh)` / `(Zh-Hant)` / `(Zh-Hans)` 语言标记收录了 **290** 条，详见 [2.1.11](#2111-中文汉化覆盖)。Xbox 360 的 Redump 集同样有约 305 条官方中文，详见 [2.3](#23-xbox-360)。

### 1.5 MAME software list 覆盖【实测】

| 列表 | HTTP | `<software>` 数 | `<info name="serial">` 数 | 备注 |
|---|---|---|---|---|
| `hash/wswan.xml` | 200 | 126 | 125 | 芯片级 `<rom name="mh8m256s033a.u3">`，带 `pcb` / `u1..u4` / `slot` feature |
| `hash/wscolor.xml` | 200 | 108 | — | 同上 |
| `hash/ngp.xml` | 200 | 10 | — | — |
| `hash/ngpc.xml` | 200 | 119 | 97 | serial 形如 `NeoP00640` |
| `hash/lynx.xml` | 200 | 119 | 79 | serial 形如 `PA2042`；ROM 是 **裸 262144 字节**（对应 `.lyx`） |
| `hash/3do.xml` | 200 | 20 | 14 | **`<disk sha1=...>`（CHD），只有 SHA-1**；带 `barcode` |
| `hash/3do_m2.xml` | 200 | 3 | — | — |
| `hash/ngage.xml` | **404** | — | — | **不存在** |

MAME 三个列表的 `serial` 与 No-Intro 的 `serial` 属性可交叉验证（见 3.2 的 NGPC 分析）。

### 1.6 RetroAchievements / rcheevos 支持【实测】

来源：[`RetroAchievements/rcheevos`](https://github.com/RetroAchievements/rcheevos)（`develop` 分支）

| 平台 | `rc_consoles.h` 里的 enum | `hash.c` 里的哈希实现 |
|---|---|---|
| Atari Lynx | `RC_CONSOLE_ATARI_LYNX = 13` | ✅ `rc_hash_lynx()` —— **剥 64 字节 LYNX 头**后 MD5 |
| Neo Geo Pocket | `RC_CONSOLE_NEOGEO_POCKET = 14` | ✅ 整文件 MD5（`rc_hash_whole_file`），无任何头处理 |
| WonderSwan | `RC_CONSOLE_WONDERSWAN = 53` | ✅ 整文件 MD5，无任何头处理 |
| 3DO | `RC_CONSOLE_3DO = 43` | ✅ `rc_hash_3do()` —— **MD5(132 字节卷头 ‖ LaunchMe)** |
| Wii U | `RC_CONSOLE_WII_U = 20` | ❌ **无实现** |
| Xbox（含 360） | `RC_CONSOLE_XBOX = 22` | ❌ **无实现** |
| N-Gage | `RC_CONSOLE_NOKIA_NGAGE = 61` | ❌ **无实现** |
| PS Vita | **enum 中不存在** | ❌ Sony 系仅 PS1(12) / PS2(21) / PSP(41) / PS3(82) |

复现：
```bash
curl -sL https://raw.githubusercontent.com/RetroAchievements/rcheevos/develop/include/rc_consoles.h | grep -n "RC_CONSOLE_"
curl -sL https://raw.githubusercontent.com/RetroAchievements/rcheevos/develop/src/rhash/hash.c   | grep -c "RC_CONSOLE_WII_U\|RC_CONSOLE_XBOX\|RC_CONSOLE_NOKIA_NGAGE"   # → 0
```

---

## 第 2 章 · 高成本组：现代平台

> 本章 3 个平台的共同点：**"一个游戏"在磁盘上不是一个文件，而是一整棵目录树或"主文件 + 附属文件"**。识别的主战场从"算哈希"转移到"先认出这是哪种成型形态，再从内部元数据里读出 TitleID"。
> 三者都**没有稳定的整文件哈希**可用（唯一例外是 Redump 的 Xbox 360 ISO）。

### 2.1 PlayStation Vita

> ⭐ **本节最有价值的发现**：NoNpDrm 转储里有一个**恒定 512 字节的 `sce_sys/package/work.bin`**，它的 SHA-1 能直接命中 No-Intro 的 Vita (PSN) DAT（实测命中率 **96.8%**）。**不用摸 2 GB 本体，只哈希 512 字节就能识别一个 Vita 游戏。**

#### 2.1.1 PARAM.SFO —— 主锚点

**位置**：`<TITLEID>/sce_sys/param.sfo`（已安装/已转储目录）、`.vpk` ZIP 内的 `sce_sys/param.sfo`、`.pkg` 的 metadata type 14 指向的**明文**区域。

**头部**（psdevwiki 逐字，<https://www.psdevwiki.com/vita/PARAM.SFO>）：
```c
typedef struct{
    int magic;             // PSF
    int version;           // 1.1
    int keyTableOffset;
    int dataTableOffset;
    int indexTableEntries;
} sfo_header_t;
```

| 字段 | 偏移 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| `magic` | `0x00` | 4 | `00 50 53 46` = `"\0PSF"`（LE u32 = `0x46535000`） | **极高（magic）** |
| `version` | `0x04` | 4 | `01 01 00 00` = `0x00000101`（1.1） | 高 |
| `key_table_start` | `0x08` | 4 | 键表起始偏移 | 结构必需 |
| `data_table_start` | `0x0C` | 4 | 数据表起始偏移 | 结构必需 |
| `tables_entries` | `0x10` | 4 | 索引表条目数 | 结构必需 |

**索引表条目**（从 `0x14` 起，每条 `0x10` 字节 × `tables_entries`）：

| 字段 | 条目内偏移 | 长度 | 内容 |
|---|---|---|---|
| `key_offset` | `+0x00` | 2 | 相对 `key_table_start` 的键名偏移 |
| `alignment` | `+0x02` | 1 | 恒 `4` |
| `type` | `+0x03` | 1 | `0`=BIN，`2`=STR(UTF-8)，`4`=VAL(u32) |
| `data_len` | `+0x04` | 4 | 实际值长度（含 NUL） |
| `data_max_len` | `+0x08` | 4 | 保留区长度 |
| `data_offset` | `+0x0C` | 4 | 相对 `data_table_start` 的数据偏移 |

**两套命名的调和**：psdevwiki 把 `+0x02..+0x03` 当作一个 LE u16 `param_fmt`（`0x0004`/`0x0204`/`0x0404`），而 vitasdk / VitaShell / FAGDec 把它拆成 `alignment:u8 + type:u8`。**是同一 16 字节的两种写法**：`param_fmt = alignment | (type << 8)`。

vitasdk `vita-mksfoex.c`（写入端权威）逐字：
```c
#define PSF_MAGIC    0x46535000
#define PSF_VERSION  0x00000101
struct SfoHeader { uint32_t magic; uint32_t version; uint32_t keyofs; uint32_t valofs; uint32_t count; };
struct SfoEntry  { uint16_t nameofs; uint8_t alignment; uint8_t type; uint32_t valsize; uint32_t totalsize; uint32_t dataofs; };
#define PSF_TYPE_BIN  0
#define PSF_TYPE_STR  2
#define PSF_TYPE_VAL  4
```
序列化逻辑：`keyofs = 0x14 + count*0x10`，键表 4 字节对齐。

Vita3K（读取端）`vita3k/packages/include/packages/sfo.h` 逐字：
```cpp
enum SfoDataFormat : uint16_t {
    UTF8 = 0x0004, UTF8_NULL = 0x0204, ASCII = 0x0402, UINT32_T = 0x0404,
};
```

> ⚠ **【事实】Vita3K 完全不校验 `\0PSF` magic** —— `SfoHeader::magic` 被读入但从未比较（全仓库 grep `0x46535000` 零命中），也无任何边界检查，畸形 SFO 会越界读。**自己实现时必须校验。**

#### 2.1.2 关键 SFO 键

| 键名 | 类型 | max_len | 含义 | 可靠性 |
|---|---|---|---|---|
| **`TITLE_ID`** | utf-8 | `0x0C` | 产品码 `WXYZ12345`（9 字符 + NUL） | **极高（主键）** |
| **`CONTENT_ID`** | utf-8 | `0x30` | `XXYYYY-NP_COMMUNICATION_ID-LICENSE_ID` | **极高** |
| `TITLE` | utf-8 | `0x80` | 默认语言游戏名 | 高 |
| `TITLE_xx` | utf-8 | `0x80` | 本地化游戏名（`xx` = 两位语言码数字） | 高 |
| `STITLE` / `STITLE_xx` | utf-8 | `0x34` | 短标题 | 高 |
| **`APP_VER`** | utf-8 | `0x8` | `XX.YY`，如 `01.00` | **高（区分修订）** |
| **`CATEGORY`** | utf-8 | `0x4` | `gd`=Game Digital App / `gp`=Game Patch / `ac`=Additional Content / `gda`=System App / `gdc`=Non-Game Big App / `sd`=SaveData | **极高（决定成型分流）** |
| `PSP2_DISP_VER` | utf-8 | `0x8` | 显示用最低固件 | 中（**Vita3K 不读它**，全仓库零命中） |
| `PSP2_SYSTEM_VER` | uint32 | `0x4` | 最低固件 | 中 |
| `VERSION` | utf-8 | `0x8` | 发行版修订号（再版/GOTY 递增） | 高 |
| **`PUBTOOLINFO`** | utf-8 | `0x200` | `c_date=yyyymmdd,sdk_ver=xxxxxxxx` | **高（区分变体的好材料）** |
| `INSTALL_DIR_SAVEDATA` | utf-8 | `0xc` | 存档目录名 | 中 |
| `PARENTAL_LEVEL` / `REGION_DENY` | uint32 | `0x4` | 分级 / 区域限制位 | 中 |

**Vita3K 实际读哪些键**（`vita3k/packages/src/sfo.cpp` 逐字，值得照抄的优先级逻辑）：
```cpp
sfo::get_data_by_key(app_info.app_version, sfo_handle, "APP_VER");
if (app_info.app_version[0] == '0') app_info.app_version.erase(app_info.app_version.begin());
sfo::get_data_by_key(app_info.app_category, sfo_handle, "CATEGORY");
sfo::get_data_by_key(app_info.app_content_id, sfo_handle, "CONTENT_ID");
if (!sfo::get_data_by_key(app_info.app_short_title, sfo_handle, fmt::format("STITLE_{:0>2d}", sys_lang)))
    sfo::get_data_by_key(app_info.app_short_title, sfo_handle, "STITLE");
if (!sfo::get_data_by_key(app_info.app_title, sfo_handle, fmt::format("TITLE_{:0>2d}", sys_lang)))
    sfo::get_data_by_key(app_info.app_title, sfo_handle, "TITLE");
sfo::get_data_by_key(app_info.app_title_id, sfo_handle, "TITLE_ID");
```
即**标题优先 `TITLE_<两位语言码>`，回退 `TITLE`**；`APP_VER` 去掉前导 `0`。

#### 2.1.3 CONTENT_ID 结构

psdevwiki 逐字：*"The format is: XXYYYY-NP_COMMUNICATION_ID-LICENSE_ID."* / *"e.g.: JP0365-PCSG90004_00-SKP2TRIAL0000000"*

以 `EP4497-PCSB00381_00-0000000000000000`（37 字符）为例：

| 片段 | 字符区间 | 含义 | 可靠性 |
|---|---|---|---|
| `EP` | `[0..2)` | 区域码（`EP`=Europe、`UP`=USA、`JP`=Japan、`HP`=HongKong/Asia、`KP`=Korea、`IP`=Internal） | 高（**【推断】**：字母表由 PS3 Digital TITLE_ID 表推得，逐字来源只给了 `XXYYYY` 与 Service ID 定义） |
| `4497` | `[2..6)` | 发行商编号 | 中 |
| **`PCSB00381`** | `[7..16)` | **= `TITLE_ID`** | **极高** |
| `_00` | `[16..19)` | NP Title ID 子号 | 极高 |
| `0000000000000000` | `[20..36)` | License / Entitlement Label（16 字符） | 极高 |

`TITLE_ID = CONTENT_ID[7..16]` 由两份源码独立证实：
- Vita3K `license.cpp`：`emuenv.license_title_id = emuenv.license_content_id.substr(7, 9);`
- pkg2zip `pkg2zip.c`：`const char* id = content + 7;`
- Vita3K DLC 目录名用 `app_content_id.substr(20)`（License Label 部分）

#### 2.1.4 TITLE_ID 前缀 → 区域

**来源 A（逐字，[wiki.no-intro.org](https://wiki.no-intro.org/index.php?title=Sony_-_Playstation_Vita_undumped)** —— 注意这是**与被禁的 `datomatic.no-intro.org` 不同的主机**）：
```
* PCSA = US, 1st party      * PCSE = US, 3rd party
* PCSB = EU, 3rd party      * PCSF = EU, 1st party
* PCSC = Japan, 1st party   * PCSG = Japan, 3rd party
* PCSD = Asia/Korea, 1st    * PCSH = Asia/Korea, 3rd party

For Japan/Asia/Korea, the cover/card serial does not match the internal serial. They are:
* VCAS = Asia, 1st   * VCJS = Japan, 1st   * VCJX = Japan, 1st, demos
* VCKS = Korea, 1st  * VLAS = Asia, 3rd    * VLJM = Japan, 3rd
* VLJS = Japan, 3rd  * VLKS = Korea, 3rd
Internal serials are still the PCS.... serials and usually match the PSN release serials.
```

**来源 B（逐字，pkg2zip `get_region()`，社区事实标准）**：
```c
if (memcmp(id,"PCSE",4)==0 || memcmp(id,"PCSA",4)==0 || memcmp(id,"NPNA",4)==0) return "USA";
else if (memcmp(id,"PCSF",4)==0 || memcmp(id,"PCSB",4)==0 || memcmp(id,"NPOA",4)==0) return "EUR";
else if (memcmp(id,"PCSC",4)==0 || memcmp(id,"VCJS",4)==0 || memcmp(id,"PCSG",4)==0 ||
         memcmp(id,"VLJS",4)==0 || memcmp(id,"VLJM",4)==0 || memcmp(id,"NPPA",4)==0) return "JPN";
else if (memcmp(id,"VCAS",4)==0 || memcmp(id,"PCSH",4)==0 || memcmp(id,"VLAS",4)==0 ||
         memcmp(id,"PCSD",4)==0 || memcmp(id,"NPQA",4)==0) return "ASA";
else return "unknown region";
```

> ⚠ **两个关键陷阱**：
> 1. **`VLJM-xxxxx` / `VLAS-xxxxx` / `VCAS-xxxxx` 是印在卡带外包装上的编号，不出现在 `param.sfo` 里**。`param.sfo` 里永远是 `PCSx`。**别指望从转储里读出 `VLJM`。**
> 2. **`PCSD` / `PCSH` = 亚洲/韩国，不等于中文。** 实测 NPS 的 465 条 ASIA 中 `PCSH` 402 + `PCSD` 63，混有韩文版（如 `PCSH-00217 Farming Simulator 16` 配 `VLKS-66031`，`VLKS` = Korea 3rd party）。

#### 2.1.5 ⭐ `work.bin` / RIF —— 恒定 512 字节的最强指纹

**位置**：`<TITLEID>/sce_sys/package/work.bin`
【实测】No-Intro `Unofficial - Sony - PlayStation Vita (NoNpDrm)` DAT 的 452 个 game 中，`work.bin` 出现 **452 次**（100%），且 **size 恒为 512**。

结构（NoNpDrm `main.c` 逐字，与 Vita3K `packages/include/packages/license.h` 完全一致）：
```c
typedef struct {
  uint16_t version;                 // 0x00
  uint16_t version_flag;            // 0x02
  uint16_t type;                    // 0x04
  uint16_t flags;                   // 0x06
  uint64_t aid;                     // 0x08
  char     content_id[0x30];        // 0x10
  uint8_t  key_table[0x10];         // 0x40
  uint8_t  key[0x10];               // 0x50
  uint64_t start_time;              // 0x60
  uint64_t expiration_time;         // 0x68
  uint8_t  ecdsa_signature[0x28];   // 0x70
  uint64_t flags2;                  // 0x98
  uint8_t  key2[0x10];              // 0xA0
  uint8_t  unk_B0[0x10];            // 0xB0
  uint8_t  openpsid[0x10];          // 0xC0
  uint8_t  unk_D0[0x10];            // 0xD0
  uint8_t  cmd56_handshake[0x14];   // 0xE0
  uint32_t unk_F4;                  // 0xF4
  uint32_t unk_F8;                  // 0xF8
  uint32_t sku_flag;                // 0xFC
  uint8_t  rsa_signature[0x100];    // 0x100
} SceNpDrmLicense;
#define FAKE_AID 0x0123456789ABCDEFLL
```
Vita3K 有 `static_assert(sizeof(SceNpDrmLicense) == 0x200);`。

| 字段 | 偏移 | 长度 | 识别价值 | 可靠性 |
|---|---|---|---|---|
| 文件总长 | — | **恒 `0x200` = 512** | 强类型判定 | **极高** |
| **`aid`** | `0x08` | 8 | `EF CD AB 89 67 45 23 01`（LE 的 `0x0123456789ABCDEF`）= **NoNpDrm 假授权标志** | **极高** |
| **`content_id`** | `0x10` | `0x30` | ASCII NUL 结尾，**直接给出 TITLE_ID 与区域**，无需读 SFO、无需解密 | **极高** |
| `sku_flag` | `0xFC` | 4（BE） | `0` 或 `3`（3 = 完整版而非试玩） | 高 |

NoNpDrm readme 逐字（**隐私要点**）：
> "on games obtained through the PlayStation Store, work.bin is tied to your Sony Interactive Entertainment (also known as PlayStation Network) account and contains your account ID. The fake license does however **NOT** contain any personal information."

假授权的固定文件名（**对所有游戏都一样**，由 `ksceNpDrmGetFixedRifName(rif_name, FAKE_AID)` 派生）：
`ux0:nonpdrm/license/app/TITLE_ID/6488b73b912a753a492e2714e9b38bc7.rif`

#### 2.1.6 ⭐⭐ 实测：zRIF → work.bin → No-Intro DAT，端到端跑通

```
NoPayStation TSV 的 zRIF 字符串
  → base64 解码 → zlib inflate (wbits=10, 预置字典)
  → 恰好 512 字节的 work.bin
  → SHA-1 精确命中 No-Intro 的 Vita (PSN) DAT
```

**单例复现**（`10 Second Ninja X`，`PCSE00890`）：

| | 从 zRIF 重建 | DAT 里写的 |
|---|---|---|
| size | 512 | 512 |
| CRC32 | `8DEDF8F6` | `8DEDF8F6` ✅ |
| MD5 | `2397B3CCEA66B4A724E5C5D1181B8EA5` | 同 ✅ |
| SHA-1 | `14482D8313C3613AC84F97FDF66CD3EDABE772D3` | 同 ✅ |

**全量统计**（NPS `PSV_GAMES.tsv` 全部 1141 条 zRIF vs libretro 的 Vita (PSN) DAT 的 15694 条 `work.bin`）：
```
decoded sizes: {512: 1141}                    ← 100% 恰好 512 字节，0 解码失败
in DAT = 1105   not-in-DAT = 36               ← 命中率 96.8%
AID @0x08 分布: [('efcdab8967452301', 1141)]  ← 100% 是 NoNpDrm FAKE_AID
content_id@0x10 == TSV 'Content ID': 1141 (100.0%)
sku_flag@0xFC (BE): [(0, 1020), (3, 121)]     ← 与 NoNpDrm 源码 bswap32(0)/bswap32(3) 吻合
```

**工程含义**：
1. 拿到一个 NoNpDrm 转储目录，**只需哈希 512 字节的 `work.bin`**（成本几乎为零）就能查 No-Intro DAT 得到规范游戏名 + 区域。
2. `work.bin` 的 `content_id@0x10` 直接就是权威 CONTENT_ID。
3. 可离线自建 `SHA1(work.bin) → (游戏名, 区域, CONTENT_ID, TITLE_ID)` 映射表（约 15 700 条），也可从 NPS 的 zRIF 现场重建校验值。

#### 2.1.7 PKG 格式（全部大端）

psdevwiki PS3 `PKG_files`（PSV PKG 与 PS3 同结构）：

| 字段 | 偏移 | 长度 | 说明 | 可靠性 |
|---|---|---|---|---|
| `magic` | `0x00` | 4 | `7F 50 4B 47` = `"\x7FPKG"` | **极高** |
| `pkg_revision` | `0x04` | 2 | `80 00`=finalized(retail)，`00 00`=debug | 高 |
| **`pkg_type`** | `0x06` | 2 | `00 01`=PS3，**`00 02`=PSP 和 PS Vita** | **极高** |
| `pkg_metadata_offset` | `0x08` | 4 | PS3 常为 `0xC0`，**PSP/PSV 常为 `0x280`** | 高 |
| `pkg_metadata_count` | `0x0C` | 4 | metadata 条目数 | 结构必需 |
| `pkg_metadata_size` | `0x10` | 4 | PSV 常为 `0x160` | 高 |
| `item_count` | `0x14` | 4 | 加密数据内的文件/目录数 | 中 |
| `total_size` | `0x18` | 8 | pkg 总大小 | 中 |
| `data_offset` | `0x20` | 8 | 加密数据偏移 | 结构必需 |
| `data_size` | `0x28` | 8 | 加密数据大小 | 结构必需 |
| **`contentid`** | `0x30` | `0x24`（字段区 `0x30`） | **PKG Content ID** | **极高** |
| `digest` | `0x60` | `0x10` | — | 中 |
| `pkg_data_riv` | `0x70` | `0x10` | AES-128-CTR IV | 解密必需 |
| `pkg_header_digest` | `0x80` | `0x40` | — | 中 |
| **ext header magic** | `0xC0` | 4 | `7F 65 78 74` = `".ext"`，**仅 PSP/PSV 有** | **极高** |
| `pkg_key_id` | `0xE7` 低 3 位 | — | PSP=1，**PS Vita=`0xC0000002`**（`&7` 后为 2），PSM=`0xC0000004` | **极高** |

pkg2zip 实现逐字（社区事实标准）：
```c
if (get32be(pkg_header) != 0x7f504b47 || get32be(pkg_header + PKG_HEADER_SIZE) != 0x7F657874)
    sys_error("ERROR: not a pkg file\n");
uint64_t meta_offset = get32be(pkg_header + 8);
uint32_t meta_count  = get32be(pkg_header + 12);
uint32_t item_count  = get32be(pkg_header + 20);
uint64_t total_size  = get64be(pkg_header + 24);
uint64_t enc_offset  = get64be(pkg_header + 32);
uint64_t enc_size    = get64be(pkg_header + 40);
const uint8_t* iv = pkg_header + 0x70;
int key_type = pkg_header[0xe7] & 7;
```
（`PKG_HEADER_SIZE = 192 = 0xC0`）

⭐ **metadata type 14 = 明文 SFO —— 识别器的黄金入口**。psdevwiki 逐字：
```c
typedef struct { // size is 0x38   ← identifier 0xE, "PARAM.SFO Info"
 u32 param_offset;
 u16 param_size;
 u32 unk_int;
 u32 PSP2_SYSTEM_VER; // BCD encoded
 u8  unk[0x8];
 u8  param_digest[0x20]; // SHA256 of param_data. Called ParamDigest
} sfo_info;
```
pkg2zip：`else if (type == 14) { sfo_offset = get32be(block + 8); sfo_size = get32be(block + 12); }`

> **【事实】PKG 里的 `param.sfo` 是明文的** —— Vita3K 直接 `fseek(infile, sfo_offset)` + `fread` 后 `sfo::load`，**无需任何 AES 解密**即可拿到 TITLE_ID / TITLE / CATEGORY / CONTENT_ID。

`content_type`（metadata type 2）→ 平台：`0x15`=VITA_APP、`0x16`=VITA_DLC、`0x18`/`0x1d`=VITA_PSM、`0x1F`=VITA_THEME；`CATEGORY == "gp"` 时 APP 改判为 PATCH。

#### 2.1.8 其他容器格式

**`.psv`（psvgamesd / yifanlu 归档格式）—— Vita 界的 `.nds`/`.cia`**
来源：[`motoharu-gosuto/psvgamesd` — `driver/psv_types.h`](https://github.com/motoharu-gosuto/psvgamesd/blob/master/driver/psv_types.h)
设计动机原文：*"One unified .psv format for archiving (preserving) Vita games. ... We want something akin to .nds or .3ds/.cia or .iso but for Vita games."*

```c
typedef struct psv_file_header_v1 {
  uint32_t magic;               // 'PSV\0'
  uint32_t version;             // 0x00 = first version
  uint32_t flags;
  uint8_t  key1[0x10];
  uint8_t  key2[0x10];
  uint8_t  signature[0x14];     // same as in RIF
  uint8_t  hash[0x20];          // sha256 over complete data (cart) or over the pkg (digital)
  uint64_t image_size;
  uint64_t image_offset_sector; // in multiple of 512 bytes; 0 == no image included
  opt_header_t headers[];
} psv_file_header_v1;
#define PSV_MAGIC (0x00565350)   // 'PSV\0'
#define FLAG_TRIMMED     (1 << 0)
#define FLAG_DIGITAL     (1 << 1)
#define FLAG_COMPRESSED  (1 << 2)
```

| 字段 | 偏移 | 长度 | 可靠性 |
|---|---|---|---|
| `magic` | `0x00` | 4 | **极高** |
| `version` | `0x04` | 4 | 极高 |
| `flags` | `0x08` | 4 | bit0 TRIMMED / bit1 DIGITAL / bit2 COMPRESSED —— 极高 |
| `key1` / `key2` | `0x0C` / `0x1C` | `0x10` ×2 | 极高 |
| `signature` | `0x2C` | `0x14` | 极高 |
| **`hash`** | `0x40` | `0x20` | **SHA-256 全量校验和 ← 天然的变体主键**，极高 |
| `image_size` | `0x60` | 8 | 极高 |
| `image_offset_sector` | `0x68` | 8 | 极高 |

> ⚠ **`.psv` 扩展名被至少三种东西用过**：（a）上述 psvgamesd 格式；（b）Cobra Blackfin 的另一套 `.psv`（psvgamesd README 逐字声明：*"the format that we have develped with Yifan and devnoname120 is a completely independent format and has nothing to do with Cobra Blackfin"*）；（c）某些工具直接把裸 SceMBR 卡带镜像叫 `.psv`。**必须靠魔数区分，不能靠扩展名。**

**`.vci` 与 SceMBR（卡带镜像）**
Vita3K `vci.h` 逐字：
```cpp
constexpr uint32_t VCI_HEADER_SIZE = 0x200;
static const char VCI_MAGIC[4] = { 'V', 'C', 'I', '\0' };
static const char SCE_MBR_MAGIC[] = "Sony Computer Entertainment Inc.";
struct SceMbr {
    char     magic[0x20];          // "Sony Computer Entertainment Inc."
    uint32_t version;              // 0x3
    uint32_t device_size;          // in blocks
    uint8_t  unk1[0x28];
    ScePartition partitions[0x10]; // 0x50 .. 0x160
    uint8_t  reserved[0x9E];
    uint16_t boot_signature;       // 0xAA55
};
enum ScePartitionCode : uint8_t { ScePartitionCode_GRO0 = 0x9, ScePartitionCode_GRW0 = 0xA };
enum ScePartitionType : uint8_t { ScePartitionType_FAT16 = 0x6, ScePartitionType_EXFAT = 0x7, ScePartitionType_RAW = 0xDA };
```

| 字段 | 偏移 | 长度 | 可靠性 |
|---|---|---|---|
| VCI `magic` | `0x00` | 4 | `"VCI\0"` —— 极高 |
| VCI 头总长 | — | `0x200` | 其后紧跟裸 SceMBR 镜像 |
| SceMBR `magic` | 裸镜像 `0x00` / VCI 内 `0x200` | `0x20` | `"Sony Computer Entertainment Inc."` —— 极高 |
| SceMBR 分区表 | +`0x50` .. +`0x160` | 16 × `0x11` | `code 0x9`=gro0（游戏只读区）、`0xA`=grw0；`type 0x7`=exFAT —— 极高 |
| `boot_signature` | +`0x1FE` | 2 | `0xAA55` —— 极高 |

henkaku wiki Game Card 页交叉验证：`0x9 | exfat | gro0 | Game Card`。**要从卡带镜像里读出 `param.sfo`，必须实现 exFAT 只读。**

**`.psvimg` / `.psvmd`（CMA/QCMA 备份）**
[`yifanlu/psvimgtools` — `psvimg.h`](https://github.com/yifanlu/psvimgtools/blob/master/psvimg.h) 逐字：
```c
#define PSVMD_CONTENT_MAGIC (0xFEE1900D)
#define PSVMD_BACKUP_MAGIC  (0xFEE1900E)
#define PSVIMG_ENDOFHEADER "EndOfHeader\n"
#define PSVIMG_ENTRY_ALIGN (0x400)
#define PSVIMG_BLOCK_SIZE  (0x8000)
```

| 字段 | 偏移 | 长度 | 可靠性 |
|---|---|---|---|
| `.psvmd` `magic` | `0x00` | 4 | `0xFEE1900D`(content) / `0xFEE1900E`(backup) —— 极高 |
| `.psvmd` `psid` | `0x10` | 16 | 主机唯一 PSID —— 高 |
| `.psvmd` `name` | `0x20` | 64 | 备份集名 —— 高 |
| `.psvmd` `iv` | `0x58` | 16 | AES IV —— 高 |
| **`.psvimg` 全体** | — | — | **AES-256-CBC 加密，无密钥不可解析** —— **不可用** |

> ⚠ **纠正一个常见说法：`.psvimg` 是 AES-256-CBC，不是 AES-128。** `psvimgtools/crypto.h` 逐字：`gcry_cipher_open(&ctx, GCRY_CIPHER_AES256, GCRY_CIPHER_MODE_CBC, 0); gcry_cipher_setkey(ctx, key, 32);`
> 密钥派生（henkaku wiki PSVIMG 页逐字）：`static uint8_t passphrase[CMA_PASSPHRASE_LEN] = "Sri Jayewardenepura Kotte";` → `sha256(aid[8] || passphrase[25])` → `aes_ecb_decrypt` → 32 字节 AES-256-CBC 密钥。**密钥绑定 PSN AID，逐机不同。**

CMA 目录结构（psvimgtools README + qcma `cmarootobject.cpp`）：
`PS Vita/<APP|PGAME|PSAVEDATA|PSGAME|PSM|SYSTEM>/<16位hex AID>/<TITLE_ID>/game/game.psvimg`

**PFS 加密状态（只需读 8 字节魔数，无需解密）**
[`motoharu-gosuto/psvpfstools`](https://github.com/motoharu-gosuto/psvpfstools) 逐字：
```c
#define MAGIC_WORD       "SCENGPFS"    // sce_pfs/files.db
#define DB_MAGIC_WORD    "SCEIRODB"    // sce_pfs/unicv.db
#define FT_MAGIC_WORD    "SCEIFTBL"
#define CV_DB_MAGIC_WORD "SCEICVDB"    // icv.db (存档/奖杯)
#define NULL_MAGIC_WORD  "SCEINULL"
```

| 判定 | 依据 | 可靠性 |
|---|---|---|
| **未解密**（PFS 保留） | `sce_pfs/files.db` 前 8 字节 `SCENGPFS` **且** `sce_pfs/unicv.db` 前 8 字节 `SCEIRODB` | **极高** |
| **已解密**（Vitamin / MaiDumpTool） | 无 `sce_pfs/` 或缺 db | 高（**【推断】**） |
| **Vitamin 转储** | 存在 `sce_module/steroid.suprx` | **极高** |

Vita3K 逐字：
```cpp
if (m_filename.contains("sce_module/steroid.suprx")) {
    LOG_CRITICAL("A Vitamin dump was detected, aborting installation...");
```

> ⚠ **`sce_sys/package/head.bin` 的内部布局：【未找到权威来源】。** 该文件在 psdevwiki "Files on the PS Vita" 与 henkaku "Applications" 的目录清单中确有记载，但无任何 wiki 页或开源实现解析其结构。社区教程称可用十六进制编辑器从中读出 Content ID，**未能取得可引用的一手来源**。【推断】它是安装时保留的 PKG 头部（含 `contentid@0x30`），但**未经证实，不要据此实现**。
> 同样 **【未找到权威来源】**：`stat.bin` / `tail.bin` / `body.bin` / `cert.bin` / `clearsign` / `keystone` 的布局（`temp.bin` 例外，psdevwiki 逐字：*"Pre-embedded .rif file. Comes with DRM free app."*）；以及 **MaiDumpTool 的产物格式规范**（原作者仓库 `BeniYukiMai/MaiDumpTool` **闭源**，只有一行 README）。

#### 2.1.9 磁盘上的"变体成型"形态 —— 8 种并存

**PSV 是本文所有平台里成型最复杂的。** 5 种是目录，3 种是单文件。

| # | 形态 | 类型 | 判定 |
|---|---|---|---|
| **A** | **已安装的应用目录** | 目录 | 含 `sce_sys/param.sfo` |
| **B** | **NoNpDrm 转储** | 目录 + sidecar | A + `sce_sys/package/work.bin`（512 B，`aid == FAKE_AID`） |
| **C** | **`.vpk`** | 单文件（ZIP） | ZIP 内含 `*/sce_sys/param.sfo` |
| **D** | **MaiDumpTool 转储** | 目录 | 无 `sce_pfs/`；含 `mai_moe/eboot_origin.bin` |
| **E** | **FAGDec 输出** | 目录 | `ux0:/FAGDec` 下的 `.elf`/`.self`/`.seg<N>`/`.sha256`/`.ppk` —— **不是完整游戏，别当变体** |
| **F** | **`.pkg`** | 单文件 + 需 zRIF/work.bin | `\x7FPKG` @0 且 `.ext` @0xC0 且 `pkg_type == 2` |
| **G** | **卡带整体镜像** | 单文件 | `.psv` / `.vci` / 裸 SceMBR，靠魔数区分 |
| **H** | **CMA 备份** | 主文件 + 附属 | `.psvimg` + `.psvmd`（加密，只能靠路径推 TITLE_ID） |

**形态 A 的规范目录树**（henkaku wiki `Applications` 页逐字）：
```
 d:/PCSG00001
   d:/sce_module        f:/libc.suprx
   d:/sce_pfs           f:/files.db   f:/pflist   f:/unicv.db
   d:/sce_sys
     d:/abort           f:/right.suprx
     d:/livearea/contents   f:/bg0.png  f:/default_gate.png  f:/template.xml
     d:/package         f:/cert.bin  f:/head.bin  f:/stat.bin  f:/tail.bin  f:/temp.bin
     f:/clearsign  f:/icon0.png  f:/keystone  f:/param.sfo  f:/pic0.png
   f:/eboot.bin
```

⭐ **成型规则（Vita3K 的实现，可直接照抄）**：
```cpp
const auto is_content = (filename == "param.sfo") || (filename == "theme.xml");
if (is_content) {
    auto parent_path = p.path().parent_path();
    const auto content_path = (filename == "param.sfo") ? parent_path.parent_path() : parent_path;
```
即：**找到 `param.sfo` → 上跳两级 = 变体根**。

> ⚠ **目录名 ≠ TITLE_ID。** Vita3K 明确区分 `app.path = title_id;`（磁盘目录名）vs `app.title_id = info.app_title_id;`（`param.sfo` 里的值）。`param.sfo` 缺失时全部退化为目录名，`app_ver`/`category` 记 `"N/A"`。**识别器必须以 SFO 为准，把目录名当线索而非事实。**

**按 `CATEGORY` 分流**（Vita3K `set_content_path()`）：
- `gd`/其它 → `ux0/app/<TITLE_ID>`（本体）
- `gp` → `ux0/patch/<TITLE_ID>`（**补丁，必须先装本体**：`if (!fs::exists(app_path) || fs::is_empty(app_path)) { LOG_ERROR("Install app before patch"); return false; }`）
- `ac` → `ux0/addcont/<TITLE_ID>/<CONTENT_ID.substr(20)>`（DLC）
- 含 `theme.xml` → `ux0/theme/<CONTENT_ID>`（主题）

**形态 B（NoNpDrm）的完整目录树**（NoNpDrm readme 逐字）：
```
├───addcont/TITLE_ID/DLC_FOLDER
├───app/TITLE_ID/sce_sys/package/work.bin
├───license/addcont/TITLE_ID/DLC_FOLDER/6488b73b912a753a492e2714e9b38bc7.rif
├───patch/TITLE_ID
```
NoNpDrm readme 逐字：*"This software WILL NOT ... Work with PFS decrypted content (such as games dumped using applications such as Vitamin or MaiDumpTool)."* —— 即 **NoNpDrm 转储的 `sce_pfs/` 仍在，内容未解密**。

**形态 C（`.vpk`）就是一个 ZIP**。vitasdk `vita-pack-vpk.c` 逐字：
```c
#include <zip.h>
zip = zip_open(output, ZIP_CREATE | ZIP_TRUNCATE, &err);
if (!add_file_zip(zip, sfo, "sce_sys/param.sfo")) { ... }
if (!add_file_zip(zip, eboot, "eboot.bin")) { ... }
```
> ⚠ **一个 VPK 里可以有多个内容根**（前缀不同的多个 `sce_sys/param.sfo`），要逐个成型。Vita3K 用 miniz 打开并 `push_if_not_exists(content_path, m_filename)`。
> ⚠ **VPK 不是"透明容器"**。它的字节哈希有意义（No-Intro 就哈希整个 `.vpk`），但**同一游戏重新打包会得到不同哈希**。识别锚点仍是内部的 `param.sfo`。
> psvgamesd 作者对 VPK 作归档格式的批评（逐字）：*"What's wrong with using .vpk? VPK is designed for homebrew. The patches to enable homebrew strips out a lot of the game executable metadata ... This leads to many subtle as well as major bugs (saves not working, ...)"*

**形态 D（MaiDumpTool）**，只能从 changelog 逐字提取约定：
> "Support for patch installation from folder (ux0:mai/TitleID_patch)"
> "Support for DLC installation from folder (ux0:mai/TitleID_addc)"
> "Trying to keep original files of extracted games, especially **mai_moe/eboot_origin.bin**"

最强指纹是 **`mai_moe/eboot_origin.bin`**。⚠ GitHub 全站代码搜索 `mai_moe` 与 `eboot_origin.bin` **零命中** —— 没有任何开源实现处理过它，需靠真实样本确认。可靠性 **中**。
Renascene 把转储状态明确分三类（逐字）：`dumped (NoNPDRM)` / `dumped (Vitamin/Mai)` / `undumped`。

**⭐ 可直接实现的成型判定顺序**：
```
1. 单文件，前 4 字节 == 7F 50 4B 47 且 +0xC0 == 7F 65 78 74      → .pkg
2. 单文件，前 4 字节 == 'PSV\0'                                   → psvgamesd .psv
3. 单文件，前 4 字节 == "VCI\0"                                   → .vci 卡带镜像
4. 单文件，前 0x20 == "Sony Computer Entertainment Inc." 且 +0x1FE == 55 AA → 裸 SceMBR 卡带镜像
5. 单文件，前 4 字节 == FEE1900D / FEE1900E (LE)                  → .psvmd（配套 .psvimg）
6. 单文件，是 ZIP 且内含 */sce_sys/param.sfo                      → .vpk（可能多个内容根）
7. 目录，含 sce_sys/param.sfo                                     → 已安装/已转储的应用目录
     ├─ 含 sce_sys/package/work.bin (512B, aid==FAKE_AID) → NoNpDrm 变体
     ├─ 含 sce_pfs/files.db(SCENGPFS)+unicv.db(SCEIRODB)  → PFS 未解密
     ├─ 含 sce_module/steroid.suprx                        → Vitamin 转储（Vita3K 拒绝加载）
     └─ 含 mai_moe/eboot_origin.bin                        → MaiDumpTool 转储（PFS 已解密）
8. 目录，含 theme.xml                                             → 主题
9. 文件恰 512 字节且 aid@0x08 == EF CD AB 89 67 45 23 01          → 孤立的 work.bin/.rif 授权
```

⚠ **PSN CDN 下载的 `.pkg` 文件名是随机混淆串**（如 `dDyVSMdZRoYY…JYRaiFTEhFfcfRSHBUnflIZCq.pkg`），也有带结构的（`UP4108-PCSE00743_00-DLC0000000000002_bg_1_<sha1>.pkg`）。**绝不能靠文件名识别。**

#### 2.1.10 DAT / 数据库覆盖

| 数据库 | 覆盖？ | 程度 | 哈希对象 | 哈希算法 |
|---|---|---|---|---|
| **Redump** | ❌ **完全没有** | 0 | — | — |
| **No-Intro 官方** | ✅ | PSN Content **17505** + Updates **3976** | 加密 `.pkg` / 512 B `work.bin` / `.rap` / `PSP2UPDAT.PUP` | CRC32+MD5+SHA1 100%；**SHA256 仅 66.6%**（`.pkg` 仅 39.8%） |
| **No-Intro Unofficial** | ✅ | VPK 240 / NoNpDrm 452 / PSVgameSD 383 / BlackFinPSV 67 | 整卡镜像 / 整包 / **逐资源文件** | CRC32+MD5+SHA1，部分 SHA256 |
| **TOSEC** | ⚠ 名义上有 | **41 条，全是 Demos + Homebrew** | `.vpk` / `.velf` | 商业游戏 **= 0** |
| **NoPayStation** | ✅ | GAMES 1141 / DEMOS 395 / DLCS 414 / THEMES 1276 / UPDATES | 加密 `.pkg` 整包 | **仅 SHA256** |
| **libretro-database** | ✅（二手） | 2 个 DAT（No-Intro 副本） | `.vpk` / `work.bin` / `.pkg` | CRC32+MD5+SHA1 |
| **Renascene** | ✅（纯元数据） | card + psn 双目录，130 页 | **无任何哈希** | — |
| **RetroAchievements** | ❌ | `rc_consoles.h` 86 个常量中 **无 Vita** | — | — |
| **VitaDB** | ❌ | 只收 homebrew，且 `vitadb.rinnegatamante.it` 已成域名停放页 | — | — |
| **PSNDL / PSDLE** | — | psndl.net Cloudflare 403 + IA 503 → **【未找到权威来源】**；psdle 是个人账号脚本，**2025-03-21 已停更** | — | — |

**No-Intro 的 8 个 Vita DAT 全表**：

| DAT 名称 | 内部 id | 条目 | 被哈希的实体 |
|---|---|---|---|
| `Sony - PlayStation Vita (PSN) (Content)` | 92 | **17505** | `.pkg`（加密原包）+ `work.bin` + `.rap` |
| `Sony - PlayStation Vita (PSN) (Updates)` | 143 | **3976** | `.pkg` ×3853、`PSP2UPDAT.PUP` ×122 |
| `Unofficial - … (VPK)` | 84 | 240 | 整个 `.vpk` |
| `Unofficial - … (NoNpDrm)` | 84 | 452 | **逐文件**（`.png` ×229214、`.at9` ×43479…，共 431 605 个 rom） |
| `Unofficial - … (PSVgameSD)` | 84 | 383 | `.psv` ×382 + `.rif` ×86 |
| `Unofficial - … (BlackFinPSV)` | 84 | 67 | `.psv` ×67 |
| `Unofficial - … (PSN) (Decrypted) (VPK)` | 85 | 50 | `.vpk` |
| `Unofficial - … (PSN) (Decrypted) (NoNpDrm)` | 85 | 1 | 逐文件 |

DAT 样例（VPK 集）—— ⚠ **`serial` 带连字符 `PCSG-00159`，而 `param.sfo` 里是 `PCSG00159`**：
```xml
<game name="Akiba Strip 2 (Korea)" id="0078">
    <game_id>PCSG00159</game_id>
    <rom name="Akiba Strip 2 (Korea).vpk" size="1429914890" crc="ac365da0"
         md5="9b84459f746b41444a180da26d71d421"
         sha1="d345af0e3973ff29738d1c3092eba3b753c9c662" serial="PCSG-00159"/>
</game>
```
卡带镜像集样例 —— 注意 `mia="yes"` = 已登记但实物未保存：
```xml
<game name="Little Big Planet (USA) (Demo) (Kiosk)" id="x001">
    <game_id>PCSA00018</game_id>
    <rom name="… .psv" size="1895825408" crc="bd6e7699" … mia="yes" serial="PCSA-00018"/>
    <rom name="… .rif" size="512" … mia="yes"/>
</game>
```

**Redump 明确不存在 PSV**：
- 【实测 2026-08-31】`https://redump.info/downloads/` 的 **106 个系统短码**中，Sony 只有 `PSX / PS2 / PS3 / PS4 / PS5 / PSP`，**无 `PSV` / `VITA`**
- 旧镜像 `http://redump.org/discs/system/psv/` 返回页面逐字：`<div class="error">System "psv" doesn't exist.`
- **【推断】** Redump 只收光盘介质，Vita 卡带是 MMC 闪存卡（henkaku Game Card 页逐字：*"Game card is a standard MMC card."*），不在其范围内。Redump 官方未明文声明这一点。
- ⚠ 注意与 [1.2](#12-redump必须走-redumpinfo旧镜像少-46-个系统实测) 的区别：**Wii U 的"Redump 无覆盖"是旧镜像造成的假象，PS Vita 的则是真的。**

**NoPayStation TSV 列头**（逐字）：
```
PSV_GAMES.tsv   (12 列)  Title ID⇥Region⇥Name⇥PKG direct link⇥zRIF⇥Content ID⇥Last Modification Date⇥Original Name⇥File Size⇥SHA256⇥Required FW⇥App Version
PSV_DLCS.tsv    (9 列)   …⇥File Size⇥SHA256
PSV_UPDATES.tsv (9 列)   Title ID⇥Region⇥Name⇥Update Version⇥Required FW VERSION⇥PKG direct link⇥Last Modification Date⇥File Size⇥SHA256
```
**只有 SHA256，没有 CRC32/MD5/SHA-1** —— 与 No-Intro 的哈希算法几乎不重叠，交叉校验必须自行补算。
Region 分布实测：`JP 1250 / US 1046 / EU 1020 / ASIA 465 / INT 7 / UNKNOWN 1`。

> ⚠ **libretro-database 的陷阱**：`metadat/no-intro/Sony - PlayStation Vita.dat` 虽在 `no-intro/` 目录下，**署名却不是 No-Intro**，头部逐字：`homepage "http://github.com/robloach/libretro-dats"`，version `2021.10.24`。它是 No-Intro **Unofficial VPK** 集的 2021 年转格式副本（240 条）。**不要当官方 Vita DAT 用。**
> `metadat/no-intro/Sony - PlayStation Vita (PSN).dat`（version `2026.08.01`，17100 条）则是 `work.bin` 集，扩展名分布 `bin 15695 / pkg 1304 / vpk 49 / rap 43`。
> RetroArch 用 **serial 而非 CRC** 作 Vita 主键：`libretro-build-database.sh` 逐字 `build_libretro_database "Sony - PlayStation Vita" "rom.serial"`。

**headered vs headerless**：Vita **不存在**这个概念。`auto-datfile-generator` README 逐字只列了四个需要 header XML 的系统：`Atari Jaguar / Atari Lynx / Nintendo FDS / Nintendo NES`。

#### 2.1.11 中文汉化覆盖

**⭐ 必须区分「官中」与「民间汉化」—— 前者覆盖良好，后者完全空白。**

**（一）官方中文版：No-Intro 覆盖良好**【实测】

在 No-Intro Vita (PSN) DAT（17100 条）里统计：
- 带 `Zh` 语言标签的游戏：**290 条**
- Region 分布：`Japan 5807 / USA 5059 / Europe 4240 / Asia 1859 / Korea 85 / **China 28** / Taiwan 1`
- 标签形态样本：
  ```
  36 Fragments of Midnight (Asia) (En,Zh,Ko)
  7'scarlet (Asia) (Zh,Ko)
  Access Denied (Asia) (En,Ja,Fr,De,Es,Zh-Hant,Zh-Hans,Ko,Ru)
  Akiba's Trip 2 (Asia) (Zh,Ko)
  ```
  即区分 `Zh` / `Zh-Hant` / `Zh-Hans`，Region 另有独立的 `China` / `Taiwan`。

**结论：Sony 官方发行的中文版（含国行、港版繁中、简中），No-Intro 覆盖良好且有明确语言标记，可直接消费。**

**（二）民间汉化：确认不存在任何结构化、可下载的数据库**

| 来源 | 结果 |
|---|---|
| TOSEC | Vita 共 41 条 DAT 条目，全是 Demos + Homebrew，`[tr` 出现 **0** 次 → **`[tr zh]` = 0** |
| No-Intro / libretro Vita DAT | 搜索 `Chinese` / `[tr` → **0 命中** |
| NoPayStation | **无 language 字段**，只有 Region。全表提到中文的只有 3 行（`PCSH00131 One Tap Hero (Chinese Edition)`、`PCSH00194 Mr. Pumpkin's Adventure (Chinese)`、`PCSH10060 空之軌跡 SC Evolution (中文版)`），属偶然写进标题，非系统性标记 |
| GitHub | 多组查询（`psv 汉化`、`vita 汉化`、`psvita chinese translation`…）**全部命中都是单个游戏的汉化工程/补丁源码，无任何聚合列表 / JSON / CSV / DAT** |
| Renascene | 有 `ZRIF` / `Dump status` / `Media ID` / `BOX ID`，但**无任何哈希，也无语言字段** |

**唯一成规模的中文数据源是中文资源站的 HTML 长列表（无 CSV/JSON/DAT 导出，需自写爬虫）**：

| 站点 | 条目数 | 字段 |
|---|---|---|
| `laojiku.com/44.html` | 486 | 中文名、TitleID、区域(国行/港版/日版/美版/欧版)、语言(简中/繁体)、版本、DLC 数、格式(NONPDRM/Mai233)、固件；**明确区分「官中」与「汉化」** |
| `www.2023game.com/psv/psvcn/32549.html` | 447 | 编号 PSVCH001–PSVCH447、中文名、TitleID、区域、语言、版本、DLC、格式(Mai/NONPDRM/Vitamin)、固件、**汉化组署名** |
| `www.gamer520.com/13107.html` | 486 | 同类结构 |

> ⚠ 这些站点把「官中」和「汉化」混在同一列表里，需人工拆分。
> ⭐ **【推断】强启发式**：汉化版通常是 MaiDumpTool 形态（PFS 解密后才能改文件），所以**「发现 `mai_moe/eboot_origin.bin` 且 TITLE_ID 是日文区（`PCSG`/`PCSC`）」= 疑似汉化版**，应直接进人工待确认队列。
> **【推断】** `param.sfo` 的 `TITLE_ID` + `CONTENT_ID` 通常不会被汉化改动，所以民间汉化版仍能被正确归到"基于哪个发行版"，只是汉化组/版本必须人工裁决后钉进自建库。

#### 2.1.12 实现成本评估

**Rust 生态现状（诚实版）**

⚠ **crates.io 上不存在的（别浪费时间找）**：
- `param-sfo`、`psvpfs`、`psvpfstools`、`psvpfsparser`、`npdrm` → **0 结果**
- 所有叫 `psf` 的 crate 都是**点阵字体或 Cadence 仿真波形**（`psf` / `psf-rs` / `psf2` / `simple-psf` / `rusty-psf` / `psfparser`），与 PARAM.SFO 无关
- 所有叫 `vpk` 的 crate 都是 **Valve Pak 或 N64 vpk0**；`libvpk` 同理
- `vita` / `vita-io` / `vita-core` 是**分子化学信息学**库；`vita49` 是无线电协议

**真正可用的四个**：

| crate | 版本 | 下载 | 仓库 | 评价 |
|---|---|---|---|---|
| `pkg2zip` | 0.1.11 | 166 | <https://codeberg.org/NeverAnswers/pkg2zip-rs> | pkg2zip 的纯 Rust 移植，导出 `Sfo`/`PkgReader`/`decode_zrif`/`Region::from_id`，README 自称 `Bit-for-Bit Parity`。⚠ **2026-08-25 创建，5 天发 12 个版本，成熟度未经验证** |
| ⭐ `gumshoe` | 0.0.4-beta | 82 | ⚠ crates.io 登记的 `github.com/SnowflakePowered/gumshoe` **返回 404**，只能读 crate tarball | 通用主机 dump 识别器，**已内建完整 Vita 支持**（`headerck/sony/psv/{mod,pfs,zrif,livearea,decrypt}.rs` + `vfs/psv.rs` + `vfs/sony_pkg.rs`），**是最接近本项目需求的参考实现** |
| `orbis-unpkg` | 0.1.1 | 73 | <https://github.com/Aspenini/ConsoleTools-rs> | `src/psf.rs` 是通用 PSF 解析器（shadPS4 移植），可单独用 |
| `datary` | 0.3.0 | 2089 | <https://github.com/one-retro/datary> | **读写 No-Intro / Logiqx / Redump / TOSEC DAT**，DAT 环节直接用它 |

`gumshoe` 的 Vita 识别流程逐字（**可直接照抄的架构**）：
```rust
if package.len()? >= 4 && package.read_at::<4>(0)? == *b"\x7fPKG" {
    if package.len()? < 0xe8
        || u16::from_be_bytes(package.read_at::<2>(6)?) != 2
        || package.read_at::<4>(0xc0)? != *b"\x7fext"
        || !matches!(package.read_at::<1>(0xe7)?[0] & 7, 2..=4)
    { return Ok(None); }
}
...
let encrypted = vfs.exists(b"sce_pfs/files.db".as_bstr())? && vfs.exists(b"sce_pfs/unicv.db".as_bstr())?;
```

**参考开源实现**（全部已核实存在）

| 项目 | 语言 | 价值 |
|---|---|---|
| `vitasdk/vita-toolchain` → `src/vita-mksfoex/vita-mksfoex.c` | C | ★★★★★ SFO 二进制布局权威（写入端） |
| 同上 → `src/vita-pack-vpk/vita-pack-vpk.c` | C | ★★★★★ 证明 VPK = ZIP + 固定内部路径 |
| `Vita3K/Vita3K` → `vita3k/packages/{sfo,pkg,license,vci}.*`、`interface.cpp`、`app/src/apps_list.cpp` | C++ | ★★★★★ 读取端 + 成型规则 + 安装分流 |
| `mmozeiko/pkg2zip` | C | ★★★★★ PKG 偏移与区域表权威（已 archived，Unlicense） |
| 同上 → `zrif2rif.py` / `rif2zrif.py` | Python | ★★★★★ zRIF ↔ work.bin 转换（本次实测就用它） |
| `TheOfficialFloW/NoNpDrm` → `main.c` + `readme.md` | C | ★★★★☆ work.bin 结构与目录布局权威 |
| `motoharu-gosuto/psvgamesd` → `driver/psv_types.h` | C | ★★★★★ `.psv` 归档格式规范 |
| `motoharu-gosuto/psvpfstools` | C++ | ★★★★☆ PFS 魔数/结构（⚠ **`psvpfsparser` 在这里，不在 vita-toolchain**） |
| `yifanlu/psvimgtools` → `psvimg.h`、`crypto.h` | C | ★★★★☆ `.psvimg`/`.psvmd` 权威 |
| `oestriot/VCI-TOOLS` | — | ★★★★☆ `.vci`/`.psv`/`.img` 互转与语义 |
| `TheOfficialFloW/VitaShell` → `sfo.h` | C | ★★★★☆ SFO 结构第三份独立佐证 |
| `codestation/qcma` → `common/cmarootobject.cpp` | C++ | ★★★☆☆ CMA 备份目录结构 |
| `Vita3K/compatibility`（GitHub Issues） | — | ★★★☆☆ 4000+ 条 `游戏名 [TITLEID]` 映射，可作刮削辅助 |
| `BeniYukiMai/MaiDumpTool` | **闭源** | ★☆☆☆☆ 只有 changelog 可参考 |

**最小解析面**

必做（覆盖 ~95% 实际转储）：

| # | 任务 | 规模 | 说明 |
|---|---|---|---|
| 1 | PARAM.SFO 解析器 | ~80 行 | 头 `0x14` + 条目 `0x10`×N，LE。**必须校验 magic 与边界（Vita3K 都没做）** |
| 2 | ZIP 嗅探（`.vpk`） | `zip` crate | 找 `*/sce_sys/param.sfo`，注意一个 VPK 可能多个内容根 |
| 3 | 目录成型规则 | 纯路径判定 | 见 2.1.9 的 9 条规则，零二进制解析 |
| 4 | PFS 加密状态 | 读 8 字节 | `SCENGPFS` + `SCEIRODB`，**不需要解密** |
| 5 | work.bin / RIF 解析 | ~30 行 | size==512、`aid@0x08`、`content_id@0x10` |
| 6 | zRIF 解码 | ~20 行 | base64 + `flate2` 的 `Decompress::new_with_window_bits(false, 10)` + 预置字典 |
| 7 | PKG 头解析（仅识别） | ~120 行 | 全 BE；metadata type 14 → 明文 SFO，**无需 AES** |
| 8 | 哈希 | crate | CRC32 / MD5 / SHA-1 / SHA-256 |
| 9 | DAT 摄取 | `datary` 或自写 | No-Intro XML v3/v4 + clrmamepro 括号格式 + NPS TSV |

选做：`.psv`/`.vci`/SceMBR 头嗅探（~60 行，便宜）；**卡带镜像内 exFAT 只读（重，本平台最重的一块，gumshoe 用 `hadris-fat 2.0.0-rc.4` 这个 rc 版）**；`.psvimg` 识别（~20 行，只能报"CMA 备份，需 AID 密钥"）。

**明确不需要做**：PFS 解密（需 klicensee + F00D 在线服务）、SELF/ELF 解密（需 Vita 硬件）、PKG item 表解密（只识别不解包时不需要）、自己实现 VPK 格式（就是 ZIP）。

**人日估算**

| 模块 | 人日 |
|---|---|
| SFO 解析器 + 键映射 + 单测 | 0.5 |
| VPK(ZIP) + 目录形态检测 + 9 条成型规则 | 1.5 |
| work.bin/RIF 解析 + zRIF 解码 | 0.5 |
| PKG 头 + metadata + 明文 SFO 提取 | 1.0 |
| `.psv` / `.vci` / SceMBR / `.psvmd` 头嗅探 | 0.5 |
| DAT 摄取（No-Intro XML ×2 schema + clrmamepro + NPS TSV）与索引 | 1.5 |
| 置信度分层、候选与依据生成 | 1.5 |
| 真实样本联调与边界情况 | 1.5 |
| **核心小计（不含 exFAT）** | **≈ 8.5 人日** |
| 卡带镜像 exFAT 只读（可选，覆盖 No-Intro Unofficial 的 450 条 `.psv`） | +3 ~ 5 |
| **合计（含卡带镜像）** | **≈ 12 ~ 14 人日** |

> **成本全在"成型"和"多形态并存"上**，解析本身不难（SFO 极简单）。一个变体可能是目录、可能是 ZIP、可能是单文件镜像，还可能是"目录 + 512 字节 sidecar"。

**推荐识别流水线**
```
L0 容器归一化 : .zip/.7z 里若含单个 .vpk → 透传；.vpk 本身不当透明容器
L1 魔数嗅探   : PKG / PSV / VCI / SceMBR / PSVMD / ZIP   (读前 0x200 字节)
L2 目录成型   : 找 sce_sys/param.sfo → 上跳两级 = 变体根；收集 sidecar
L3 结构化识别 : param.sfo → TITLE_ID + CONTENT_ID + APP_VER + CATEGORY  ← 主键
                work.bin  → content_id（与 SFO 交叉校验）
L4 哈希比对   : work.bin(512B) SHA-1 → No-Intro Vita (PSN) DAT   ← 成本最低、命中最准
                整个 .vpk  CRC/MD5/SHA1 → No-Intro Unofficial (VPK) DAT
                .psv       CRC/MD5/SHA1 → No-Intro Unofficial (PSVgameSD/BlackFinPSV)
                .pkg       SHA-256      → NoPayStation TSV
L5 变体标注   : PFS 状态 / Vitamin / MaiDump 指纹 → 原版转储还是改动过
                日文区 TITLE_ID + MaiDump 指纹 → 「疑似汉化」进待确认队列
L6 自建库     : 人工裁决 → SHA1(work.bin) 或目录内容哈希 钉到 发行版 + 汉化组 + 版本
```

---

### 2.2 Wii U

> ⭐ **本节三条最重要的结论**：
> 1. **WUD 光盘镜像的前 22 字节是明文 ASCII**（形如 `WUP-P-ADRZ-04-512EUR-0`）—— **零密钥即可识别**。加密从 `0x18000` 才开始。
> 2. **Cemu 的 `TitleDataFormat` 枚举就是现成的"变体成型"分类法**，5 种形态，判定逻辑可直接照抄。
> 3. **Redump 有 541 条 Wii U 光盘 DAT，且 541 条尺寸全部恰为 `25 025 314 816` 字节**（此前一度误判为「Redump 无 Wii U」，根因见 [1.2](#12-redump必须走-redumpinfo旧镜像少-46-个系统实测)）。
>
> ⚠ 另有一条**必须避开的陷阱**：**`meta.xml` 里的 `title_id` 和 `title_version` 可能是错的**，权威顺序是 **TMD > ticket > app.xml > meta.xml**。

#### 2.2.1 ⭐ 磁盘上的"变体成型"形态 —— Cemu 的官方分类法

Cemu `src/Cafe/TitleList/TitleInfo.h` L136-155 逐字：
```cpp
enum class TitleDataFormat
{
    HOST_FS = 1,       // host filesystem directory (fullPath points to root with content/code/meta subfolders)
    WUD = 2,           // WUD or WUX
    WIIU_ARCHIVE = 3,  // Wii U compressed single-file archive (.wua)
    NUS = 4,           // NUS format. Directory with .app files, title.tik and title.tmd
    WUHB = 5,
    // error
    INVALID_STRUCTURE = 0,
};

enum class InvalidReason : uint8
{
    NONE = 0,
    BAD_PATH_OR_INACCESSIBLE = 1,
    UNKNOWN_FORMAT = 2,
    NO_DISC_KEY = 3,
    NO_TITLE_TIK = 4,
    MISSING_XML_FILES = 4,
};
```

⭐ **`TitleInfo::DetectFormat()` 的判定逻辑**（`TitleInfo.cpp` L180-221 逐字，**可直接照抄**）：
```cpp
bool TitleInfo::DetectFormat(const fs::path& path, fs::path& pathOut, TitleDataFormat& formatOut)
{
    std::error_code ec;
    if (path.has_extension() && fs::is_regular_file(path, ec))
    {
        std::string filenameStr = _pathToUtf8(path.filename());
        if (boost::iends_with(filenameStr, ".rpx"))
        {
            // is in code folder?
            fs::path parentPath = path.parent_path();
            if (boost::iequals(_pathToUtf8(parentPath.filename()), "code"))
            {
                parentPath = parentPath.parent_path();
                // next to content and meta?
                std::error_code ec;
                if (fs::exists(parentPath / "content", ec) && fs::exists(parentPath / "meta", ec))
                {
                    formatOut = TitleDataFormat::HOST_FS;
                    pathOut = parentPath;
                    return true;
                }
            }
        }
        else if (boost::iends_with(filenameStr, ".wud") ||
                 boost::iends_with(filenameStr, ".wux") ||
                 boost::iends_with(filenameStr, ".iso"))
        {
            formatOut = TitleDataFormat::WUD;
            pathOut = path;
            return true;
        }
        else if (boost::iequals(filenameStr, "title.tmd"))
        {
            formatOut = TitleDataFormat::NUS;
            pathOut = path;
            return true;
        }
        else if (boost::iends_with(filenameStr, ".wua"))
        {
            formatOut = TitleDataFormat::WIIU_ARCHIVE;
            ...
        }
    }
    ...
}
```

**七种形态汇总**（Cemu 5 种 + 分卷 WUD + WUMAD）：

| # | 形态 | 类型 | 判定 | 附属文件 |
|---|---|---|---|---|
| **A** | **HOST_FS**（loadiine / 解包目录） | 目录 | 含 `code/` + `content/` + `meta/` | — |
| **B** | **WUD**（光盘镜像） | 单文件 + sidecar | `.wud` / `.wux` / `.iso`（Redump 命名）；`WUP-`@0 + `0xCC549EB9`@0x10000 | ⚠ **访问分区内容需 16 字节 `game.key`**（Cemu 报 `NO_DISC_KEY`）；**但识别不需要** |
| **B'** | **分卷 WUD** | 12 个文件 | `game_part1.wud` … `game_part12.wud`（前 11 卷各 `0x80000000`）或 `game.wud.part1..12`（WUDD 命名） | 同上 |
| **C** | **NUS**（CDN / 已安装数字版） | 目录 | `title.tmd`+`title.tik`+`%08X.app`，或 CDN 原样 `tmd`/`tmd.N`+`cetk`+裸 8 位 hex | `.h3` 哈希文件 |
| **D** | **WIIU_ARCHIVE**（`.wua`） | 单文件 | ⚠ **魔数在文件尾**（见 2.2.7） | — |
| **E** | **WUHB** | 单文件 | `"WUHB"`@0，Aroma homebrew bundle | — |
| **F** | **WUMAD**（SDK mastering archive，极罕见） | ZIP | 含 `sip.fst.00000000.app`、`pXX.header.bin` | — |

> ⚠ **`.iso` 在 Wii U 语境下不是 ISO9660** —— Redump 把原始加密 WUD 镜像命名为 `.iso`。**在 Redump 的 Wii U DAT 里 grep `.wud` 会一无所获，判据必须用 `size == 25025314816`。**

#### 2.2.2 ⭐⭐ WUD 光盘：前 22 字节明文，零密钥可识别

**盘体大小 = `0x5D3A00000` = 25 025 314 816 字节（≈23.31 GiB）** —— **五个独立来源逐字互证**：

| # | 来源 | 证据 |
|---|---|---|
| 1 | [JNUSLib `WUDImage.java`](https://github.com/Maschell/JNUSLib) | `public static long WUD_FILESIZE = 0x5D3A00000L;` |
| 2 | [FIX94/wudump `src/main.cpp`](https://github.com/FIX94/wudump) | `#define SECTOR_SIZE 2048` + `while(readSectors < 0xBA7400)` → `0xBA7400 × 2048 = 25025314816` |
| 3 | [bodgit/wud](https://pkg.go.dev/github.com/bodgit/wud) | 导出常量 `UncompressedSize = 25025314816`、`SectorSize = 0x8000`、`GameKeyFile = "game.key"` |
| 4 | [FIX94/wud2app `wudparts.c`](https://github.com/FIX94/wud2app) | 分卷 `11×0x80000000 + 0x53A00000 = 0x5D3A00000` |
| 5 | **Redump DAT**【实测】 | 541/541 条 `size="25025314816"`；No-Intro Non-Redump 集 2 条 `.wud` 同值 |

⚠ **扇区大小有两套，别混**：
- **逻辑/加密扇区 = `0x8000`（32 KiB）** —— Cemu `constexpr size_t DISC_SECTOR_SIZE = 0x8000;`、JNUSLib `SECTOR_SIZE = 0x8000`、WudCompress `#define SECTOR_SIZE (0x8000)` 三方一致
- wudump 里的 `SECTOR_SIZE 2048` 是**光驱物理扇区**（读盘 API 单位），与格式无关

⭐ **offset 0 的 22 字节明文头** —— [rom-properties `wiiu_structs.h`](https://github.com/GerbilSoft/rom-properties/blob/master/src/libromdata/Console/wiiu_structs.h) 逐字：
```c
#pragma pack(1)
#define WIIU_MAGIC 'WUP-'
typedef struct _WiiU_DiscHeader {
	union RP_PACKED {
		uint32_t magic;		// [0x000] 'WUP-'
		char id[10];		// [0x000] "WUP-P-xxxx"
		struct RP_PACKED {
			char wup[3];	// [0x000] "WUP"
			char hyphen1;	// [0x003] '-'
			char p;		// [0x004] 'P'
			char hyphen2;	// [0x005] '-'
			char id4[4];	// [0x006] "xxxx"
		};
	};
	char hyphen3;		// [0x00A]
	char version[2];	// [0x00B] Version number, in ASCII (e.g. "00")
	char hyphen4;		// [0x00D]
	union {
		struct {
			char os_version[3];	// [0x00E] Required OS version, in ASCII (e.g. "551")
			char region[3];		// [0x011] Region code, in ASCII ("USA", "EUR")
			char hyphen5;		// [0x014]
			char disc_number;	// [0x015] Disc number, in ASCII
		};
		char os_update_full[8];		// [0x00E] Full OS update field
	};
} WiiU_DiscHeader;
ASSERT_STRUCT(WiiU_DiscHeader, 22);
#pragma pack()

// Secondary Wii U disc magic at 0x10000.
#define WIIU_SECONDARY_MAGIC 0xCC549EB9
```

| 偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| **`0x000`** | 4 | **magic** | `'WUP-'`（`57 55 50 2D`） | **极高（magic）** |
| `0x000` | 10 | **id** | `"WUP-P-xxxx"`（完整产品码） | **极高** |
| `0x006` | 4 | **id4** | 4 字符唯一标识（**ID4**，末位是地区字母） | **极高** |
| `0x00A` | 1 | hyphen3 | `-` | 格式校验 |
| `0x00B` | 2 | **version** | ASCII 版本号（如 `"04"`） | **高** |
| `0x00D` | 1 | hyphen4 | `-` | 格式校验 |
| `0x00E` | 3 | os_version | ASCII，如 `"551"` | 中 |
| `0x011` | 3 | region | ASCII，`"USA"` / `"EUR"` | ⚠ **中**（源码自带 `TODO: Is this the enforced region?`） |
| `0x014` | 1 | hyphen5 | `-` | 格式校验 |
| `0x015` | 1 | disc_number | ASCII | ⚠ **低**（源码自带 `TODO: Verify?`） |
| **`0x10000`** | 4 | **二级 magic** | `0xCC549EB9`（大端 `CC 54 9E B9`） | **极高** |

**三方交叉验证 offset 0 是明文**：
- **wudump** L323-326 逐字：`//get disc name for folder` / `char discId[11]; discId[10]='\0'; memcpy(discId, sectorBuf, 10);` —— **直接把盘首 10 字节当字符串做目录名**（README 示例目录 `WUP-P-TEST`）
- **Cemu** `TitleInfo.cpp` 要求两处同时命中：
  ```cpp
  uint8 wudMagic1[4] = { 0x57,0x55,0x50,0x2D }; // wud files should always start with "WUP-..."
  uint8 wudMagic2[4] = { 0xCC,0x54,0x9E,0xB9 };
  ```
- **Dolphin WiiU 分支** `VolumeWiiU.cpp`：`GetNames()` 读 offset 0 共 22 字节；`GetCountry()` 读 **offset 9**；`GetUniqueID()` 读 offset 6 起 7 字节拼 ID6

⚠ **必须排除 GCN/Wii 误判** —— rom-properties `WiiU.cpp` 逐字：
```cpp
	if (gcn_header->magic_wii == be32_to_cpu(WII_MAGIC) ||
	    gcn_header->magic_gcn == be32_to_cpu(GCN_MAGIC))
	{
		// GameCube and/or Wii magic is present.
		// This is not a Wii U disc image.
		return -1;
	}
```

**加密分界线：`[0, 0x18000)` 明文，`0x18000` 起加密。** Cemu / JNUSLib / wud2app 三方一致 —— 分区表在 `0x18000`，长 `0x8000`，AES-128-CBC(disc key, IV=0)。
JNUSLib `Settings.java`：`public static int WIIU_DECRYPTED_AREA_OFFSET = 0x18000;`
wud2app `main.c`：
```c
fseeko64(f, 0x18000, SEEK_SET); fread(partTblEnc,1,0x8000,f);
aes_decrypt(iv,partTblEnc,partTbl,0x8000);
magic = bswap32(*(uint32_t*)partTbl);
if(magic != 0xCCA6E67B) puts("Invalid FST!");
```

解密后的结构（Cemu `FST.cpp` 逐字，**仅在需要枚举分区内容时才用得上**）：
```cpp
struct DiscPartitionTableHeader   // at 0x18000, encrypted
{
	static constexpr uint32 MAGIC_VALUE = 0xCCA6E67B;
	/* +0x00 */ uint32be magic;
	/* +0x04 */ uint32be blockSize;              // must be 0x8000?
	/* +0x08 */ uint8 partitionTableHash[20];
	/* +0x1C */ uint32be numPartitions;
};
static_assert(sizeof(DiscPartitionTableHeader) == 0x20);

struct DiscPartitionTableEntry
{
	/* +0x00 */ uint8be partitionName[31];
	/* +0x1F */ uint8be numAddresses;
	/* +0x20 */ uint32be partitionAddress;
	/* +0x24 */ uint8 padding[0x80 - 0x24];
};
static_assert(sizeof(DiscPartitionTableEntry) == 0x80);

struct DiscPartitionHeader
{
	static constexpr uint32 MAGIC_VALUE = 0xCC93A4F5;
	/* +0x00 */ uint32be magic;
	/* +0x04 */ uint32be sectorSize;
	/* +0x10 */ uint32be h3HashNum;
	/* +0x14 */ uint32be fstSize;
	/* +0x18 */ uint32be fstSector;
	/* +0x24 */ uint8 fstHashType;
	/* +0x25 */ uint8 fstEncryptionType;
	/* +0x40 */ uint8be h3HashArray[32];
};
```
⭐ **分区名内嵌 Title ID** —— JNUSLib `WUDInfoParser.java`：
`String partitionName = "GM" + Utils.ByteArrayToString(Arrays.copyOfRange(rawTIK, 0x1DC, 0x1DC + 0x08));`
→ 游戏分区名形如 `GM0005000010101E00`；`SI` 分区内按 `<两位hex>/title.tik|title.tmd|title.cert` 存放。

Cemu 的 5 步流程注释（同文件）：
```cpp
	// 1) parse WUD headers and verify
	// 2) read SI partition FST
	// 3) find main GM partition
	// 4) use SI information to get titleKey for GM partition
	// 5) Load FST for GM
```
并明确**不信任盘上的 TMD**：`// but the console seems to ignore this file for disc images, at least when mounting, so we shouldn't rely on it either`

**密钥文件命名有两套约定，都要支持**：
- wudump README：`After it has been dumped the wud, common.key and game.key will be in a "wudump" folder` → `game.key` + `common.key`，各 16 字节
- Cemu `FST.cpp`：`keyPath.replace_extension("key");` → `Game.wud` 配 `Game.key`（**No-Intro 的 Non-Redump DAT 用的正是这套**）

**分卷 WUD**（JNUSLib `WUDDiscReaderSplitted.java` 逐字）：
```java
    public static long WUD_SPLITTED_FILE_SIZE = 0x100000L * 0x800L;   // = 0x80000000 = 2 GiB
    public static long NUMBER_OF_FILES = 12;
    public static String WUD_SPLITTED_DEFAULT_FILEPATTERN = "game_part%d.wud";
```
判据：文件名 == `game_part1.wud` 且大小 == `0x80000000`。前 11 卷 2 GiB，第 12 卷 `0x53A00000`。
WUDD 工具用另一套命名 `game.wud.part1..part12`。

#### 2.2.3 WUX 压缩格式 —— 头恒 `0x20`，含对齐填充

**格式规范原文**（[WudCompress `main.cpp`](https://github.com/cemu-project/WudCompress/blob/master/WudCompress/main.cpp) 逐字）：
```
		[Header]
		UINT32		magic1				"WUX0"
		UINT32		magic2				0x1099d02e
		UINT32		sectorSize			Size per uncompressed sector (SECTOR_SIZE constant)
		UINT64		uncompressedSize	Size of the Wii U image before being compressed
		UINT32		flags				Enable optional parts of the header (not used right now)
		[SectorIndexTable]
		UINT32[]	lookupIndex			... sectorCount = (uncompressedSize+sectorSize-1)/sectorSize
		[SectorData]
		UINT8[]		padding				Padding until the next field (sectorData) is aligned to sectorSize bytes
		UINT8[]		sectorData
```

⭐ **真实磁盘偏移含 C 对齐填充** —— [bodgit/wud](https://pkg.go.dev/github.com/bodgit/wud) 的 Go 结构体给出决定性澄清（逐字）：
```go
// The original tool read/wrote this using fread/fwrite so there's padding involved
type header struct {
	Magic            [2]uint32   // 0x00, 0x04
	SectorSize       uint32      // 0x08
	_                uint32      // 0x0C  ← 填充
	UncompressedSize uint64      // 0x10
	Flags            uint32      // 0x18
	_                uint32      // 0x1C
}
```

| 偏移 | 长度 | 字段 | 值 | 可靠性 |
|---|---|---|---|---|
| `0x00` | 4 | `magic0` | `"WUX0"` = `57 55 58 30` | **极高（magic）** |
| `0x04` | 4 | `magic1` | `0x1099D02E` = `2E D0 99 10`（小端落盘） | **极高（magic）** |
| `0x08` | 4 | `sectorSize` | Cemu 校验 `>= 0x100 && < 0x10000000` | 结构必需 |
| `0x0C` | 4 | **（填充）** | — | — |
| `0x10` | 8 | `uncompressedSize` | 解压后 WUD 大小 | 结构必需 |
| `0x18` | 4 | `flags` | **未使用，恒 0** | 忽略即可 |
| `0x1C` | 4 | （填充） | — | — |
| **头总长** | **`0x20`** | 索引表起于 `0x20` | | |

> ⚠ **JNUSLib 把 `0x0C` 当 flags 读是错的**（那是填充）；但 flags 恒 0 且规范注明未使用，**识别器直接跳过该字段即可**。
> **8 字节魔数序列 `57 55 58 30 2E D0 99 10` 可一次性比对**（Cemu 就是这样做的），不受对齐影响。

#### 2.2.4 TMD / Ticket —— 数字版的权威锚点，且**完全明文**

来源：[wiiubrew — Title metadata](https://wiiubrew.org/wiki/Title_metadata)（⚠ **该站对默认 UA 返回 403，需带浏览器 UA**）

**Signed blob header**：`0x000` 签名类型(4) / `0x004` 签名(256) / `0x104` padding modulo 64(60)

**Main header**（绝对偏移，**对 `RSA_2048` 成立**）：

| 绝对偏移 | 长度 | 字段 | 可靠性 |
|---|---|---|---|
| `0x140` | 64 | Issuer（`Root-CA%08x-CP%08x`） | 见下（判别用） |
| `0x180`–`0x183` | 1×4 | Version / ca_crl_version / signer_crl_version / padding | 低 |
| `0x184` | 8 | System Version | 中 |
| **`0x18C`** | **8** | **Title ID** | **极高（主键）** |
| `0x194` | 4 | Title type | 中 |
| `0x198` | 2 | **Group ID**（publisher） | 中–高 |
| `0x19A` | 62 | reserved | — |
| `0x1D8` | 4 | Access rights | 低 |
| **`0x1DC`** | **2** | **Title version** | **高** |
| `0x1DE` | 2 | Number of contents | 结构必需 |
| `0x1E0` / `0x1E2` | 2 / 2 | boot index / Minor version | 低 |

**v1 header**：`0x1E4` 32 字节 SHA-256 of CMD groups；`0x204` 2304 字节 = 64 个 CMD group（每组 `0x00` offset(2) / `0x02` CMD 数(2) / `0x04` SHA-256(32)）

⚠⚠ **Content entry 的大小：Wii U 是 `0x30`，不是 wiiubrew C 片段里的 `0x24`**。
wiiubrew 页面上那段 `content_record`（`// size: 0x24 bytes`）是 **Wii 时代的**。**三方确证 Wii U 为 `0x30`**：Cemu `static_assert(sizeof(TMDFileContentEntryWiiU) == 0x30)`、JNUSLib `CONTENT_SIZE = 0x30`、rom-properties `ASSERT_STRUCT(WUP_Content_Entry, 48)`。
Content 条目起于 **`0xB04`**（= `0x204 + 64 × 0x24`）。
Content type 位：`CONTENT_ENCRYPTED = 0x0001`、**`CONTENT_HASHED = 0x0002`（决定是否有 `.h3`）**、`CONTENT_FLAG_UNKWN1 = 0x4000`。

⭐ **TMD / Ticket 的极简判别** —— [CDecrypt](https://github.com/VitaSmith/cdecrypt)：
```c
#define T_MAGIC_OFFSET  0x0150
#define TMD_MAGIC       0x4350303030303030ULL   // "CP000000"
#define TIK_MAGIC       0x5853303030303030ULL   // "XS000000"
```
原理：Issuer 字段从 `0x140` 起是 `Root-CA00000003-CP0000000b`，前 16 字符 `Root-CA00000003-` 之后正好落在 `0x150`。

**Wii U TMD 自检**（JNUSLib）：`if ((titleID & 0x0005000000000000L) != 0x0005000000000000L) throw new ParseException("Invalid TMD file. This is not a Wii U TMD", 0);`
**Ticket**：Title ID 在 **`0x1DC`**，加密 title key 在 **`0x1BF`**（16 字节）。

> ⭐⭐ **TMD 与 Ticket 本身完全明文（只有 `.app` 内容体加密）** —— 这意味着**数字版识别完全不需要任何密钥**。
> ⚠ 与第一部分 A.6.3 对 3DS TMD 的结论同构：**应读首个 u32 签名类型再定位，不要硬编码 `0x140`**（`RSA_2048 = 0x00010001` 签名 256 字节；`RSA_4096 = 0x00010000` 签名 512 字节）。

#### 2.2.5 `meta.xml` / `app.xml` / `cos.xml` —— 根元素与权威性

⚠ **三个 XML 的根元素是最高频的陷阱**（两方独立互证）：

| 文件 | 根元素 |
|---|---|
| `code/app.xml` | `<app>` |
| `code/cos.xml` | `<app>` |
| **`meta/meta.xml`** | **`<menu>`（不是 `<meta>`！）** |

- Cemu：`const auto root = app_doc.child("app");` / `const auto root = meta_doc.child("menu");`
- rom-properties [`WiiUPackage_xml.cpp`](https://github.com/GerbilSoft/rom-properties/blob/master/src/libromdata/Console/WiiUPackage_xml.cpp)：
  ```cpp
  int retSys  = loadSystemXml(appXml,  "/code/app.xml",  "app");
  int retCos  = loadSystemXml(cosXml,  "/code/cos.xml",  "app");
  int retMeta = loadSystemXml(metaXml, "/meta/meta.xml", "menu");
  ```

**`app.xml` 真实样例**（[wiiubrew `/vol/code`](https://wiiubrew.org/wiki//vol/code) 逐字）：
```xml
  <?xml version="1.0" encoding="utf-8"?>
    <app type="complex" access="777">
     <version type="unsignedInt" length="4">15</version>
     <os_version type="hexBinary" length="8">000500101000400A</os_version>
     <title_id type="hexBinary" length="8">0005000010183300</title_id>
     <title_version type="hexBinary" length="2">0000</title_version>
     <sdk_version type="unsignedInt" length="4">21012</sdk_version>
     <app_type type="hexBinary" length="4">80000000</app_type>
     <group_id type="hexBinary" length="4">00001833</group_id>
     <os_mask type="hexBinary" length="32">00000…000</os_mask>
  </app>
```
> ⚠ **进制陷阱**：Cemu 解析时 `title_id`/`title_version`/`app_type`/`group_id` 按**十六进制**，`sdk_version` 按**十进制**。

**`cos.xml`** 承载的唯一识别相关字段是 **`<argstr>` = 要启动的 `.rpx` 文件名**（其余全是内存/栈配额）。rom-properties 也只取这一个：`ADD_TEXT(cosRootNode, "argstr", "argstr");`

⭐⭐ **权威性排序** —— wiiubrew [Title](https://wiiubrew.org/wiki/Title) 页逐字：
> "A title's title ID is stored in the TMD, ticket, app.xml, and meta.xml. ... Note that due to the way the Wii U handles game updates, the title ID in meta.xml for update titles will be the title ID of the base title. Separately, the title ID in meta.xml can be different from the actual title ID. **The actual title ID of a title will always be in the TMD, ticket, and app.xml.** At the very least, a complete title will always include either a TMD or app.xml."

同页另有：*"For unknown reasons, the version in meta.xml may be incorrect."*

> **→ 实现建议：title id 取值优先级 TMD > ticket > app.xml > meta.xml。`meta.xml` 只用来取人可读名。**

**`meta.xml` 的字段表**（[wiiubrew Meta.xml](https://wiiubrew.org/wiki/Meta.xml)，页首逐字：*"For certain titles, meta.xml is an exact copy of its app.xml instead of the usual meta.xml format."*）：

| Key | 类型 | 大小 | 说明 | 可靠性 |
|---|---|---|---|---|
| **`product_code`** | string | 32 | *"title product code it's **WUP-X-\*\*\*\***"* | **高** |
| **`company_code`** | string | 8 | 发行商代码 | **高**（ID6 必需） |
| `mastering_date` | string | 32 | 压盘日期 | 中 |
| `title_id` | hexBinary | 8 | *"Upper TID followed by lower TID. Example, `00050000-12345678`. The installed game would be in `mlc:/usr/title/00050000/12345678`"* | ⚠ **中**（可能与真实 title id 不符，见上） |
| `title_version` | unsignedInt | 4 | ⚠ 可能错 | 中 |
| `group_id` | hexBinary | 4 | — | 中 |
| **`region`** | hexBinary | 4 | 位掩码，`FFFFFFFF` = region free | **高** |
| `app_launch_type` | hexBinary | 4 | `00000000` = 菜单可启动；`00000001` = autoboot | 中 |
| `pc_cero`/`pc_esrb`/…/**`pc_cgsrr`**/`pc_oflc` | unsignedInt | 4 | 各国分级（`pc_cgsrr` 是台湾 CGSRR） | 低 |
| ⭐ **`longname_ja`/`_en`/`_fr`/`_de`/`_it`/`_es`/`_zhs`/`_ko`/`_nl`/`_pt`/`_ru`/`_zht`** | string | **512** | **12 语言槽长标题** | **极高（显示名）** |
| `shortname_*`（同 12 语言） | string | 512（ja/en）/ 256 | 短标题 | 高 |
| `publisher_*`（同 12 语言） | string | 256 | 发行商名 | 中–高 |

**Cemu 实际解析的字段**（`ParsedMetaXml.h` 逐字）：
```cpp
struct ParsedMetaXml
{
	uint32 m_version;
	std::string m_product_code;
	std::string m_company_code;
	std::string m_content_platform;
	uint64 m_title_id;
	CafeConsoleRegion m_region;
	std::array<std::string, 12> m_long_name;
	std::array<std::string, 12> m_short_name;
	std::array<std::string, 12> m_publisher;
	uint32 m_olv_accesskey;

	std::string GetLongName(CafeConsoleLanguage languageId) const
	{
		return m_long_name[(size_t)languageId].empty() ? m_long_name[(size_t)CafeConsoleLanguage::EN] : m_long_name[(size_t)languageId];
	}
	...
};
```
→ **12 个语言槽，取不到就回退英文** —— 与 PSV 的 `TITLE_xx` → `TITLE` 回退逻辑同构。

#### 2.2.6 产品码 / ID6 / Title ID 分类

**产品码格式**（wiiubrew Title 页逐字）：
> "The 3 segments that compose a product code are the platform (always "WUP" for Wii U), the app type, and a unique identifier provided to developers by Nintendo. The first 3 characters of the unique identifier is a combination of uppercase letters and numbers. The last character of the unique identifier is meant to represent the locale and/or the supported languages of the title."

| 段 | 取值 |
|---|---|
| **App Type**（第 2 段） | `B`=Kiosk Interactive Demo · `M`=DLC · `N`=Digital-Only · **`P`=Standard** · `T`=eShop Demo · `U`=Update |
| **Locale**（第 3 段第 4 字符） | `A`=All · `D`=Germany/占位 · `E`=USA 或 USA&Europe · `F`=France · `I`=Italy · `J`=Japan · `P`=Europe · `R`=Russia · `S`=Spain · `X`=Scandinavia/Australia · `Y`=USA · `Z`=USA/Europe/… |

⚠ **占位值必须当"无信息"处理**（wiki 逐字）：*"'WUP-P-ABCD' is commonly used as a placeholder product code, and it is shared by many titles"*；company code 占位是 `ZZZZ`。
⚠ 有的 `meta.xml` 存**无连字符**形式，wiki 逐字：*"if meta.xml contains 'WUPMDHQA', it should be written as 'WUP-M-DHQA'"*。

⭐ **ID6 推导规则** —— [loadiine_gx2 `src/utils/xml.c`](https://github.com/dimok789/loadiine_gx2/blob/master/src/utils/xml.c) 逐字：
```c
if(XML_GetNodeText(xmlData, "product_code", xmlNodeData, XML_BUFFER_SIZE) && strlen(xmlNodeData) == 10)
    strncpy(id6, xmlNodeData + 6, 4);
if(XML_GetNodeText(xmlData, "company_code", xmlNodeData, XML_BUFFER_SIZE) && strlen(xmlNodeData) == 4)
    strncpy(id6 + 4, xmlNodeData + 2, 2);
id6[6] = 0;
```
> **`ID6 = product_code[6:10] + company_code[2:4]`**

【实测验证】libretro `dat/Nintendo - Wii U.dat` 中 Disney Infinity 2.0 (Europe) 的 `serial "ADRP4Q"`；wiiubrew company code 表记 `004Q = Disney Interactive Studios`。`ADRP` + `4Q` = `ADRP4Q` ✓
对该 DAT 全部 **2871** 条 serial 统计：
- 地区字母 `id4[3]`：`E`=1107、`P`=927、`J`=802、`Z`=12、`D`=10、`A`=4、`I`/`S`/`F`=2、`X`/`R`/`Y`=1 —— **与 wiiubrew locale 字母表同集合**
- 公司后缀：`01`(Nintendo 0001)=128、`52`(Activision)=50、`41`(Ubisoft)=49、`AF`(Namco Bandai)=28、`4Q`(Disney)=17、`08`(Capcom)=15、`8P`(Sega)=11、`GD`(Square Enix)=9、`C8`(Tecmo Koei)=8 —— **全部等于 wiiubrew company code 去掉前导 `00`**

**libretro 的 `serial` 字段装的是 ID4 或 ID6**（实测长度分布：4 字符 2415 条、6 字符 456 条），**无任何哈希**，rom 名一律 `.wux` 后缀：
```
game (
	name " Disney Infinity 2.0: Play Without Limits (Europe)"
	serial "ADRP4Q"
	developer "Avalanche Software (USA)"
	publisher "Disney Interactive"
	releaseyear 2014
	rom ( name " Disney Infinity 2.0: … (Europe).wux"  serial "ADRP4Q" )
)
```
（version 2022.10.28，头部 `homepage "https://github.com/RobLoach/libretro-database-gametdb"` → 源自 GameTDB）

⭐ **Title ID 高 32 位分类**（wiiubrew Title 页 + Cemu `TitleId.h`，与本文实测的 No-Intro CDN 频次逐条对应）：

| 高 32 位 | 含义 | Cemu 枚举 | 【实测】CDN 频次 |
|---|---|---|---|
| `00050000` | Game Application Titles（游戏本体） | `BASE_TITLE = 0x00` | **2795** |
| `00050002` | Kiosk Interactive Demo / eShop Demo | `BASE_TITLE_DEMO = 0x02` | 135 |
| `0005000C` | Game DLC Titles | `AOC = 0x0C` | 144 |
| `0005000E` | Game Update Titles | `BASE_TITLE_UPDATE = 0x0E` | 431 |
| `00050010` | System Application Titles | `SYSTEM_TITLE = 0x10` | 54 |
| `0005001B` | Shared Data Title | `SYSTEM_DATA = 0x1B` | 26 |
| `00050030` | Overlay Application Titles | `SYSTEM_OVERLAY_TITLE = 0x30` | 46 |
| `00000007` | vWii Essential System Titles | — | 32 |
| `00070002` / `00070008` | vWii System Channel / Hidden Channel | — | 6 / 6 |

其他：`0005000B`=Shared User Data · `0005004E`=Disc Update Package · `00050001/00058001/0005C001`=Cafe SDK NDEBUG/DEBUG/FDEBUG OS。

结构解释（wiiubrew 逐字）：*"The first 2 bytes are the platform ID, which is 5 for Wii U titles. The second 2 bytes are likely a bitmask."*
位掩码：`0x1`=非可执行数据/SDK OS · `0x2`=Free · `0x4`=Patch · `0x8`=Add-on content · `0x10`=System title · `0x20`=后台运行 · `0x40`=Disc GI 分区 · `0x4000`=SDK FDEBUG · `0x8000`=SDK DEBUG。

Cemu `TitleId.h` 逐字：
```cpp
	uint8 GetTypeByte() const { return (m_titleId >> 32) & 0xFF; }
	uint16 GetPlatformWord() const { return (m_titleId >> 48) & 0xFFFF; }
	bool IsPlatformCafe() const { return GetPlatformWord() == 0x0005; }
	bool IsSystemTitle() const { return (GetTypeByte() & 0x10) != 0; }
```

> ⚠⚠ **大坑**：`app.xml` 的 `<app_type>` 与 title id 的类型字节是**两套无关编码，且尾字节恰好相反**。Cemu `AppType.h`：`GAME = 0x80000000, GAME_UPDATE = 0x0800001B, GAME_DLC = 0x0800000E` —— UPDATE 结尾是 `1B`、DLC 结尾是 `0E`，而 title id 里 update 是 `0E`、DLC 是 `0C`。**切勿互推。**

**区域码**（`meta.xml` `<region>`，位掩码）：`0x1`=JPN · `0x2`=USA · `0x4`=EUR · `0x8+`=Reserved · `0x7`=ALL · `0xFFFFFFFF`=占位全区。rom-properties 取 `region & 0x7F`。
> ⭐ **只有三个真实地区位** —— 这是 2.2.9 中文结论的硬件级依据。

#### 2.2.7 `.rpx` / `.rpl` —— **不含任何 title 身份信息**

**头部**（修改版 ELF）。Cemu `rpl.cpp` 用 9 字节 ident 区分：
- RPX = `7F 45 4C 46 01 02 01 CA FE`
- 普通 ELF = `7F 45 4C 46 01 02 01 00 00`

即标准 ELF magic + class=1(32bit) + data=2(**大端**) + version=1 + **EI_OSABI=`0xCA`** + **EI_ABIVERSION=`0xFE`**（合起来 `EABI_CAFE = 0xcafe`）。
ELF header `0x34`、section header `0x28`、`e_machine = EM_PPC(20)`。**无 program header** —— [wiiubrew RPL](https://wiiubrew.org/wiki/RPL) 逐字：*"there are no program headers (section headers are used to load the executable into memory instead)"*，且 *"some sections are zlib-compressed"*。

**RPL 自定义 section 类型**（decaf-emu `cafe_loader_rpl.h`、Cemu `rpl_structs.h`、RetroArch elf2rpl `elf.h` 三方逐字一致）：
```
SHT_RPL_EXPORTS  = 0x80000001
SHT_RPL_IMPORTS  = 0x80000002
SHT_RPL_CRCS     = 0x80000003
SHT_RPL_FILEINFO = 0x80000004
SHF_DEFLATED / SHF_RPL_COMPRESSED = 0x08000000
SHF_TLS = 0x04000000
```
> ⚠ **【未找到权威来源】** 字符串 `"RPL_CRCS"` / `"RPL_FILEINFO"` 在 Cemu 与 decaf-emu 中**都不作为字面量存在** —— 它们只是 `#define` 宏名，不写进 `.shstrtab`。**不能靠 grep 字符串识别。** 定位靠位置约定（Cemu `rpl.cpp`）：**倒数第 1 个 section 必须是 FILEINFO，倒数第 2 个必须是 CRCS。**

**RPLFileInfo** 魔数三版本：`0xCAFE0300`(`0x40`) / `0xCAFE0401`(`0x40`) / `0xCAFE0402`(**`0x60`**)。v4.2 的 25 个字段全部是内存布局/对齐/TLS/工具链信息 —— **没有 title id、product code、group id**；`filename` 只是字符串表偏移。
RPX vs RPL 判别：`RPL_IS_RPX = 1 << 1`（flags 位于 FILEINFO `+0x34`）；Cemu：`bool IsRPX() const { return fileInfo.flags & 2; }`

⭐ **Cemu 直接启动 `.rpx` 时的 title id 从哪来？答案：伪造。** `CafeSystem.cpp` 逐字：
```cpp
	// since a lot of systems (including save folder location) rely on a TitleId, we derive a placeholder id from the executable hash
	uint32 h = generateHashFromRawRPXData(execData->data(), execData->size());
	sForegroundTitleId = 0xFFFFFFFF00000000ULL | (uint64)h;
```
哈希函数（种子逐字）：
```cpp
	uint32 h = 0x3416DCBF;
	for (sint32 i = 0; i < size; i++) { uint32 c = rpxData[i]; h = (h << 3) | (h >> 29); h += c; }
```
（`.wuhb` 同样伪造：`(0x0005000Full << 32) | crc32(author + longName + shortName)`）

> **→ `.rpx` 的可识别性评级：不可用。** 唯一能做的是对文件本身求哈希当指纹。
> 旁证：No-Intro `Unofficial - Nintendo - Wii U (Digital) (Deprecated)` 里确实收了 74 条 `code\*.rpx`，但那是按 loadiine 目录整体收录，rpx 只是其中一个文件。

**`.wua` = ZArchive**（作者同为 Cemu 的 Exzap）。README 逐字：*"Originally this format was created to store Wii U games dumps. These use the file extension .wua (Wii U Archive) but are otherwise regular ZArchive files."*
⚠⭐ **魔数在文件尾，不在文件头** —— `zarchivecommon.h` 逐字：
```cpp
	struct Footer
	{
		static inline uint32_t kMagic = 0x169f52d6;
		static inline uint32_t kVersion1 = 0x61bf3a01; // also acts as an extended magic
		...
		uint8_t integrityHash[32];
		uint64_t totalSize;
		uint32_t version;
		uint32_t magic;
	};
	static_assert(sizeof(Footer) == (16 * 6 + 32 + 8 + 4 + 4));   // = 0x90
```
落盘为**大端** → **文件最后 4 字节 = `16 9F 52 D6`**，其前 4 字节 = `61 BF 3A 01`，Footer 共 `0x90` 字节。
（这正是 Cemu `DetermineCafeSystemFileType` 里写着 `// check for WUA / todo` 的原因 —— 它只读文件头。）
`.wua` 内每个 title 一个子目录，命名 `<16位hex titleId>_v<十进制version>`（Cemu `ParseWuaTitleFolderName`）。

**`.wuhb`**：magic `"WUHB"`，romfs 变体，元数据在 **`meta/meta.ini`**（不是 meta.xml），title id 同样伪造。

⭐ **魔数速查表（磁盘字节序列）**：
```
WUX          0x00      57 55 58 30 2E D0 99 10        (8 字节一起比)
WUD          0x00      57 55 50 2D                    ("WUP-")
WUD 二级     0x10000   CC 54 9E B9                    (BE)
分区 TOC     0x18000   CC A6 E6 7B                    (需先 AES 解密, BE)
分区头       分区起始   CC 93 A4 F5                    (需先 AES 解密, BE)
FST          +0x00     46 53 54 00                    ("FST\0", BE)
TMD 判别     0x150     43 50 30 30 30 30 30 30        ("CP000000")
TIK 判别     0x150     58 53 30 30 30 30 30 30        ("XS000000")
RPX ident    0x00      7F 45 4C 46 01 02 01 CA FE
RPL FILEINFO —         CA FE 04 02                    (BE)
.wua         EOF-4     16 9F 52 D6                    (BE，★在文件尾)
.wuhb        0x00      57 55 48 42                    ("WUHB")
```

#### 2.2.8 NUS / CDN 目录形态 —— 两套命名，必须都支持

| 子形态 | TMD | Ticket | 内容文件 | 哈希文件 |
|---|---|---|---|---|
| **已安装 / WUP Installer 目录** | `title.tmd` | `title.tik`（+`title.cert`） | `%08X.app` | `%08X.h3` |
| **CDN 原始下载** | `tmd` 或 `tmd.<version>` | `cetk` 或 `cetk.<version>` | **裸 8 位 hex，无扩展名** | `%08x.h3` |

JNUSLib `Settings.java` 逐字：
```java
    public static final String TMD_FILENAME = "title.tmd";
    public static final String TICKET_FILENAME = "title.tik";
    public static final String CERT_FILENAME = "title.cert";
    public static final String ENCRYPTED_CONTENT_EXTENTION = ".app";
    public static final String H3_EXTENTION = ".h3";
    public static final String WUD_KEY_FILENAME = "game.key";
    public static String URL_BASE = "http://ccs.cdn.c.shop.nintendowifi.net/ccs/download";
```
文件名用 **Content ID**（非 index）：`String.format("%08X%s", getID(), ENCRYPTED_CONTENT_EXTENTION)`。JNUSLib 用 `getFileIgnoringFilenameCases()` 做**大小写不敏感**查找。
CDecrypt 为兼容现实世界试 **4 种模式**：
```c
    const char* pattern[] = { "%s%c%08x.app", "%s%c%08X.app", "%s%c%08x", "%s%c%08X" };
```

rom-properties 的目录判定（31 行搞定两种形态）：
```cpp
	static const array<const char*, 3> NUS_package_filenames = {{
		"title.tik", "title.tmd", "title.cert",
	}};
	// Check for an extracted title.
	// NOTE: Ticket, TMD, and certificate chain might not be present.
	static const array<const char*, 3> extracted_package_filenames = {{
		"code/app.xml", "code/cos.xml", "meta/meta.xml",
	}};
```

【实测】No-Intro `Nintendo - Wii U (Digital) (CDN)` 的实际分布（**3682 game / 168 387 rom**，平均 46 文件/title）：

| 文件类型 | 数量 | 语义 |
|---|---|---|
| 裸 8 位 hex content id | **105 232** | 加密内容体 |
| `.h3` | **56 069** | 哈希树（对应 content type `& 0x2`） |
| `tmd.<N>` | 6 425（3682 个 game 每个都有，`<N>` = title version） | TMD |
| `cetk.<N>` + `cetk` | 765 + 104 | Ticket（只有 602 个 game 有票；**【推断】**其余是系统/无票 title） |
| `.app` / `title.tmd` / `title.tik` | **0** | 印证这是 CDN 原样形态 |

`<game_id>` 是 **16 位十六进制 Title ID**（如 `000500101004b000`）。

> ⭐ **识别只需要 TMD 一个文件**（读 `0x18C` 的 Title ID），**不需要逐一哈希那 46 个内容文件**。

**FST（解密后才可见）**：magic `0x46535400` = `"FST\0"`，header `0x20`（magic@`0x00` / offsetFactor@`0x04`（通常 `0x20`）/ numCluster@`0x08` / hashIsDisabled@`0x0C`）。Cluster 条目 `0x20`：offset / size / **ownerTitleId@`0x08`(u64 BE)** / groupId@`0x10` / hashMode@`0x14`。File 条目 `0x10`。
> **【推断】** `ownerTitleId` 是 FST 内一条**尚未被任何实现利用**的 title id 来源，但 Cemu 从不读它，且必须先解密。

**H3 哈希块**（rom-properties）：`WUP_H3_SECTOR_SIZE_ENCRYPTED 0x10000` / `..._DECRYPTED 0xFC00` / `..._DECRYPTED_OFFSET 0x400` —— 每 64 KB 块前 `0x400` 是 h0/h1/h2 哈希，后 `0xFC00` 是数据。

**Loadiine / HOST_FS 目录**（loadiine_gx2 `src/common/common.h` 逐字）：
```c
#define WIIU_PATH               "/wiiu"
#define GAMES_PATH              "/games"
#define SD_GAMES_PATH           WIIU_PATH GAMES_PATH
#define CONTENT_PATH            "/content"
#define RPX_RPL_PATH            "/code"
#define META_PATH               "/meta"
```
SD 布局 `sd:/wiiu/games/<游戏名 [ID6]>/{content,code,meta}/`；目录名尾部 `[ID6]`(8 字符) 或 `[ID4]`(6 字符)，无后缀则读 meta.xml 推导。

#### 2.2.9 DAT / 数据库覆盖

| 库 | 光盘 | 数字 / CDN | 哈希粒度 | 收 disc key |
|---|---|---|---|---|
| ⭐ **Redump**（`redump.info`，2026-08-28） | **541**（`.iso` 后缀，实为 raw WUD） | — | 整盘 CRC32+MD5+SHA-1（**无 sha256、无 serial**）。分类 Games 503 / Applications 34 / Preproduction 2 / Demos 2 | ❌ **0 条 `.key`** |
| **No-Intro** | 2 条 `.wud`（`Non-Redump` 集，共 3 game） | CDN **3682** game / **168 387** rom；Dev 1137；Lotcheck 159；Deprecated 142 | 逐 NUS 文件，crc+md5+sha1+sha256；带 `<game_id>` | ✅ **16 字节 key 作为独立 `<rom>` 收录** |
| **libretro / GameTDB** | 2871（`.wux` 后缀，**无哈希**） | 3373（No-Intro 裁剪） | 仅 `serial`（ID4/ID6） | ❌ |
| **TOSEC** | **0** | **0** | — | — |
| **No-Intro（Dev Kit HDD）** | 3 game（`.img` 整盘 320/500 GB）+ `otp.bin`(1024 B) + `seeprom.bin`(512 B) | — | — | — |
| **MAME / RetroAchievements** | 0 | 0 | `RC_CONSOLE_WII_U = 20` 但 `hash.c` **零实现** | — |

**Redump 的密钥政策**（[wiki.redump.info Nintendo Wii U Dumping Guide](https://wiki.redump.info/Nintendo_Wii_U_Dumping_Guide) 逐字）：
> `* Keep your game.key file for personal use`

MediaWiki 修订 **66296**（2026-07-10，摘要 `Remove mention of game.key for submission`）刚刚删掉了原先的 `Make sure you submit the game.key!`。
> **→ Redump 从不公开分发 Wii U 密钥；要拿 key 只能去 No-Intro 的 `Non-Redump` 集（仅 2 条）。**

⭐ **四套并列的 DAT 匹配键**：
1. **Redump** → `size == 25025314816` **预筛** + SHA-1 匹配整盘（**别按 `.wud` 后缀过滤**）
2. **No-Intro CDN** → `<game_id>` = TMD `0x18C` 的 Title ID，或逐内容文件哈希
3. **No-Intro Non-Redump** → `.wud` SHA-1 + 独立的 16 字节 `.key` 条目
4. **libretro / GameTDB** → `serial`（ID4 或 ID6，由 `product_code` + `company_code` 推导）

#### 2.2.10 中文覆盖 —— **官中与汉化双双为零**

这是本文 8 个平台里**唯一一个连"官方中文版"都没有**的平台。**四条独立证据链**：

**（1）硬件层无中文区**
`meta.xml` 的 region 位掩码**只有 JPN/USA/EUR 三位**。wiiubrew 逐字：*"if Nintendo had chosen to create a new console region (for example if they decided to later release the Wii U in South Korea), all titles with a region of '00000007' would have to be updated"*。Cemu 虽定义了 `CHN=0x10, KOR=0x20, TWN=0x40`，但**无 title 使用**。

**（2）Redump 自家筛选器**【实测】

| 查询 | 结果 |
|---|---|
| `system=WIIU&region=cn` | **0 discs found** |
| `region=tw` | **0** |
| `region=kr` | **0** |
| `language=zh` | **5**（Darksiders Warmastered、Minecraft Wii U Edition、Tumblestone —— **全是欧美版内置 Zh 语言选项**） |

541 条区域分布：`USA 203 / Europe 180 / Japan 124 / World 8 / Scandinavia 6 / Europe,Australia 5 / Latin America 4 / Germany 4 / France 2 / USA,Asia 2 / Australia 2 / Russia 1` —— **无 China / Taiwan / Korea**。

**（3）No-Intro**【实测】6 个 Wii U DAT 共 5126 条 game：`(China)`/`(Taiwan)`/`(Korea)` 区域标记 **0 条**；`Zh` 语言标记仅 **6 条**，全是 3 个西方游戏（EvoFish、Gravity Badgers、Minecraft Wii U Edition 及其 Update/DLC）。

**（4）wiiubrew Title database**【实测】4951 个 title ID：区域列只有 `USA 1460 / EUR 1285 / JPN 997 / ALL 188 / EUR-USA 18`；全库检索 `chinese|china|korea|taiwan` 仅 5 处命中，**全部是游戏标题本身**（`THE 功夫 (China Warrior)`、`スーパーチャイニーズ` 等 VC 老游戏），无一是地区标记。

**（5）官方从未在中国大陆发售**
【事实】神游科技（iQue (China) Ltd.，任天堂在华唯一企业）官网 <https://www.ique.com/> 「主机」栏目**全部**产品逐字：`iQue 3DS XL / iQue DSi / iQue DS Lite / iQue DS / iQue micro / iQue SP / iQue GBA / iQue Player` —— **不含 Wii 与 Wii U**。

**（6）TOSEC / 民间汉化**
TOSEC 官方 2025-03-13 pack 全部 4743 个 DAT 中，"Wii U" 只出现在 **5 个 TOSEC-PIX** DAT（扫描件/广告视频，共 101 条），**`TOSEC/` 与 `TOSEC-ISO/` 中不存在任何 Wii U 软件 DAT**。
GitHub 全站检索 `wiiu 汉化` / `Wii U 汉化` / `wii u chinese translation patch` 等，仅命中 2 个 0★ 个人项目（`AKA10195/WiiPartyU_Chinese_Translation` 等），**无任何带哈希的 DAT 或数据库**。

> ⚠ **格式层面的巨大反差**：`meta.xml` **明确定义了 `longname_zhs` / `longname_zht` / `shortname_zhs` / `shortname_zht` / `publisher_zhs` / `publisher_zht` 六个中文字段，还有台湾 `pc_cgsrr` 分级字段** —— 格式完全支持中文，但**实际发行为零**。识别器仍应读这两个槽，但不要指望命中。

#### 2.2.11 实现成本评估

⭐ **最重要的成本结论：六种识别目标全部零密钥**

| 目标 | 需 AES？ | 依据 |
|---|---|---|
| WUX 头嗅探 | ❌ | 头 `0x20` 明文 |
| WUD 头嗅探 | ❌ | **前 22 字节明文 ASCII** |
| 分卷 WUD | ❌ | 文件名 + 精确大小 |
| NUS 目录 + TMD 字段 | ❌ | **TMD/TIK 本身完全明文**，只有 `.app` 内容体加密 |
| Loadiine 目录 + XML | ❌ | 明文 XML |
| RPX/ELF 嗅探 | ❌ | 标准 ELF 头 |
| *WUD 内部分区 / FST* | ✅ **必须** | JNUSLib `createAndLoad(discReader, byte[] titleKey)` 强制要 key |

旁证：Rust crate `sachet` 文档逐字：*"Sometimes you just need to parse TMD's/FST's as while they're packaging formats, they're used in many aspects of the Wii-U. You don't want the burden of cryptographic dependencies to compile."*（crypto 是可关闭 feature）

**Rust crates 现状**（碎片化，**无任何 `.wud` 实现**）：

| crate | 版本 | 下载 | 许可 | 覆盖 |
|---|---|---|---|---|
| `sachet` | 0.0.14（2026-05-11） | 120 | MIT | NUS/TMD/Ticket/FST（6564 行）。⚠ pre-1.0、托管 Codeberg、不承诺 SemVer |
| ⭐ `wux` | 0.1.0（2023-11-03） | **2105** | MIT OR Apache-2.0 | WUX 解压，**仅 154 行**（基于 binrw）。3 年未更新但格式已冻结 |
| `cargo-wiiu` | 0.2.2（2026-08-11） | 82 | — | ELF→RPX；`src/elf.rs` 含**全套 RPL 常量**（与 decaf 逐字一致，可直接抄） |
| `zelzip_niiebla` | 0.4.0 | 1214 | MPL-2.0 | 通用 TMD/Ticket；**无 Wii U 专属逻辑** |
| ⭐ `datary` | 0.3.0 | 2089 | Apache-2.0 | 读写 No-Intro/Logiqx/Redump/TOSEC DAT |
| `nod` | 2.0.0-alpha.9 | 37 395 | MIT/Apache | ⚠ **不支持 Wii U**。其 `NFS (Wii U VC)` 是 Wii U 虚拟主机包装的 **Wii** 盘；全仓无 `.wud`/`.wux` 代码 |
| `wiiu_swizzle` | 0.3.0 | 7653 | MIT | GX2 纹理 tiling，**与识别无关** |

**参考实现与许可**（均已核实）：

| 项目 | 语言 | 许可 | 价值 |
|---|---|---|---|
| ⭐ **rom-properties** | C++ | **GPL-2.0-or-later** | **最直接对标 —— 本身就是 ROM 识别器**。Wii U 部分 3587 行：`WiiU.cpp` 629 / `WiiUPackage.cpp` 985 / `WiiUPackage_xml.cpp` 692 / `WiiUFst.cpp` 680 / `WiiUH3Reader.cpp` 405 / `wiiu_structs.h` 196 |
| ⭐ **Cemu** | C++ | **MPL-2.0** | 容器嗅探 / TitleList / XML / WUD-WUX 读取 |
| ⭐ **JNUSLib** | Java | GPL-3.0 | **唯一把 NUS/WUD/WUX/分卷/WUMAD/Woomy 六形态统一建模**的库（88 类） |
| **CDecrypt** | C | GPLv3 | NUS 解密事实标准；`T_MAGIC_OFFSET` 技巧出处 |
| **decaf-emu** | C++ | GPL-3.0 | RPL/RPX 结构最完整定义 |
| **wudump / wud2app** | C | **MIT** ✅ | **许可最安全**；443 行的 wud2app 是最小可用 WUD→NUS |
| **bodgit/wud** | Go | — | **唯一明确文档化 WUX 对齐填充** |
| **loadiine_gx2** | C++ | — | ID6 规则唯一权威来源 |
| **wit / Wiimms ISO Tools** | C | GPL-2.0 | ❌ **完全不支持 Wii U**（官网逐字只提 Wii 与 GameCube；源码树 `wud\|wiiu\|wux` 命中 0）。⚠ WIT 的 `.wdf` 与 `.wud` 毫无关系 |
| **libwiiu** | C | — | ❌ 与识别无关（README 逐字：*"**Nothing** in this repository is useful to the general public."*，是浏览器漏洞利用） |

> ⚠ **【推断】许可风险**：关键参考几乎全是 GPL（rom-properties GPL-2.0+、JNUSLib/CDecrypt/decaf GPL-3.0），WudCompress **无 LICENSE 文件**（法律上保留所有权利）。
> **建议：只引用格式规范（偏移/magic/字段名 —— 事实性数据不受版权保护），实现灵感取自 MIT 的 wudump/wud2app、MPL-2.0 的 Cemu、MIT/Apache 的 `wux`/`sachet`，避免直接复制 GPL 代码块。**

**人日估算**（参考基准：rom-properties `.wud` 嗅探 59 行 C++；目录嗅探 31 行；Rust `wux` crate 全部 154 行）：

*纯识别层（零密钥）*：

| 模块 | Rust 行数 | 人日 |
|---|---|---|
| 容器嗅探（WUD/WUX/RPX/ELF/WUHB/**WUA 尾部魔数**） | ~150 | 1.0 |
| 分卷 WUD 识别与拼接读取器（两套命名） | ~120 | 0.5 |
| WUX 透明解压读取器 | ~160 | 0.5（用 `wux` crate 则 **0.2**） |
| TMD/Ticket 解析（binrw 声明式） | ~200 | 1.0 |
| NUS 目录 + 两套命名约定 | ~150 | 1.0 |
| Loadiine 目录 + 三个 XML（注意根元素 `<menu>`）+ ID6 推导 | ~250 | 1.5 |
| RPX/RPL 嗅探 + FILEINFO 定位 | ~150 | 0.5 |
| 哈希流水线（**25 GB 流式并行分块**） | ~150 | 1.0 |
| DAT 索引与匹配（`datary` + 四套键） | ~300 | 1.5 |
| 测试与 fixture（**拿不到真盘，需构造合成头部**） | ~400 | 1.5 |
| **小计** | **~1800 行** | **≈10 人日** |

*可选解密层*：WUD 分区 TOC 解密 ~300 行/1.5 人日；FST 解析与遍历 ~500 行/2.0；H3 哈希块读取 ~400 行/2.0；`.app` 内容解密 ~200 行/1.0 —— **小计 ~1400 行 / ≈6.5 人日**（Rust 生态零实现）。

**总计：纯识别 ≈10 人日 / 1800 行；含完整解密 ≈16.5 人日 / 3200 行。**

> ⭐ **强烈建议只做识别层**：零密钥、零 AES 依赖、覆盖全部形态，且已能对接 Redump(541)、No-Intro(3682 title)、libretro/GameTDB(2871)、wiiubrew(4951 title ID) 四库。**解密层是唯一引入 GPL 传染与密钥合规问题的部分。**
> 若只要「最小可用」（形态判定 + WUD 明文头 + TMD Title ID + `size` 预筛），可压缩到 **≈3 人日**；上表的 10 人日是含分卷、WUX 解压、完整 DAT 索引与测试的工程化版本。

#### 2.2.12 本节的「未找到权威来源」

1. **wiiubrew 上不存在 WUD / WUX / Loadiine 的格式页**（全站 340 页穷举确认）。三者的权威描述只能来自实现源码。
2. `"RPL_CRCS"` / `"RPL_FILEINFO"` **字符串常量**：Cemu 与 decaf 中均不存在，只有宏名。
3. **GameTDB 站点本身不可达**（TLS 握手失败），其准确条目数无法核实；存在性由 rom-properties 与 libretro 源码引用间接证实。
4. **WUD 明文头 `0x00B`–`0x015` 各字段语义**：rom-properties 是唯一来源，且自带两处 `TODO`。
5. **WUX `flags` 的"正确"偏移**：原始 C 结构（`0x18`）与 JNUSLib（`0x0C`）分歧；因字段未使用且恒 0，**无法也不必判定 —— 跳过它**。
6. **Cemu 源码中不存在盘体固定大小常量**：只有 `DISC_SECTOR_SIZE = 0x8000`，盘大小取文件实际字节数。该常量由 JNUSLib / wudump / bodgit-wud / Redump / No-Intro 五方提供。
7. **`title.cert` 解析**：Cemu 里是 `// todo - parse certificates`，未实现。

---

### 2.3 Xbox 360

> ⭐ **本节最关键的一条结论：XEX 头部完全不加密，零售 `default.xex` 的 TitleID 无需任何密钥即可读出。** 被加密/压缩的只是 `header_size` 之后的 PE 映像。这让 Xbox 360 成为三个现代平台里**识别成本最低**的一个。
> ⭐ **第二关键：Xbox 360 是三个现代平台里唯一有 Redump 光盘 DAT 的**（3707 条，整 `.iso` 的 CRC32/MD5/SHA-1），因此也是唯一能做"稳定整文件哈希"的。

#### 2.3.1 XEX 文件格式

**Magic 变体**（emoose/idaxex `formats/xex.hpp` L11-17 逐字）：
```c
#define MAGIC_XEX0   0x58455830  // 'XEX0'
#define MAGIC_XEX3F  0x5845583F  // 'XEX?'
#define MAGIC_XEX2D  0x5845582D  // 'XEX-'
#define MAGIC_XEX25  0x58455825  // 'XEX%'
#define MAGIC_XEX1   0x58455831  // 'XEX1'
#define MAGIC_XEX2   0x58455832  // 'XEX2'
```
同文件 L244-250 给出各自最低内核版本：`XEX0`=1332、`XEX?`=1529、`XEX-`=1640、`XEX%`=1746、`XEX1`=1838、`XEX2`=1861。

**头长度因版本而异**（idaxex `xex_structs.hpp` + `xex.cpp` L45-86）：

| Magic | 头长度 | 布局 |
|---|---|---|
| `XEX2` / `XEX1` / `XEX%` / `XEX-` | **`0x18`** | 同一布局（见下表） |
| `XEX?` | `0x1C` | Magic / ModuleFlags / SizeOfHeaders / SizeOfDiscardableHeaders / LoadAddress / ImageSize / HeaderDirectoryEntryCount |
| `XEX0` | `0x14` | Magic / SizeOfHeaders / LoadAddress / ImageSize / HeaderDirectoryEntryCount |

idaxex 读取时把 XEX0/XEX? 转换成 XEX2 布局，用 `dirHeaderOffset` 记录各自的目录起始位置。
**Xenia 只认 XEX1/XEX2**（`src/xenia/cpu/xex_module.h` L28-29）。**零售游戏基本都是 XEX2。**

**XEX2 主头（`0x18` = 24 字节，全大端 BE）**
Xenia `src/xenia/kernel/util/xex2_info.h` L548-557 逐字：
```c
struct xex2_header {
  xe::be<uint32_t> magic;                  // 0x0 'XEX2'
  xe::be<xex2_module_flags> module_flags;  // 0x4
  xe::be<uint32_t> header_size;            // 0x8
  xe::be<uint32_t> reserved;               // 0xC
  xe::be<uint32_t> security_offset;        // 0x10
  xe::be<uint32_t> header_count;           // 0x14
  xex2_opt_header headers[1];              // 0x18
};
```

| 偏移 | 长度 | 字段 | 说明 | 可靠性 |
|---|---|---|---|---|
| `0x00` | 4 | **Magic** | `'XEX2'` 等 6 种 | **极高** |
| `0x04` | 4 | Module Flags | `0x01` Title Module / `0x02` Exports To Title / `0x04` System Debugger / `0x08` DLL Module / `0x10` Module Patch / `0x20` Patch Full / `0x40` Patch Delta / `0x80` User Mode | 中 |
| `0x08` | 4 | **PE data offset** | 三个来源三种叫法、同一字段：free60 叫 "PE data offset"、Xenia 叫 `header_size`、idaxex 叫 `SizeOfHeaders`。语义 = **头部区域总长度 = basefile(PE) 在文件中的起始偏移** | 结构必需 |
| `0x0C` | 4 | Reserved | idaxex 叫 `SizeOfDiscardableHeaders` | — |
| `0x10` | 4 | **Security Info Offset** | 指向 Security Info（`0x184` 字节） | 结构必需 |
| `0x14` | 4 | **Optional Header Count** | 目录项数 | 结构必需 |
| `0x18` | 8×N | 可选头目录 | | |

free60 XEX 页的表格与之逐项一致：`0x0 ascii "XEX2" magic / 0x4 Flags / 0x8 unsigned int "PE data offset" / 0xC Reserved / 0x10 "Security Info Offset" / 0x14 "Optional Header Count"`。

**可选头目录项（每项 8 字节）**，Xenia L539-546：
```c
struct xex2_opt_header {
  xe::be<uint32_t> key;                                        // 0x0
  union { xe::be<uint32_t> value; xe::be<uint32_t> offset; };  // 0x4
};
```

⭐ **`key` 的编码规则** —— idaxex `xex_headerids.hpp` L3-9 给出生成式定义（最权威）：
```c
#define XEX_HEADER_STRUCT(key, struct)    (((key) << 8) | (sizeof (struct) >> 2))
#define XEX_HEADER_FIXED_SIZE(key, size)  (((key) << 8) | ((size) >> 2))
#define XEX_HEADER_ULONG(key)             (((key) << 8) | 1)
#define XEX_HEADER_FLAG(key)              ((key) << 8)
#define XEX_HEADER_SIZEDSTRUCT(key)       (((key) << 8) | 0xFF)
```
即 **`key = (ID << 8) | (数据字节数 / 4)`**。

**解码规则** —— free60 XEX 页逐字：
> "To handle the data you would first check to see what its size is, to do this you need to AND the Header ID by 0xFF. If `ID & 0xFF == 0x01` then the Header Data field is used to store the headers data, otherwise it's used to store the data's offset. if `ID & 0xFF == 0xFF` then the Header's data will contain its size. if `ID & 0xFF == (Anything else)` the value of this is the size of the entry in number of DWORDS (times by 4 to get real size)"

Xenia 实现 `xex_module.cc` L69-101：
```c
switch (key & 0xFF) {
  case 0x00: *(uint32_t*)out_ptr = opt_header.value; break;           // 值即数据
  case 0x01: *out_ptr = &opt_header.value; break;                     // 值即数据
  default:   *out_ptr = uintptr_t(header) + opt_header.offset; break; // 值是偏移
}
```
> ⚠ `default` 分支的 `offset` 是**相对 XEX 文件头起点**，不是相对目录项。
> 自洽验证：`0x00040006` 低字节 `0x06` → 6 DWORD = `0x18` 字节 = execution info 的 `sizeof` ✓

#### 2.3.2 ⭐ XEX_HEADER_EXECUTION_INFO = `0x00040006` —— TitleID 在这里

四源一致：Xenia `xex2_info.h` L300 `XEX_HEADER_EXECUTION_INFO = 0x00040006`；free60 XEX 页 `0x40006 | Execution ID`；idaxex `xex_headerids.hpp` L59 `XEX_HEADER_EXECUTION_ID XEX_HEADER_STRUCT(0x0400, xex_opt::XexExecutionId)`；iso2god-rs `src/executable/xex.rs` `ExecutionId = 0x_00_04_00_06`。

Xenia `xex2_info.h` L479-500 逐字（行尾偏移注释是原文自带的）：
```c
struct xex2_opt_execution_info {
  xe::be<uint32_t> media_id;            // 0x0
  xe::be<uint32_t> version_value;       // 0x4
  xe::be<uint32_t> base_version_value;  // 0x8
  xe::be<uint32_t> title_id;            // 0xC
  uint8_t platform;                     // 0x10
  uint8_t executable_table;             // 0x11
  uint8_t disc_number;                  // 0x12
  uint8_t disc_count;                   // 0x13
  xe::be<uint32_t> savegame_id;         // 0x14
};
static_assert_size(xex2_opt_execution_info, 0x18);
```

| 结构体内偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| `+0x00` | 4 | **Media ID** | 光盘/内容媒体 ID | **高** |
| `+0x04` | 4 | Version | 位域 `major:4 / minor:4 / build:16 / qfe:8` | **高** |
| `+0x08` | 4 | Base Version | 同上位域 | 中 |
| **`+0x0C`** | **4** | **Title ID** | **Xbox 360 的主识别键** | **极高** |
| `+0x10` | 1 | Platform | free60：Xbox 360 = `2` / PC = `4` | 中 |
| `+0x11` | 1 | Executable Table / Type | | 低 |
| `+0x12` | 1 | **Disc Number** | 多碟游戏的碟号 | **高** |
| `+0x13` | 1 | **Disc Count** | 总碟数 | **高** |
| `+0x14` | 4 | Savegame ID | | 低 |
| **合计** | **`0x18`** | | | |

**TitleID 的内部结构** —— idaxex `formats/xex_optheaders.hpp` L52-64 逐字：
```c
struct XexExecutionId {
  xe::be<uint32_t> MediaID;      // 0x0 sz:0x4
  xex::Version Version;          // 0x4 sz:0x4
  xex::Version BaseVersion;      // 0x8 sz:0x4
  union {
    xe::be<uint32_t> TitleID;    // 0xC sz:0x4
#ifdef _MSC_VER
    struct {
      xe::be<uint16_t> PublisherID; // 0xC sz:0x2
      xe::be<uint16_t> GameID;      // 0xE sz:0x2
    };
#endif
  };
  uint8_t Platform;              // 0x10
  uint8_t ExecutableType;        // 0x11
  uint8_t DiscNum;               // 0x12
  uint8_t DiscsInSet;            // 0x13
  xe::be<uint32_t> SaveGameID;   // 0x14
};
static_assert(sizeof(XexExecutionId) == 0x18, "xex_opt::XexExecutionId");
```
→ **`PublisherID` = BE u16 @ `+0x0C`（高 2 字节），`GameID` = BE u16 @ `+0x0E`（低 2 字节）**。

**关于"2 个 ASCII 字符的发行商代号"**：
【事实】Redump wiki 对 **XeMID/DMI 字符串**有明确定义（逐字）：
> "Open the DMI.bin file with Windows' Notepad app and you'll find a string like `AV202202E0X11` ... `AV2022` is the disc serial (AV-2022). **AV is the two-ASCII-character publisher identifier (AV=Activision)** `2` is the Platform identifier. `2` indicates Xbox 360. `022` is the game ID ... **`X` indicates a XGD2 disc while `F` indicates a XGD3 disc.** Finally, the last two bytes tell you that it's Disc 1 of 1."
> —— <https://wiki.redump.info/Stock_Xbox360_drives>

【推断】XEX TitleID 的 `PublisherID` 沿用同一套 ASCII 代号（如 `0x4D53` = `'MS'` = Microsoft；旁证：free60 GPD 页举例文件名 `4D5307E6.gpd`、Xbox Live URL `.../content/4d530914/thumbnails/`）。
> **【未找到权威来源】** 把 XEX TitleID 的 PublisherID 明文定义为 ASCII 的一手文档，以及权威发行商代号对照表。**不要用它做强校验。**
> 💡 附带收获：**XeMID 第 11 位 `X`/`F` 直接标明 XGD2/XGD3** —— 无需读扇区就能判定世代的字符串特征。

#### 2.3.3 ⭐⭐ 加密：头部完全不加密

**【事实】XEX 头部完全不加密。零售 `default.xex` 的 TitleID 无需任何密钥即可读取。** 四条独立证据：

**(1)** Xenia `xex_module.cc` L913-915，读头是纯 memcpy，无解密：
```c
// Read in XEX headers
xex_header_mem_.resize(src_header->header_size);
std::memcpy(xex_header_mem_.data(), src_header, src_header->header_size);
```

**(2)** Xenia `src/xenia/kernel/user_module.cc` L33-49，`title_id()` 直接遍历明文目录，零密钥：
```c
uint32_t UserModule::title_id() const {
  auto header = xex_header();
  for (uint32_t i = 0; i < header->header_count; i++) {
    auto& opt_header = header->headers[i];
    if (opt_header.key == XEX_HEADER_EXECUTION_INFO) {
      auto opt_header_ptr = reinterpret_cast<const uint8_t*>(header) + opt_header.offset;
      auto opt_exec_info = reinterpret_cast<const xex2_opt_execution_info*>(opt_header_ptr);
      return static_cast<uint32_t>(opt_exec_info->title_id);
    }
  }
  return 0;
}
```

**(3)** `aes_decrypt_buffer()` 在 Xenia 中**仅在 `ReadImage()` 内**调用（L489-492），对象是 `header_size` 之后的 basefile。

**(4)** `iso2god-rs` 是能从零售 ISO 读出 TitleID 的生产工具，其 `Cargo.toml` **无任何加密依赖**（只有 sha1 用于哈希），`src/executable/xex.rs` 全程纯 `read_u32::<BE>()`。

**被加密/压缩的是什么**：`header_size` 之后的 basefile（PE 映像），由 `XEX_HEADER_FILE_FORMAT_INFO (0x000003FF)` 描述：
- `encryption_type`：`XEX_ENCRYPTION_NONE=0` / `XEX_ENCRYPTION_NORMAL=1`（AES-128-CBC，IV 全零）
- `compression_type`：`NONE=0` / `BASIC=1`（零块 RLE）/ `NORMAL=2`（LZX）/ `DELTA=3`
- 会话密钥 = 用 retail 或 devkit 密钥解密 `security_info.aes_key[0x10]`。Xenia 先试 retail 再试 devkit。

| | 内容 |
|---|---|
| **无需密钥可读** | TitleID / MediaID / Version / BaseVersion / DiscNumber / DiscCount / SavegameID / Platform / 区域码 / allowed media / Game Ratings / Original PE Name / RESOURCE_INFO 的**资源名**（通常就是 TitleID 的 8 位十六进制串） |
| **需要密钥** | **游戏显示名称**（在 XDBF/SPA 资源里，而资源 address 是映像虚拟地址），需 AES 解密 + LZX 解压 + XDBF 解析 |

#### 2.3.4 其它有识别价值的可选头

Xenia L278-309 与 free60 XEX 页一致：

| ID | 名称 | 内容 |
|---|---|---|
| `0x000002FF` | RESOURCE_INFO | u32 size + `{char name[8]; be32 address; be32 size}[]` ← **资源名明文可读** |
| `0x000003FF` | FILE_FORMAT_INFO | 加密/压缩描述 |
| `0x000183FF` | ORIGINAL_PE_NAME | u32 size + 字符串（**明文**，常含项目代号） |
| `0x00030000` | SYSTEM_FLAGS | 内联 u32 |
| **`0x00040006`** | **EXECUTION_INFO** | `0x18` 定长 ⭐ |
| `0x00040310` | GAME_RATINGS | **64 字节**定长 |
| `0x000405FF` | XBOX360_LOGO | sized blob |
| `0x000406FF` | MULTIDISC_MEDIA_IDS | sized blob（多碟游戏全部 MediaID） |
| `0x000407FF` | ALTERNATE_TITLE_IDS | u32 size + u32[] |

> ⚠ **GAME_RATINGS 长度是 64 字节**（idaxex `XEX_HEADER_FIXED_SIZE(0x0403, 64)`，且 key 低字节 `0x10` = 16 DWORD 印证）。Xenia 的 `xex2_game_ratings_t` 是 12×BE u32 = 48 字节（只覆盖前 48）；free60 列的是 12×u8 = 12 字节。**三者不一致，以 64 字节为准。**

**Security Info**（位于 `header[0x10]` 所指文件偏移，`sizeof == 0x184`，Xenia L570-588）：

| 偏移 | 长度 | 字段 |
|---|---|---|
| `+0x000` / `+0x004` | 4 / 4 | header_size / image_size |
| `+0x008` | `0x100` | rsa_signature |
| `+0x10C` / `+0x110` | 4 / 4 | image_flags / load_address |
| `+0x114` | `0x14` | section_digest |
| `+0x140` | `0x10` | **xgd2_media_id**（16 字节，与 exec info 的 4 字节 MediaID 不同） |
| `+0x150` | `0x10` | **aes_key**（加密的会话密钥） |
| `+0x164` | `0x14` | header_digest |
| **`+0x178`** | 4 | **region**（区域码） |
| `+0x17C` / `+0x180` | 4 / 4 | allowed_media_types / page_descriptor_count |
| `+0x184` | `0x18`×N | page_descriptors |

`region` 取值（Xenia L102-110）：`NTSCU=0x000000FF`、`NTSCJ=0x0000FF00`、**`NTSCJ_JAPAN=0x00000100`**、**`NTSCJ_CHINA=0x00000200`**、`PAL=0x00FF0000`、`PAL_AU_NZ=0x00010000`、`OTHER=0xFF000000`、`ALL=0xFFFFFFFF`。
> `NTSCJ_CHINA=0x200` 是 **XEX 层面唯一的「中国」标识位**。

> ⚠ free60 XEX 页底部那段 `<nowiki>` 旧笔记把 `0x17C` 标为 `SHA Hash[0x14]`，与 `0x180 ImageDataCount` 区间重叠、自相矛盾。**以 Xenia + idaxex（`sizeof(xex2::SecurityInfo)==0x184`）为准。**
> ⚠ **XEX1/XEX%/XEX- 的 SecurityInfo 布局完全不同**：idaxex 给出 `sizeof(xex1::SecurityInfo)==0x168`、`xex25==0x154`、`xex2d==0x140`。**跨版本必须分支。**

#### 2.3.5 光盘：XDVDFS / GDF

**Magic** = `"MICROSOFT*XBOX*MEDIA"`，20 字节 ASCII，位于**游戏分区起点 + 32 × 2048 = `+0x10000`**（分区内第 32 扇区）。三源：
- extract-xiso：`#define XISO_HEADER_OFFSET 0x10000`
- Xenia `disc_image_device.cc`：`VerifyMagic(state, state->game_offset + (32 * kXESectorSize))`，`kXESectorSize = 2_KiB`
- `xdvdfs` crate：`reader.seek(SeekFrom::Start(0x20 * SECTOR_SIZE + iso_type.root_offset()))`

free60 GDFX 页：*"The 32nd segment seems to be a descriptor."*；xboxdevwiki XDVDFS 页：*"The Volume descriptor is located at Sector #32 and #33 of the game partition."*
扇区大小恒为 2048（extract-xiso `#define XISO_SECTOR_SIZE 2048`）。

⭐ **游戏分区基址 —— 5 源交叉验证，数值完全一致**：

| 类型 | 偏移 | 十进制 |
|---|---|---|
| XISO / XSF（裸镜像） | `0x00000000` | 0 |
| **XGD3** | **`0x02080000`** | 34 078 720 |
| **XGD2** | **`0x0FD90000`** | 265 879 552 |
| XGD1（初代 Xbox） | `0x18300000` | 405 798 912 |
| （Xenia 另探测两个） | `0x0000FB20` / `0x00020600` | **【未找到权威来源】用途** |

来源逐条：
```c
// (1) extract-xiso（XboxDev 官方）extract-xiso.c L447-449
#define GLOBAL_LSEEK_OFFSET       0x0FD90000ul
#define XGD3_LSEEK_OFFSET         0x02080000ul
#define XGD1_LSEEK_OFFSET         0x18300000ul

// (2) Xenia src/xenia/vfs/devices/disc_image_device.cc L69-72
static const size_t likely_offsets[] = {
    0x00000000, 0x0000FB20, 0x00020600, 0x02080000, 0x0FD90000,
};

// (4) xdvdfs crate xdvdfs-core/src/blockdev/offset.rs
pub enum XDVDFSOffsets {
    #[default] XISO = 0,
    XGD1 = 405798912,   // 0x18300000
    XGD2 = 265879552,   // 0x0FD90000
    XGD3 = 34078720,    // 0x02080000
}

// (5) abgx360 src/abgx360.c 注释
// proper backups should have the game partition starting at 0xFD90000 (XGD2) or 0x2080000 (XGD3)
```
（(3) iso2god-rs `src/iso/iso_type.rs` 同样：`Xgd3 => 0x2080000, Xgd2 => 0xfd90000, Xgd1 => 0x18300000, Xsf => 0`）

> **【推断】** 做通用探测器应取并集（6 个偏移）：Xenia 缺 `0x18300000`，其余三家缺 `0xFB20`/`0x20600`。

**卷描述符（扇区 32，2048 字节）**，xboxdevwiki XDVDFS + free60 GDFX + Xenia `Verify()` 三源一致：

| 偏移 | 长度 | 类型 | 字段 |
|---|---|---|---|
| `0x000` | 20 | ascii | `"MICROSOFT*XBOX*MEDIA"` |
| `0x014` | 4 | **LE u32** | Root Directory Table Sector（× `0x800` 得偏移） |
| `0x018` | 4 | **LE u32** | Root Directory Table Size（字节，`0x800` 的倍数） |
| `0x01C` | 8 | FILETIME | 创建时间 |
| `0x7EC` | 20 | ascii | `"MICROSOFT*XBOX*MEDIA"`（重复，校验用） |

扇区 33 为 `"XBOX_DVD_LAYOUT_TOOL_SIG"`（xboxdevwiki，该页标注 FIXME，版本字段未文档化）。

> ⚠⚠ **GDF 的整数是小端序**：extract-xiso 读完调用 `little32()`；Xenia 用 `xe::load<uint32_t>`（宿主小端）。**这与 XEX/STFS 的大端相反，是最容易踩的坑。**

**目录项（GDF Dirent）**，free60 GDFX.md 与 Xenia `ReadEntry()` 一致：

| 偏移 | 长度 | 字段 |
|---|---|---|
| `0x00` / `0x02` | 2 / 2 | 左 / 右子树偏移（以 4 字节为单位） |
| `0x04` / `0x08` | 4 / 4 | 起始扇区 / 文件大小 |
| `0x0C` | 1 | 属性（`READONLY=0x01 HIDDEN=0x02 SYSTEM=0x04 DIRECTORY=0x10 ARCHIVE=0x20 DEVICE=0x40 NORMAL=0x80 TEMPORARY=0x100`） |
| `0x0D` | 1 | 文件名长度 |
| `0x0E` | n | 文件名 |

遍历顺序 左→自身→右；项按 4 字节对齐。

**ISO9660 与视频分区**：
【事实】**XGD 用 XDVDFS，不是 ISO9660。** Redump `Microsoft Xbox 360 Guide` 逐字：
> "Xbox Game Discs (XGDs) are **multi-session DVDs where the first session is a DVD-Video partition**. XGDs also use Security Sectors in an attempt to prevent piracy. The XGDs for the Xbox 360 are second and third iterations of the technology with enhanced security measures called XGD2 and XGD3."

【事实】**视频分区在扇区 16 有 ISO9660 PVD**。abgx360：`if (memcmp(buffer+1, "CD001", 5) == 0) { ... checkvideo(...) }`。Redump 提交要求逐字：
> "**Primary Volume Descriptor (PVD)**: Open the iso file with IsoBuster. Right click on Track 1 > Sector View > type **"16"** in at the top, then copy the data at 0320 - 0370 (full six rows)"

→ **ISO9660 PVD 在完整 ISO 的扇区 16 = 偏移 `0x8000`（属视频分区）；游戏分区用 XDVDFS，magic 在游戏分区 `+0x10000`。两者并存但互不相干。**

【推断，四条事实支撑】**Redump 保存的是完整物理盘镜像（含视频分区），不是剥离后的精简镜像**：
(a) Redump 记录的 Security Sector Ranges 落在视频分区内（Alan Wake `108976–113071`，而 XGD2 视频分区为 LBA 0–129 823）；若剥离则为负数
(b) PVD 从 sector 16 读（视频分区的）
(c) redumper 源码注释 `// XGD physical sector count is only for video partition`、`// lock kreon drive before L1 video reading`
(d) Redump 扇区数比 scene "full" 镜像多出 L1 尾部视频区（abgx360 `assume_start_offset_of_L1_padding`：XGD2 = `0x01C3610000` = 3 697 696 扇区）

#### 2.3.6 STFS / XContent（XBLA / GOD / DLC 的统一容器）

**XContentHeader（`0x344` 字节）**，Xenia `src/xenia/vfs/devices/stfs_xbox.h` L470-483 逐字：
```c
struct XContentHeader {
  be<XContentPackageType> magic;
  uint8_t signature[0x228];
  XContentLicense licenses[0x10];
  uint8_t content_id[0x14];
  be<uint32_t> header_size;
};
static_assert_size(XContentHeader, 0x344);

enum class XContentPackageType : uint32_t {
  kCon  = 0x434F4E20,    // 'CON '（含尾部空格）
  kPirs = 0x50495253,    // 'PIRS'
  kLive = 0x4C495645,    // 'LIVE'
};
```

| 偏移 | 长度 | 字段 | 可靠性 |
|---|---|---|---|
| **`0x000`** | 4 | **Magic**：`'CON '`=`0x434F4E20` / `'LIVE'`=`0x4C495645` / `'PIRS'`=`0x50495253` | **极高（magic）** |
| `0x004` | `0x228` | 签名（CON=主机证书签名，LIVE/PIRS=MS RSA 签名） | — |
| `0x22C` | `0x100` | `licenses[0x10]`，每项 `0x10`：`licensee_id`(BE u64) + `license_bits`(u32) + `license_flags`(u32) | 低 |
| `0x32C` | `0x14` | Content ID / Header SHA-1 | 高 |
| `0x340` | 4 | header_size | — |

free60 STFS 页逐字：*"0x032C | 0x14 | byte[] | Header SHA1 Hash (from 0x344 to first hash table)"*
iso2god `src/god/con_header.rs` L117-118 给出可执行定义：
```rust
let digest: [u8; 20] = Sha1::digest(&self.buffer[0x0344..(0x0344 + 0xacbc)]).into();
self.write_bytes(0x032c, &digest);
```
（`0x344 + 0xACBC = 0xB000`，正是 STFS 头总长，与 "to first hash table" 吻合）

⭐ **XContentMetadata（`0x93D6` 字节，起于 `0x344`）—— 三源一致**
（Xenia `stfs_xbox.h` L280-468 / free60 STFS.md / iso2god `con_header.rs`）

| 绝对偏移 | 长度 | 字段 | 可靠性 |
|---|---|---|---|
| **`0x0344`** | 4 | **Content Type**（BE u32） | **极高** |
| `0x0348` | 4 | Metadata Version（1 或 2） | 结构必需 |
| `0x034C` | 8 | Content Size（BE u64） | 中 |
| **`0x0354`** | 4 | **Media ID** ┐ | **高** |
| `0x0358` | 4 | Version │ 这 `0x18` 字节就是 XEX 的 | 高 |
| `0x035C` | 4 | Base Version │ `xex2_opt_execution_info` | 中 |
| **`0x0360`** | 4 | **Title ID** │ **结构体原样内嵌！** | **极高** |
| `0x0364` | 1 | Platform │ Xenia 直接写 | 中 |
| `0x0365` | 1 | Executable Type │ `xex2_opt_execution_info execution_info;` | 低 |
| `0x0366` | 1 | Disc Number │ | 高 |
| `0x0367` | 1 | Disc In Set │ | 高 |
| `0x0368` | 4 | Savegame ID ┘ | 低 |
| `0x036C` / `0x0371` | 5 / 8 | Console ID / Profile ID | 低（**含个人信息**） |
| `0x0379` | `0x24` | Volume Descriptor（STFS 或 SVOD 联合体） | 结构必需 |
| `0x039D` / `0x03A1` | 4 / 8 | Data File Count / Data File Combined Size | 高 |
| **`0x03A9`** | 4 | **Volume Type**（STFS=0，SVOD=1） | **极高（分流判据）** |
| `0x03B5` | 4 | Category | 低 |
| `0x03FD` | `0x14` | Device ID | 低 |
| **`0x0411`** | `0x900` | **Display Name** —— **9 语言槽 × `0x100`**（128 个 **UTF-16 大端** 码元） | **极高（人可读名）** |
| `0x0D11` | `0x900` | Description（同上结构） | 中 |
| `0x1611` | `0x80` | Publisher Name（64 × UTF-16BE） | 中 |
| **`0x1691`** | `0x80` | **Title Name**（64 × UTF-16BE） | **高** |
| `0x1712` / `0x1716` | 4 / 4 | Thumbnail Size / Title Thumbnail Size | — |
| `0x171A` | V1 `0x4000` / V2 `0x3D00` | Thumbnail（PNG） | — |
| `0x541A` | `0x300` | （仅 V2）Display Name 追加 3 语言 | — |
| `0x571A` | V1 `0x4000` / V2 `0x3D00` | Title Thumbnail（PNG） | — |
| **结束** | `0x344 + 0x93D6 = 0x971A` | Xenia：`static_assert_size(StfsHeader, 0x971A)` | |

iso2god `con_header.rs` 的写入调用（第三方独立验证，逐字）：
```rust
self.write_u32_be(0x0344, content_type as u32);
self.write_u32_be(0x0354, exe_info.media_id);
self.write_u32_be(0x0360, exe_info.title_id);
self.write_u8   (0x0364, exe_info.platform);
self.write_u8   (0x0365, exe_info.executable_type);
self.write_u8   (0x0366, exe_info.disc_number);
self.write_u8   (0x0367, exe_info.disc_count);
self.write_utf16_be(0x0411, game_title);
self.write_utf16_be(0x1691, game_title);
```

> ⚠⚠ **free60 STFS 页在 Display Name 上有两处错误，勿照抄**：
> **(a)** free60 写 "UTF-8 string" —— **错**。Xenia 定义为 `union { be<uint16_t> uint[9][128]; char16_t chars[9][128]; } display_name_raw;` 并返回 `std::u16string`；iso2god 用 `write_utf16_be`。**实际是 UTF-16 大端。**
> **(b)** free60 写 "0x900 (each 0x80 = different locale)" —— **自相矛盾**（`0x900/0x80 = 18`）。实际是 **9 个语言槽，每槽 `0x100` 字节**。

⭐ **Content Type 取值**（Xenia `src/xenia/xbox.h` L348-383）：

| 值 | 含义 | 【实测】No-Intro (Digital) 中的 rom 数 |
|---|---|---|
| `0x00000001` | Saved Game | 1 066 |
| `0x00000002` | **Marketplace Content**（DLC 主力） | 11 916 |
| `0x00000003` | Publisher | — |
| `0x00001000` | Xbox 360 Title | — |
| `0x00004000` | Installed Game | — |
| `0x00005000` | Xbox Title / Xbox Original | — |
| **`0x00007000`** | **Games on Demand (GOD)** | **213** |
| `0x00009000` | Avatar Item | 20 447 |
| `0x00010000` | Profile | — |
| `0x00020000` | Gamer Picture | 2 |
| `0x00030000` | Theme | 19 |
| `0x00080000` | Game Demo | 6 |
| `0x000A0000` | Game Title | — |
| `0x000B0000` | Installer（Title Update） | 243 |
| `0x000C0000` | Game Trailer | 11 |
| **`0x000D0000`** | **Arcade Title (XBLA)** | **732** |
| `0x000E0000` | XNA | — |
| `0x02000000` | Community Game | — |

> ⭐ **XBLA 与 GOD 是完全相同的 STFS 容器格式，只有 `0x344` 处的 Content Type 不同。**

**Volume Descriptor（`0x379`，两种联合体）**：

*STFS 版*（`StfsVolumeDescriptor`, `0x24`）：`0x379` descriptor_length(应=0x24) / `0x37A` version / `0x37B` flags(bit0 read_only_format、bit1 root_active_index…) / `0x37C` file_table_block_count(2) / `0x37E` file_table_block_number(**LE u24**) / `0x381` top_hash_table_hash(0x14) / `0x395` total_block_count(BE) / `0x399` free_block_count(BE)

*SVOD 版*（GOD 用这个，`SvodDeviceDescriptor`, `0x24`）：`0x379` descriptor_length / `0x37A` block_cache_element_count / `0x37B` worker_thread_processor / `0x37C` worker_thread_priority / `0x37D` first_fragment_hash_entry(0x14) / `0x391` features(**bit6 = enhanced_gdf_layout**) / `0x392` num_data_blocks(**LE u24**) / `0x395` start_data_block(**LE u24**) / `0x398` reserved(5)

> iso2god 写 `0x037D` 的 mht_hash、并把 `0x0391` 归零，正落在 SVOD 版的 `first_fragment_hash_entry` 与 `features` 上 —— 交叉验证成立。

**STFS 块寻址**（提取包内文件时需要），Xenia `stfs_container_device.h` L85 / `.cc` L641-659：
```c
const uint32_t kBlocksPerHashLevel[3] = {170, 28900, 4913000};
kBlockSize = 0x1000
blocks_per_hash_table_ = read_only_format ? 1 : 2

size_t BlockToOffsetSTFS(uint64_t block_index) const {
  uint64_t base = kBlocksPerHashLevel[0];
  uint64_t block = block_index;
  for (uint32_t i = 0; i < 3; i++) {
    block += ((block_index + base) / base) * blocks_per_hash_table_;
    if (block_index < base) break;
    base *= kBlocksPerHashLevel[0];
  }
  return xe::round_up(header_.header.header_size, kBlockSize) + (block << 12);
}
```

#### 2.3.7 磁盘上的"变体成型"形态

⭐ **GOD 的目录结构**，iso2god-rs `src/god/file_layout.rs` 逐字：
```rust
pub fn data_dir_path(&self) -> PathBuf {
    self.base_path.join(self.title_id_string())
        .join(self.content_type_string())
        .join(self.media_id_string() + ".data")
}
pub fn part_file_path(&'a self, part_index: u64) -> PathBuf {
    self.data_dir_path().join(format!("Data{:04}", part_index))
}
pub fn con_header_file_path(&self) -> PathBuf {
    self.base_path.join(self.title_id_string())
        .join(self.content_type_string()).join(self.media_id_string())
}
```
（`title_id_string` / `content_type_string` 均为 `format!("{:08X}")`）

```
<root>/
  <TitleID:08X>/                 例：5848081B
    <ContentType:08X>/           例：00007000
      <名>                       ← ★ 头文件，无扩展名（CON/LIVE 头 0xB000 字节 + 元数据）
      <名>.data/                 ← ★ 数据目录
          Data0000               ← 十进制 4 位零填充
          Data0001
          ...
```

【实测】No-Intro `Microsoft - Xbox 360 (Digital)` 的真实路径样例，完全吻合：
```
354507D3\00007000\E148200A5FD18168718CA799BFA8F20635.data\Data0000
5848081B\00007000\43F98136D11B73BFCE3A.data\Data0000
5848081B\00007000\43F98136D11B73BFCE3A.data\Data0001
5841098F\000D0000\E42409217241BEFFDAE1EFBA10F7DA460283CFB358    ← XBLA 单文件
5841098F\00000002\CCB5AF46AFE1347322A89783FA671A6B481D019758    ← DLC 单文件
```

Xenia `stfs_container_device.cc` L92-119 逐字：
```c
// NOTE: data_file_count is 0 for STFS and 1 for SVOD
if (header_.metadata.data_file_count <= 1) { ... 单文件 ... }
// If the STFS package is multi-file, it is an SVOD system. We need to map
// the files in the .data folder and can discard the header.
auto data_fragment_path = host_path_;
data_fragment_path += ".data";
...
std::sort(fragment_files.begin(), fragment_files.end(),
          [](filesystem::FileInfo& left, filesystem::FileInfo& right) {
            return left.name < right.name;
          });
if (fragment_files.size() != header_.metadata.data_file_count) { 报错 }
```
→ **头文件路径追加 `".data"` 即分片目录**；目录内按文件名字典序排序；数量必须等于 `data_file_count`。

**分片尺寸常量**，iso2god `src/god/mod.rs`：
```rust
pub const BLOCKS_PER_PART: u64 = 0xa1c4;
pub const BLOCKS_PER_SUBPART: u64 = 0xcc;
pub const BLOCK_SIZE: u64 = 0x1000;
pub const SUBPARTS_PER_PART: u32 = 0xcb;
```
→ 单个 `Data####` ≈ `0xA1C4 × 0x1000` = 169 754 624 B ≈ **162 MiB**（含交织的 SHA-1 哈希表）。

> ⚠ **头文件名有两种形态**：
> - iso2god（自制转换）：头文件名 = **MediaID** 的 8 位十六进制
> - No-Intro DAT 里的真实商店 GOD：末段是 **20 个十六进制字符**（源自 Content ID）
> **【推断】** 微软商店下发的 GOD 用 Content ID 派生名，第三方转换工具用 MediaID。
> ⭐ **实现建议：探测时不要对末段文件名做格式假设。改为：找 `*.data` 目录，其「去掉 `.data` 后缀的同名兄弟文件」即头文件。**

**SVOD 三种内部布局**（Xenia `ReadSVOD()` L197-290），靠 magic 位置区分：

| 布局 | magic 位置 | svod_base_offset | 判定条件 |
|---|---|---|---|
| Enhanced GDF | `0x2000` | `0x0000` | `volume_descriptor.svod.features.enhanced_gdf_layout` 置位 |
| XSF（第三方转换） | `0x12000` | `0x10000` | `0x2000` 处有 `"XSF"` magic |
| 单文件 | `0xD000` | `0xB000` | `data_file_count == 1`（`0xB000` STFS 头 + `0x2000` 哈希表） |

⭐ **识别决策树（可直接实现）**：
```
读前 4 字节：
├─ "XEX2"/"XEX1"/"XEX%"/"XEX-"/"XEX?"/"XEX0" → 裸 XEX，直接解析可选头目录
├─ "CON "/"LIVE"/"PIRS" → STFS 系
│   ├─ 读 0x3A9 Volume Type
│   │   ├─ 0 (STFS) → 单文件包；看 0x344 判 XBLA(0x000D0000)/DLC(0x2)/存档(0x1)
│   │   └─ 1 (SVOD) → GOD；检查同级 <名>.data/ 目录
│   └─ 无论哪种，0x360=TitleID / 0x354=MediaID / 0x411=显示名(UTF-16BE) 均可直读
└─ 其它 → 试 6 个 XGD 偏移，在 offset+0x10000 处找 "MICROSOFT*XBOX*MEDIA"
    └─ 命中 → 走 GDF 目录树找 \default.xex → 按裸 XEX 解析
       （额外：可从 video−0x1000 直读 DMI 拿 XeMID 字符串；从 SS 的 layer0_end PSN 判 XGD 世代）
```

#### 2.3.8 DAT / 数据库覆盖

**Redump 官方 DAT**（<https://redump.info/datfile/XBOX360>）：
文件名 `Microsoft - Xbox 360 - Datfile (3707) (2026-08-30 14-20-37).dat`
- **3 707 个 `<game>`，3 716 个 `<rom>`** = **3 698 个 `.iso` + 9 个 `.bin` + 9 个 `.cue`**（bin/cue 为 CD-ROM 型光盘，如 Xbox 360 HD DVD Player Installation Disc）
- **零个 SS/PFI/DMI 条目**
- Logiqx DTD，只有 **CRC32/MD5/SHA-1，无 SHA-256**，无 TitleID、无 XeMID 字段

⭐ **ISO 尺寸 → 压制 wave 对照表**（本次调研把原先的【推断】升级为【事实】）：

| 字节数 | 扇区数 | 官方 DAT 条目 | 判定 | PFI CRC32 | wave |
|---|---|---|---|---|---|
| 8 738 846 720 | 4 267 015 | **787** | XGD3 | `26AF4C58` | XGD3 |
| 8 738 854 912 | 4 267 019 | 4 | XGD3v0 | `E1647069` | XGD3v0 |
| 7 838 695 424 | 3 827 488 | **1 570** | XGD2 晚期 | `05C6C409` | WAVE4-7 |
| 7 835 492 352 | 3 825 924 | 950 | XGD2 中期 | `A4CFB59C` | WAVE2 |
| 7 834 892 288 | 3 825 631 | 337 | XGD2 早期 | `739CEAB3` | WAVE1 |
| 其它 50 种 | — | 50 | 单层 DVD-R 开发盘 / Beta | — | — |

合计 XGD3 = **791**，XGD2 = **2 857**。

PFI CRC32 ↔ wave 对照来自 Redump wiki 逐字（<https://wiki.redump.info/Stock_Xbox360_drives> §"PFI Hash"）：
```
* WAVE1: 739CEAB3
* WAVE2: A4CFB59C
* WAVE4: 05C6C409
...
* XGD3v0: E1647069
* XGD3: 26AF4C58
```
抽样光盘页验证：`/disc/59573/` Perfect Dark Zero (Europe) 7 834 892 288 / XeMID `MS200308E0X11` / `739CEAB3`；`/disc/13756/` Alan Wake (World) 7 838 695 424 / `MS205301W0X11` / `05C6C409`；`/disc/113181/` Forza Motorsport 4 (USA) 8 738 846 720 / `MS232005A0EF22` / `26AF4C58`。
（注意 XeMID 第 11 位 `X`=XGD2 / `F`=XGD3，与尺寸吻合）

> **【事实】Xbox 360 没有固定 ISO 尺寸**，Redump wiki 对 360 只写 "Provide the size in bytes"；**初代 Xbox** 才有固定值，逐字：*"Xbox discs are always **7,825,162,240** bytes"*（= DiscImageCreator `#define XBOX_SIZE (3820880)` × 2048 ✓）。

⭐ **Layer break（三源一致）**：XGD1 = **1 913 776** / **XGD2 = 1 913 760** / **XGD3 = 2 133 520**
```c
// DiscImageCreator/execScsiCmdforDVD.cpp L35-38
#define XBOX_SIZE           (3820880)
#define XGD2_LAYER_BREAK    (1913760)
#define XGD3_LAYER_BREAK    (2133520)

// abgx360
if (video == 0xFD90000LL) layerbreak = 1913760; else layerbreak = 2133520;

// redumper dvd/xbox.ixx —— 用 SS 的 layer0_end_sector PSN 定义世代
const std::map<int32_t, uint32_t> XGD_VERSION_MAP = {
    { 2110383, 1 }, { 2110367, 2 }, { 2330127, 3 }
};
```
（配合 `psn_first = 0x30000 = 196608` 反推：2 110 367 + 1 − 196 608 = **1 913 760** ✓）
> **【未找到权威来源】** Redump wiki / 数据库本身从未文档化这两个数值（360 光盘页不显示 Layerbreak 字段）。数值来自工具链源码与 DAT 统计。
> 💡 **实现建议：用 SS 的 `layer0_end_sector` PSN 三值（2110383/2110367/2330127）判 XGD1/2/3，比试探分区偏移更确定。**

⭐ **SS / DMI / PFI**：三者均为 **2048 字节**（针对 Xbox 360 已确认）。DiscImageCreator README 逐字：
```
- _DMI.bin
  2048 bytes binary image of the "Disc Manufacturing Information" (DMI) in the DVD
- _PFI.bin
  2048 bytes binary image of the "Physical Format Information" (PFI) in the DVD
- _SS.bin
  2048 bytes binary image of the "Security Sector" (SS) in the xbox/xbox 360
```
含义：**SS** = Security Sector（XGD 防拷贝结构：Security Layer Descriptor、challenge-response、media id、game id、签名、security sector ranges）；**PFI** = Physical Format Information（DVD 标准物理层描述符）；**DMI** = Disc Manufacturing Information（含 **XMID/XeMID 字符串**、时间戳、media id）。

⭐ **三者在完整 ISO 中的位置**（abgx360，相对游戏分区起点 `video`）：

| 文件 | XGD2 偏移 | XGD3 偏移 |
|---|---|---|
| SS | `video − 0x800` | `video − 0x8800` |
| DMI | `video − 0x1000` | `video − 0x9000` |
| PFI | `video − 0x1800` | `video − 0x9800` |

→ **XGD2 的 SS 就在 ISO 的 `0x0FD8F800` 处，可直接从镜像提取，无需光驱。**
⚠ SCSI 原始响应是 **2052** 字节（2048 + 4 字节 ParameterListHeader），须剥头。redumper 原始件存为 `.physical` / `.manufacturer`，剥头后才是 `.pfi` / `.dmi`。

**SS/DMI/PFI 不进 DAT** —— Redump 把三者的 CRC32 记在光盘页 Comments，文件本身随提交上传内部归档。实例（`/disc/13756/` Alan Wake）：
```
XeMID: MS205301W0X11
DMI: 5219FA7F, 56F3CB78, 279F316B, AF2A56CB, FD24E237
PFI: 05C6C409
SS: 840B97D2, A758E492, 61A08961, D5919D7C, A10C225F, 5D0CB8E1, 94514F0F
```

> ⚠⚠ **Redump 的 SS 是"清洗过"的。** redumper `dvd/dvd_split.ixx` 逐字：
> ```cpp
> // clean the .security and write it to a .ss (if it doesn't exist)
> xbox::clean_security_sector(security);
> write_vector(ss_path, security);
> ```
> 清洗 = 把每次读取都会变的 challenge-response 角度值替换为固定值（`0x01, 0x5B, 0xB5, 0x10F`）。
> wiki 佐证逐字：*"SS dumps by OmniDrive can be repaired to match redump hashes when using MPF 3.7.0 or later, however it cannot produce RawSS for abgx360 submission (a non-redump project)."*
> **→ 要比对 Redump 记录的 SS CRC32，必须先做同样清洗，否则永远对不上。**

**No-Intro 的 4 个 X360 相关 DAT**（全部非光盘）：

| DAT | 版本 | 条目 |
|---|---|---|
| `Microsoft - Xbox 360 (Digital)` | 20260626 | **17 438** |
| `Microsoft - Xbox 360 (Development Kit Hard Drives)` | 20260509 | 126 |
| `Non-Redump - Microsoft - Xbox 360` | 20260525 | 33 |
| `Unofficial - Microsoft - Xbox 360 (Title Updates)` | 20220623（停更） | 1 409 |

(Digital) 构成（按名称统计）：Addon 8 325 / DLC 4 354 / XBLIG 3 431 / XBLA 737 / Title Update 456 / Avatar Item 81 / **GOD 仅 27** / Demo 7 / App 5。
> **【推断】GOD 覆盖极不完整（仅 27 条），不可作为 GOD 收藏的完备参照。**

**其它**：
- **Redump 没有 "Xbox 360 Digital"** —— 系统短码只有 `XBOX360`，媒介 CD/DVD-5/DVD-9。"Xbox 360 (Digital)" 是 **No-Intro** 的数据集。
- 混合盘规则逐字：*"Xbox/Xbox 360 Hybrid Discs goes under the Xbox dat."*
- **TOSEC** 仅 2 个 ISO 集共 **17** 条（Coverdiscs 2 + Samplers 15），**不存在游戏集**。
- **libretro-database**：`dat/` 无 X360；`metadat/Microsoft - XBOX 360 (Games on Demand).dat`（2019.2.1）**是空文件，只有 header**。
- **RetroAchievements**：`rc_consoles.h` 有 `RC_CONSOLE_XBOX = 22`，但 `hash.c` **零实现**。

**转储硬件**：需 Kreon 固件光驱（Samsung TSSTCorp SH-D162C/D、SH-D163A/B，及 Toshiba 换标 TS-H352C/D、TS-H353A/B），OmniDrive 也兼容。
XGD3 的 SS 已不再需要原厂 0800 光驱，wiki 逐字：*"this is no longer the case when using the latest version of redumper (**build 597 or later**) and **ZZ01 or DC02** revisions of the Kreon firmware."*
主流程 = **MPF**（<https://github.com/SabreTools/MPF>）驱动 DiscImageCreator/redumper。

三套工具的文件命名不一致：

| 工具 | ISO | SS | DMI | PFI |
|---|---|---|---|---|
| Xbox Backup Creator（旧） | `Track 01.iso` | `SS.bin` | `DMI.bin` | `PFI.bin` |
| DiscImageCreator | `<img>.iso` | `<img>_SS.bin` | `<img>_DMI.bin` | `<img>_PFI.bin` |
| redumper | `<img>.iso` | `<img>.ss`（原始 `.security`） | `.dmi`（原始 `.manufacturer`） | `.pfi`（原始 `.physical`） |

#### 2.3.9 中文汉化覆盖

⚠ **结论：Xbox 360 不存在任何系统性的汉化数据库。**

**（一）民间汉化：三大库合计 0 条**

| 数据源 | `[tr zh]` / 汉化 |
|---|---|
| TOSEC 2 个 X360 集（17 条） | **0**（TOSEC 全库 `[tr zh]` 共 564 次，只分布在 10 个 DAT，**全部是卡带/掌机老平台**：FC/NES、SFC/SNES、GB、GBC、GBA、N64、MD、WonderSwan Color、Neo-Geo Pocket Color、Dreamcast Firmware） |
| No-Intro X360 (Digital)（17 438 条） | **0** 条 `[tr ...]` |
| Redump X360（3 707 条） | **0**（政策上不收修改版） |
| GitHub | 检索 `xbox360 chinese translation` / `xbox 360 titleid database` 未找到任何 X360 汉化元数据/DAT 项目 |

中文社区侧（**无结构化数据、无哈希**）：

| 站点 | 条目 | 状态 |
|---|---|---|
| 游侠网 XBOX360 汉化补丁区 | 118 条 | 2011-03-23 ~ **2014-03-07 已停更**；仅标题+说明 |
| 巴哈姆特「Xbox 360 中文遊戲表」 | ~248 条 | 末次编辑 2019-06-11；**官中清单，非汉化** |
| Gamer520 / 跑跑车 / ZNDS 等 | 258~322 条 | 官中+汉化混列，计数互不一致 |

主要汉化组：游侠 LMAO、游侠翱翔。**2014 年 3 月后 X360 汉化产出即已停止。**
> **【未找到权威来源】** 权威的「X360 汉化总数」统计。

⚠⚠ **关键工程含义**：**汉化版通常只是替换过的 `default.xex`，其 TitleID 与原版完全相同** —— 必须靠 ISO/XEX 的**哈希差异**而非 TitleID 来区分汉化版与原版。

**（二）官方中文版：Redump 侧覆盖良好**

Redump X360 约 **305** 条含 `Zh`/`Asia`/`Taiwan` token，全为官方本地化，例如：
```
Fable II (USA, Asia) (En,Zh,Ko,Pl,Cs,Hu,Sk)
Gears of War (World) (En,Fr,De,Es,It,Zh,Ko)
Alan Wake (World) (En,Ja,Fr,De,Es,It,Zh,Ko,Pl,Ru)
```
区域盘：Taiwan **8** 张、Asia **55** 张（如 `Zhen Sanguo Wushuang 5 (Taiwan)`、`Steins;Gate (Taiwan)`）。抽样 Redump "Missing DMI & SS" 页 413 条，**50 条（约 12%）语言列表含 `Zh`**。

No-Intro (Digital) 侧含 `Zh` 者仅 **2** 条，均为官方多语言 XBLA 发行（`Crazy Mouse (World) (En,Ja,Fr,De,Es,It,Zh) (XBLA)`、`Duke Nukem 3D (USA) (En,Ja,Fr,De,Es,It,Pt,Zh) (XBLA)`）；区域标签 (World) 17 294 / (Japan) 96 / (USA) 37 / (Europe) 3 / (Korea) 2 —— **无任何 (China)/(Taiwan)/(Hong Kong)**。

#### 2.3.10 实现成本评估

⭐ **Rust 生态 —— Xbox 360 是本文 8 个平台里 Rust 支持最好的**

| crate | 版本 | 下载 | License | 仓库 | 评价 |
|---|---|---|---|---|---|
| ⭐ **`xdvdfs`** | 0.8.3 | 18 187 | MIT | <https://github.com/antangelo/xdvdfs> | **推荐**。虽然描述只写 "Original Xbox"，但**已支持 Xbox 360**（`XDVDFSOffsets` 枚举含 XGD1/2/3）。CLI 子命令 `ls/tree/md5/checksum/info/copy-out/unpack/pack/build-image/…`，**`copy-out` 可直接从 XGD2/XGD3 ISO 抽出 `\default.xex`**。sync/async 双模式，keywords 含 `no_std` |
| `xdvdfs-cli` | 0.8.3 | 16 717 | MIT | 同上 | CLI |
| `xex2` | 0.1.0 | 161 | MIT/Apache-2.0 | <https://github.com/landaire/acceleration> | XEX2 解析与提取 |
| `stfs` | 0.1.0 | 114 | MIT/Apache-2.0 | 同上 | STFS 包解析 |
| `xcontent` | 0.1.0 | 30 | MIT/Apache-2.0 | 同上 | XContent（CON/LIVE/PIRS） |
| `xecrypt` | 0.1.0 | 259 | MIT/Apache-2.0 | 同上 | 配套解密 |
| `lzxc` | 0.1.0 | 99 | MIT/Apache-2.0 | 同上 | LZX 解压 |
| `xenon_types` | 0.1.0 | 318 | MIT/Apache-2.0 | 同上 | 配套类型 |
| `iso2god` | 1.8.0 | — | — | <https://github.com/iliazeus/iso2god-rs> | **ISO→GOD 端到端实现，含 CON 头构造** |
| `xex` | 0.2.0 | 52 | MIT | minirop/xex | ⚠ 太薄，不可用 |
| `xbox` | 0.2.0 | 97 | — | nuzzles/xbox | ⚠ **是 Xbox Live 认证库，与文件格式无关** |
| `xdvdfs-core` / `xbox360` / `xiso` / `gdfx` / `xextool` | — | — | — | — | ❌ **均不存在于 crates.io**（`xdvdfs-core` 是仓库目录名，不是 crate 名） |

> ⚠ `xdvdfs` 仓库 main 已到 0.9.0，crates.io 仍是 0.8.3，且 `BlockDeviceRead` 有破坏性 API 变更。
> ⚠ **【推断】** `landaire/acceleration` 全系 crate 均 0.1.0、2026-04 首发、下载量 30–320、仓库 13 stars —— **API 稳定性无保证**，且 crates.io(0.1.0) 已落后仓库(0.2.0)。
> ⚠ `xdvdfs` 只把 360 光盘当"偏移后的 XDVDFS 容器"：**无 XEX 解析、无视频分区、无 SS/DMI/PFI、无 stealth 校验**。

**开源参考实现（非 Rust）**：

| 项目 | 语言 | 价值 |
|---|---|---|
| **Xenia** | C++ | 结构定义最权威：`kernel/util/xex2_info.h`（XEX 全部结构体+枚举）、`cpu/xex_module.cc`、`kernel/user_module.cc`（`title_id()` 最小实现范例）、`vfs/devices/stfs_xbox.h`、`stfs_container_device.cc`、`disc_image_device.cc`、`xbox.h` |
| **emoose/idaxex** | C++ | **唯一支持全部 6 种 XEX magic**；含 `xex1tool` CLI |
| **extract-xiso** | C | XGD 偏移常量权威来源（XboxDev 官方） |
| **iso2god-rs** | **Rust** | ISO→GOD 端到端，含 CON 头构造 |
| **Velocity / XboxInternals** | C++/Qt, GPLv3 | STFS 参考 |
| **abgx360** | C/C++ | **唯一**处理 SS/DMI/PFI/stealth 的开源实现 |
| **DiscImageCreator / redumper / MPF** | C++/C# | 转储工具链；redumper `dvd/xbox.ixx` 是 SS 结构最好的现代参考 |

> **【未找到权威来源】** `wxPirs` 的官方上游仓库（已被 Velocity 取代）；`xextool`（xorloser）是闭源 Windows 工具，用 idaxex 替代；`xbox-disc-tools` 仓库不存在（最接近的第三方是 `wiredopposite/XGDTool`，非 Redump 官方）。

**人日估算**（假设熟悉 Rust 与二进制格式，目标是"识别"而非"运行"）：

| 阶段 | 工作内容 | 用 crate | 从零 |
|---|---|---|---|
| P0-a | 光盘定位：6 个 XGD 偏移探测 + GDF 目录树遍历 + 找 `\default.xex` | 0.5 | 2 |
| P0-b | XEX 头解析：6 种 magic + 可选头目录 + Execution Info + 区域/分级（**无需任何加密**） | 1 | 1.5 |
| P0-c | STFS/GOD 头：magic + 固定偏移读 `0x344`/`0x354`/`0x360`/`0x411`（UTF-16BE 解码） | 1 | 1 |
| P0-d | GOD 形态探测：`.data` 目录 + `Data####` 分片 + SVOD 三布局判定 | 0.5 | 1 |
| | **小计 P0（纯识别）** | **≈ 3 人日** | ≈ 5.5 人日 |
| P0-e | ⭐（建议加）XeMID 直读（`video−0x1000` 的 DMI，纯 ASCII，含发行商/游戏 ID/XGD 世代/碟号） | +0.5 | +0.5 |
| P0-f | ⭐（建议加）SS 的 `layer0_end` PSN 判 XGD 世代 | +0.5 | +0.5 |
| P1 | 从 STFS/GOD 抽 `default.xex`（哈希表跳跃寻址 + 目录块解析） | 1 | 3 |
| P2 | 游戏显示名（从光盘）：AES-128-CBC + LZX 解压 + XDBF/SPA 解析 | 2 | 8 |
| P3 | DAT 匹配：全文件 SHA-1 + Redump/No-Intro 索引 | 1 | 1.5 |
| P4 | SS/DMI/PFI 完整 stealth 校验 | — | 10+（**不建议**） |

**推荐路线**：

| 方案 | 人日 | 覆盖 |
|---|---|---|
| **最小可用 P0** | **≈ 3 人日** | ISO/STFS/GOD/裸 XEX 四种形态的 TitleID + MediaID + 版本 + 碟号 + 内容类型 + 包内显示名 |
| ⭐ **P0 + XeMID + SS 世代判定** | **≈ 4 人日** | **强烈推荐，性价比最高** |
| 实用完整 P0+P1+P3 | ≈ 6 人日 | 加上从包内抽 XEX、DAT 匹配 |
| 含游戏名 +P2 | ≈ 8 人日 | 加上从光盘读官方显示名 |
| P4 | 放弃 | 改为直接比对 Redump 网站记录的 CRC-32 |

> ⚠ 但注意：**"提取并哈希 SS/DMI/PFI"本身现在是低成本的**（位置已知、大小已知 2048），只有*完整 stealth 验证*才昂贵。若只需比对 Redump 的 CRC32，记得先做 SS 清洗。

**主要风险**：
1. **字节序陷阱**：**GDF 用小端；XEX/STFS 用大端；STFS/SVOD Volume Descriptor 内的 u24 又是小端。**
2. free60 的 STFS 显示名字段描述有误（UTF-8 / 每语言 `0x80`），照抄会解析失败。
3. GOD 头文件名格式在官方包与第三方转换包之间不一致，探测应基于 `.data` 目录而非文件名。
4. Redump 的 SS 是清洗过的，直接哈希原始 SS 对不上。
5. `landaire/acceleration` 全系 crate 极新，API 稳定性无保证。

---

## 第 3 章 · 低成本组：老掌机与老光盘机

> 本章 4 个平台（WS/NGPC/Lynx/3DO）的共同点：**单文件或单光盘、内部头结构简单、DAT 覆盖完整**。识别器可以直接复用第一部分 D 章的 L0→L1→L2 流水线，只需要为每个平台加一个几十行的头解析函数。

### 3.1 WonderSwan / WonderSwan Color

> ⭐ **WS 的内部头在文件末尾，不在开头。** 这是它与本文其他所有卡带平台最大的区别。
> WSdev Wiki 原文：*"The header is stored at the end (the final sixteen bytes) of the ROM image. For this reason, it is also sometimes called a **'footer'**."*
> —— <https://ws.nesdev.org/wiki/ROM_header>

#### 3.1.1 三个独立一手来源

| 来源 | 类型 | 覆盖范围 |
|---|---|---|
| **WSdev Wiki `ROM_header`**（<https://ws.nesdev.org/wiki/ROM_header>，CC0） | 社区规格书 | **完整 16 字节**，含位定义、容量表、发行商表、mapper 值、boot ROM 校验字段 |
| **`wstech24.txt`**（WStech doc v2.4, Judge & Dox, 2003-12-26，随 Mednafen 分发） | 早期硬件文档 | **只有最后 10 字节**，`$9` 标为 `??` |
| **MAME `src/devices/bus/wswan/slot.cpp`**（<https://github.com/mamedev/mame/blob/master/src/devices/bus/wswan/slot.cpp>） | 生产级实现 | 读 `$6`–`$F` |
| **Mednafen `src/wswan/main.cpp`** | 生产级实现 | `memcpy(header, wsCartROM + rom_size - 10, 10);` |

`wstech24.txt` §6 逐字：
```
6. ROM HEADER

 Header taking last 10 bytes of each ROM file.
 Bytes  :
 0   - Developer ID
 1   - Minimum support system     00 - WS Mono / 01 - WS Color
 2   - Cart ID number for developer defined at byte 0
 3   - ??
 4   - ROM Size                   02 - 4Mbit … 09 - 128Mbit
 5   - SRAM/EEPROM Size           00-04 SRAM / 10,20,50 EEPROM
 6   - Additional capabilities(?) - bit 0 - 1 - vertical position , 1 - horizontal position
                                  - bit 2 - always 1
 7   - 1 - RTC (Real Time Clock)
 8,9 - Checksum = sum of all ROM bytes except two last ones ( where checksum is stored)
```

MAME `internal_header_logging()` 逐字（`ROM` 是 `const u16*`，`words = len >> 1`，小端）：
```cpp
const u8 romsize = ROM[words - 3] & 0xff;
const u8 ramtype = (ROM[words - 3] & 0xf000) ? 1 : 0;  // 1 = EEPROM, 0 = SRAM
const u8 ramsize = ramtype ? (((ROM[words - 3] >> 8) & 0xf0) >> 4) : ((ROM[words - 3] >> 8) & 0x0f);
logerror("\tDeveloper ID: %X\n",   ROM[words - 5] & 0xff);
logerror("\tMinimum system: %s\n", ROM[words - 5] & 0xff00 ? "WonderSwan Color" : "WonderSwan");
logerror("\tCart ID: %X\n",        ROM[words - 4] & 0xff);
logerror("\tROM size: %s\n",       romsize_str[romsize]);
logerror("\tFeatures: %X\n",       ROM[words - 2] & 0xff);
logerror("\tRTC: %s\n",           (ROM[words - 2] & 0xff00) ? "yes" : "no");
```
把 `ROM[words-n]` 换算为字节偏移（低字节位于 `len - 2n`）后，与另外两源**逐字节对齐，无一处冲突**：

| MAME 表达式 | 字节偏移 | 头内偏移 | wstech24 | WSdev |
|---|---|---|---|---|
| `ROM[words-5] & 0xff` | `L-10` | `$6` | byte 0 Developer ID ✓ | Developer ID ✓ |
| `ROM[words-5] & 0xff00` | `L-9` | `$7` | byte 1 Minimum system ✓ | Color ✓ |
| `ROM[words-4] & 0xff` | `L-8` | `$8` | byte 2 Cart ID ✓ | Game ID ✓ |
| `ROM[words-3] & 0xff` | `L-6` | `$A` | byte 4 ROM Size ✓ | ROM size ✓ |
| `ROM[words-3] >> 8` | `L-5` | `$B` | byte 5 SRAM/EEPROM ✓ | Save type ✓ |
| `ROM[words-2] & 0xff` | `L-4` | `$C` | byte 6 capabilities ✓ | Flags ✓ |
| `ROM[words-2] >> 8` | `L-3` | `$D` | byte 7 RTC ✓ | Mapper（见 3.1.6） |
| `ROM[words-1]` | `L-2` | `$E` | bytes 8,9 Checksum ✓ | Checksum ✓ |

#### 3.1.2 可识别字段表（完整 16 字节）

`L` = 文件总长度。**WS ROM 无外挂头，文件偏移 == 卡带偏移。**

| 头内偏移 | 文件偏移 | 映射地址 | 长度 | 字段名 | 含义 | 可靠性 |
|---|---|---|---|---|---|---|
| `$0` | `L-16` | `0xFFFF0` | 5 | **Far jump** | `EA <offset:2> <segment:2>`，即 x86 `JMP FAR ptr16:16`，CPU 复位入口 | **高（首字节 `0xEA` 是唯一的弱 magic）** |
| `$5` | `L-11` | `0xFFFF5` | 1 | Maintenance | **bit0–3 必须为 0**（否则主机拒绝执行）；bit7 = splash bypass（仅 Color） | 中（有效性校验） |
| `$6` | `L-10` | `0xFFFF6` | 1 | **Developer ID** | 发行商代码，见 3.1.7 | **高**（与 Cart ID 组合近似序列号） |
| `$7` | `L-9` | `0xFFFF7` | 1 | **Color / Min system** | `00` = WS Mono，`01` = 支持 Color | **高**（WS/WSC 判别的**唯一**内部依据） |
| `$8` | `L-8` | `0xFFFF8` | 1 | **Game ID / Cart ID** | 该发行商下的游戏序号，**BCD**（WSdev 标注 "binary-coded decimal"） | **高** |
| `$9` | `L-7` | `0xFFFF9` | 1 | **Game version / Safe mode** | bit0–6 = 版本号；bit7 = 内部 EEPROM 写保护 | 中–高（**注意**：`wstech24.txt` 此字节标为 `??`，MAME/Mednafen 都不读它，只有 WSdev 给出定义） |
| `$A` | `L-6` | `0xFFFFA` | 1 | ROM size | 见 3.1.4 | 中（可与文件大小交叉校验） |
| `$B` | `L-5` | `0xFFFFB` | 1 | Save type / size | 见 3.1.5 —— **源间有真实分歧** | 中（分类用） |
| `$C` | `L-4` | `0xFFFFC` | 1 | Flags | bit0 启动方向 / bit1 ROM 总线位宽 / bit2 ROM 等待周期 | 低–中 |
| `$D` | `L-3` | `0xFFFFD` | 1 | Mapper（旧称 RTC 标志） | `$00` = Bandai 2001 或 KARNAK；`$01` = Bandai 2003 —— 见 3.1.6 | 低 |
| `$E` | `L-2` | `0xFFFFE` | 2（小端） | **Checksum** | 除最后两字节外全 ROM 字节和 | **中–高**（一致性校验 + 弱识别键） |

**映射地址 `0xFFFF0` 的依据**【推断，由四条事实唯一推出】：
1. WSdev Memory map：`0x20000–0xFFFFF` 为卡带 ROM 总线；
2. 头是 ROM 映像最后 16 字节，复位时 linear bank 映射 ROM 顶端（MAME `ws_rom_device::device_reset()`：`m_base40 = (((0xf0 & m_bank_mask) | 4) << 15) & m_rom_mask;`）；
3. V30MZ 复位时 `PS = 0xFFFF`、`PC = 0` → 物理地址 `0xFFFF0`（Mednafen `v30mz.cpp:153`：`I.sregs[PS] = 0xffff;`）；
4. 头的前 5 字节正是远跳转指令。
**WSdev 页面本身未给出该数值地址。**

**引导 ROM 校验的字段**（WSdev 逐字）：*"Parts of this header are used by the boot ROM and some are validated on Color models - they are marked in bold."* 加粗的是 `$0`、`$5`、`$9`、`$C`、`$D`。
⚠ **Color 字节 `$7` 未被加粗**，页面亦未说明校验和是否被硬件验证。

#### 3.1.3 Checksum 算法（三源一字不差）

| 来源 | 表述 |
|---|---|
| `wstech24.txt` | `Checksum = sum of all ROM bytes except two last ones ( where checksum is stored)` |
| WSdev | `"Checksum (sum of all ROM bytes excluding the checksum)"` |
| Mednafen `main.cpp:365` | `uint16 real_crc = 0; for(unsigned int i = 0; i < rom_size - 2; i++) real_crc += wsCartROM[i];` |
| MAME `slot.cpp:416-427` | 全 ROM 逐字节累加进 `u16 sum`，再 `sum -= ROM[words-1] & 0xff; sum -= ROM[words-1] >> 8;`（等价） |

```
u16 sum = (Σ file[0 .. L-3]) mod 65536
应等于 (rom[L-1] << 8) | rom[L-2]
```

> ⚠ **Mednafen 特有行为**：它把文件先向上补齐到 64 KB 再补齐到 2 的幂，且在**前端**填 `0xFF`（`memset(wsCartROM, 0xFF, rom_size - real_rom_size)`），校验和是对补齐后的缓冲区算的。**真实卡带 dump 尺寸本就是 2 的幂**（实测 No-Intro 499 条 WS/WSC，尺寸集合为 `{4096, 8192, 524288, 1048576, 2097152, 4194304, 8388608, 16777216}`，**非 2 的幂者 0 条**），故与直接对文件求和等价。

#### 3.1.4 ROM 容量码（`$A`）

| 码 | WSdev | wstech24 | MAME `romsize_str[]` |
|---|---|---|---|
| `$00` | 1 Mbit (128 KiB) | — | Unknown |
| `$01` | 2 Mbit (256 KiB) | `?` | Unknown |
| `$02` | 4 Mbit (512 KiB) | 4Mbit | 4Mbit ✓ |
| `$03` | 8 Mbit (1 MiB) | 8Mbit | 8Mbit ✓ |
| `$04` | 16 Mbit (2 MiB) | 16Mbit | 16Mbit ✓ |
| `$05` | 24 Mbit (3 MiB) | `?` | Unknown |
| `$06` | 32 Mbit (4 MiB) | 32Mbit | 32Mbit ✓ |
| `$07` | 48 Mbit (6 MiB) | `?` | Unknown |
| `$08` | 64 Mbit (8 MiB) | 64Mbit | 64Mbit ✓ |
| `$09` | 128 Mbit (16 MiB) | 128Mbit | 128Mbit ✓ |
| `$0A` / `$0B` | 256 / 512 Mbit | — | — |

重叠部分三源完全一致；`$00/$01/$05/$07/$0A/$0B` 仅 WSdev 有记载。

#### 3.1.5 存档类型码（`$B`）—— **存在真实分歧**

| 码 | WSdev | wstech24 | MAME `get_cart_type()` | Mednafen |
|---|---|---|---|---|
| `$00` | None | 0k | 无 | 0 |
| `$01` | **SRAM 256 Kbit (32 KiB)** | **64k SRAM** | **64Kbit → 8 KiB** | **8 KiB** |
| `$02` | **SRAM 1 Mbit (128 KiB)** | **256k SRAM** | **256Kbit → 32 KiB** | **32 KiB** |
| `$03` | SRAM 128 KiB | 1M SRAM | 1Mbit → 128 KiB ✓ | 128 KiB ✓ |
| `$04` | SRAM 256 KiB | 2M SRAM | 2Mbit → 256 KiB ✓ | 256 KiB ✓ |
| `$05` | **SRAM 512 KiB** | — | **512Kbit → 64 KiB** | **512 KiB**（注释 "Wonder Gate"） |
| `$10` | EEPROM 1 Kbit (128 B) | 1k EEPROM | 128 B ✓ | 128 B ✓ |
| `$20` | EEPROM 16 Kbit (2 KiB) | 16k EEPROM | 2 KiB ✓ | 2 KiB ✓ |
| `$50` | EEPROM 8 Kbit (1 KiB) | 8k EEPROM | 1 KiB ✓ | 1 KiB ✓ |

- `$03/$04/$10/$20/$50` 四源一致。
- **`$01`、`$02` 上 WSdev 与「wstech24 + MAME + Mednafen」三家不一致**（WSdev 大一档）。**若只需分类不需精确容量，建议采用模拟器共识**（`$01`=8 KiB、`$02`=32 KiB），因为它有三个独立实现支持。
- `$05` 上 MAME（64 KiB）是唯一异类，WSdev 与 Mednafen 都说 512 KiB。

> ⚠ **MAME 自身的一处内部矛盾**：同一文件里 `get_cart_type()` 的 switch 注释（`0x03`=1Mbit、`0x04`=2Mbit、`0x05`=512Kbit）与 `internal_header_logging()` 的静态表 `sram_str[] = {none, 64Kbit, 256Kbit, 512Kbit, 1Mbit, 2Mbit}`（按索引取值）**对不上**。
> **【推断】** `sram_str[]` 是按「递增大小」排的，而实际编码值 `03/04/05` 的语义并非递增，导致日志函数错位。**`get_cart_type()` 与 `wstech24.txt` 一致，应以它为准**；`internal_header_logging()` 只影响日志输出，不影响仿真。

#### 3.1.6 `$D` 的两种读法

| 来源 | 表述 |
|---|---|
| WSdev | `$D` = **Mapper**，`$00` = Bandai 2001 / KARNAK，`$01` = Bandai 2003 |
| `wstech24.txt` §6 | `7 - 1 - RTC (Real Time Clock)` |
| MAME `slot.cpp:173` | `if (ROM[(size >> 1) - 2] & 0xff00) m_cart->set_has_rtc(true);`（即 `$D != 0` → 有 RTC） |
| WSdev `Bandai 2003` 页 | *"In addition to the normal Mapper banking interface, Bandai's 2003 adds registers for an **RTC interface**, GPO pins, self flashing, and accessing more than 16MiB of ROM."* |

> **【推断】** 两种读法在实卡上结果等价：`$D = $01` → Bandai 2003 → 该 mapper 才提供 RTC。故"`$D != 0` 即有 RTC"是旧文档对 mapper 字节的经验性解释。**识别器应把它记作 mapper，并附带"可能有 RTC"的派生属性。**

#### 3.1.7 Developer ID 表（`$6`）

两个独立源：WSdev Wiki（含 3 字母代号）与 Mednafen `main.cpp:158` 的 `Developers[]`。**Bandai = `$01`，双源一致。**

| ID | 代号 | WSdev | Mednafen | 一致？ |
|---|---|---|---|---|
| `$01` | BAN | **Bandai** | Bandai | ✓ |
| `$02` | TAT | Taito | Taito | ✓ |
| `$03` | TMY | Tomy | Tomy | ✓ |
| `$04` | KEX | Koei | Koei | ✓ |
| `$05` | DTE | Data East | Data East | ✓ |
| `$06` | AAE | Asmik Ace | Asmik | ✓ |
| `$07` | MDE | Media Entertainment | Media Entertainment | ✓ |
| `$08` | NHB | Nichibutsu | Nichibutsu | ✓ |
| `$0A` | CCJ | Coconuts Japan | Coconuts Japan | ✓ |
| `$0B` | SUM | Sammy | Sammy | ✓ |
| `$0C` | SUN | Sunsoft | Sunsoft | ✓ |
| `$0D` | PAW | Mebius | Mebius | ✓ |
| `$0E` | BPR | Banpresto | Banpresto | ✓ |
| `$10` | JLC | Jaleco | Jaleco | ✓ |
| `$11` | MGA | Imagineer | Imagineer | ✓ |
| `$12` | KNM | Konami | Konami | ✓ |
| `$16` | KBS | Kobunsha | Kobunsha | ✓ |
| `$17` | BTM | Bottom Up | Bottom Up | ✓ |
| **`$18`** | KGT | **Kaga Tech** | Naxat（带疑问注释） | ✗ **WSdev 正确，见下** |
| `$19` | SRV | Sunrise | Sunrise | ✓ |
| `$1A` | CFT | Cyber Front | Cyberfront | ✓ |
| `$1B` | MGH | Megahouse | Megahouse | ✓ |
| `$1D` | BEC | Interbec | Interbec | ✓ |
| `$1E` | NAP | Nihon Application | NAC | ✗ |
| `$1F` | BVL | Bandai Visual | Emotion（注 "Bandai Visual??"） | WSdev 更明确 |
| `$20` | ATN | Athena | Athena | ✓ |
| `$21` | KDX | KID | KID | ✓ |
| `$22` | HAL | HAL Corporation | HAL | ✓ |
| `$23` | YKE | Yuki Enterprise | Yuki-Enterprise | ✓ |
| `$24` | OMN | Omega Micott | Omega Micott | ✓ |
| `$25` | LAY | **Layup** | Upstar | ✗ |
| `$26` | KDK | Kadokawa Shoten | Kadokawa/Megas | ≈ |
| `$27` | SHL | **Shall Luck** | Cocktail Soft | ✗ |
| `$28` | SQR | Squaresoft | Squaresoft | ✓ |
| `$2A` | SCC | ?（SUNCORPORATION?） | NTT DoCoMo | ✗ |
| `$2B` | TMC | Tom Create | TomCreate | ✓ |
| `$2D` | NMC | Namco | Namco | ✓ |
| `$2E` | SES | Soeishinsha | *(无)* | 仅 WSdev |
| `$2F` | HTR | **Hearty Robin** | Gust | ✗ |
| `$31` | VGD | Vanguard | Vanguard（注 "or Elorg?"） | ✓ |
| `$32` | MGT | Megatron | Megatron | ✓ |
| `$33` | WIZ | Wiz | WiZ | ✓ |
| `$35` | TAN | Tanita | *(无)* | 仅 WSdev |
| `$36` | CPC | Capcom | Capcom | ✓ |

⭐ **独立佐证：3 字母代号与卡带印刷序列号前缀严格对应**【实测】。对照 MAME `hash/wswan.xml` 与 libretro `metadat/serial`：

| 序列号 | 代号 | ID | 佐证 |
|---|---|---|---|
| `SWJ-BAN010`（GunPey） | BAN | `$01` | Bandai ✓ |
| `SWJ-SUM004`（Anchorz Field） | SUM | `$0B` | MAME publisher = "Sammy" ✓ |
| `SWJ-TMY001` | TMY | `$03` | Tomy ✓ |
| **`SWJ-KGT003`**（Bakusou Dekotora Densetsu） | KGT | `$18` | MAME publisher = **"Kaga Tech"** → **直接证明 WSdev 的 `$18`=Kaga Tech 正确、Mednafen 的 "Naxat" 错误** |
| `SWJ-HTRC01`（Alchemist Marie & Elie） | HTR | `$2F` | 佐证 WSdev 的 Hearty Robin |

> **建议：发行商表采信 WSdev。** 但注意 **卡带序列号 `SWJ-XXXnnn` 无法从 header 完整重建** —— header 只有 1 字节发行商 ID + 1 字节 BCD 游戏 ID，缺 `SWJ`/`SWL` 前缀与地区位。

#### 3.1.8 磁盘上的"变体成型"形态

**单文件，无任何外挂头，无并存容器格式** —— 本文 8 个平台里形态最简单的一个。

| 形态 | 扩展名 | 判定 |
|---|---|---|
| WonderSwan（单色） | `.ws` | `rom[L-9] == 0x00` |
| WonderSwan Color | `.wsc` | `rom[L-9] == 0x01` |
| 通用 / TOSEC | `.bin` | TOSEC 的 WS/WSC 集统一用 `.bin` |
| MAME 接受的扩展名 | `ws,wsc,bin` | `ws_cart_slot_device::file_extensions()` |
| Mednafen `KnownExtensions[]` | `.ws` / `.wsc` / **`.wsr`** | 见下 |
| beetle-wswan | `ws\|wsc\|pc2` | `MEDNAFEN_CORE_EXTENSIONS` |

**唯一存在附加结构的 WS 文件形态 —— `.wsr` 音乐 rip**：
Mednafen `main.cpp:278`：`if(!memcmp(wsCartROM + ... + fp_in_size - 0x20, "WSRF", 4))` —— 文件**末 `0x20` 字节**为 `WSRF` 尾部，`wsr_footer[0x5]` 为当前曲目。

**WonderWitch 固件的特殊识别**（Mednafen/beetle）：
`rom_size == 0x80000 && memcmp(&rom[0x70000],"ELISA",5)==0 && crc32(&rom[0x7FFF0],0x10)==0x0d05ed64`，再排除 3 个 CRC32 黑名单（`0x63f00316`、`0x60fd569b`、`0xe11538f8`）。

**兼容硬件**：`.pc2`（Benesse Pocket Challenge V2，No-Intro **67** 条）、`.pcw`（Pocket Challenge W，**79** 条）。MAME 有 `pockchalv2` 软件列表。

**无 magic**：freedesktop.org shared-mime-info 对 `application/x-wonderswan-rom` 与 `-color-rom` **只定义 glob（`*.ws` / `*.wsc`），没有 `<magic>` 规则** —— 因为头在文件末尾且没有固定常量。
来源：<https://gitlab.freedesktop.org/xdg/shared-mime-info>

⭐ **模拟器如何区分 WS 与 WSC？答案是：它们都不区分。**

| 实现 | 做法 |
|---|---|
| **Mednafen / beetle-wswan** | **完全不区分**。`src/wswan/main.cpp:45` 与 `beetle-wswan/libretro.c:400` 均为 `int wsc = 1; /*color/mono*/`，且该变量**从未被 header 赋值**，只在 `gfx.cpp:487` 用于端口 `0xA0` 返回值 —— **Mednafen 永远以 WSC 硬件运行** |
| **MAME** | 靠**机器选择**而非 ROM 内容：`wswan_state::machine_start()` 硬设 `TYPE_WSWAN`，`wscolor_state::machine_start()` 硬设 `TYPE_WSC`；软件列表分为 `wswan` 与 `wscolor`（互为 `set_compatible`） |
| **No-Intro** | 靠 DAT 归属 + 扩展名人工划分（实测两个 DAT 的扩展名 100% 纯净） |
| **唯一的内容级依据** | header `$7` 字节。MAME 仅在 `internal_header_logging()` 里**打印**它，不据此改变行为 |

> **结论：识别器应当自己读 `$7` 做 WS/WSC 判定 —— 现有模拟器都没做这件事。**

#### 3.1.9 DAT / 数据库覆盖

| 库 | 集合 | 条目 | 哈希粒度 |
|---|---|---|---|
| **No-Intro** | `Bandai - WonderSwan`（20260525-011654） | **257** | 整文件，crc+md5+sha1 全量；**sha256 部分**（148/257） |
| **No-Intro** | `Bandai - WonderSwan Color`（20260525-011610） | **242** | 同上；sha256 184/242 |
| **TOSEC** | WS 4 个集合 | **208** | Games 182 / Applications 22 / Demos 3 / Firmware 1 |
| **TOSEC** | WSC 4 个集合 | **211** | Games 172 / Demos 31 / Applications 7 / Firmware 1 |
| **MAME** | `hash/wswan.xml` **126** / `hash/wscolor.xml` **108** | 234 | ⭐ **芯片级**（见下） |
| **libretro-database** | `metadat/no-intro/` 219 + 229；`metadat/serial/` **107 + 90**；`metadat/hacks/` 1 + 3 | — | 精简版 |
| **libretro `metadat/headered/`** | **不存在** | — | ⭐ **佐证 WS 无外挂头** |
| **Redump** | — | 0 | 卡带机 |

**没有含头/去头之分** —— 每条 game 恰好 1 条 rom，**headered == headerless**。

⭐ **MAME 的 WS software list 是本文覆盖的平台里元数据最丰富的**：
```xml
<software name="anchorz">
    <description>Anchorz Field</description>
    <year>1999</year>
    <publisher>Sammy</publisher>
    <info name="serial" value="SWJ-SUM004"/>
    <info name="release" value="19990624"/>
    <info name="alt_title" value="アンカーズ・フィールド"/>
    <part name="cart" interface="wswan_cart">
        <feature name="pcb" value="PTE-0037" />
        <feature name="u1" value="BANDAI 2001" />
        <feature name="u2" value="GIZA" />
        <feature name="u3" value="ROM" />
        <feature name="u4" value="BS62LV256TC SRAM" />
        <feature name="slot" value="ws_sram" />
        <dataarea name="rom" size="1048576" width="16" endianness="little">
            <rom name="mh8m256s033a.u3" size="1048576" crc="425eb893" sha1="06447c248afce04c4f4a1c3b5f7a4ab6f383c94b" offset="000000" />
        </dataarea>
        <dataarea name="sram" size="8192" width="16" endianness="little">
        </dataarea>
    </part>
</software>
```
- `<rom name>` 是**真实掩模 ROM 型号**（`mh8m256s033a.u3`）
- `<feature name="u1">` = mapper 芯片（`BANDAI 2001`），**可与 header `$D` 交叉验证**
- `<feature name="slot">` = 存档类型（`ws_sram` / `ws_eeprom` / `ws_rom` / `wwitch`），**可与 header `$B` 交叉验证**
- **空的** `<dataarea name="sram">` / `<dataarea name="eeprom">` 声明存档容量 —— 这是 WS 独有的双 dataarea 结构（NGP 没有）
- `wswan.xml` 开头有一份 CDATA 大表，列出全部已知 WS/WSC 游戏的 `CRC | Serial | Rev | Name`

> ⚠ **No-Intro 的 WS/WSC DAT 完全没有 `serial` 属性**【实测】：499 条记录中 `serial=` 出现 **0** 次。序列号只能从另外三处取：
> - **MAME**：`<info name="serial" value="SWJ-SUM004"/>`（125/126 + 102/108 条有）
> - **TOSEC**：写进文件名方括号，如 `…(Bandai)[tr en][SWJ-BAN002]`
> - **libretro `metadat/serial/`**：`serial "SWJ-SUM004"` + `crc`

#### 3.1.10 中文汉化覆盖

| 数据源 | `[tr zh]` / 中文条目 |
|---|---|
| **TOSEC `Bandai WonderSwan Color - Games`（172 条）** | **1**：`Kidou Senshi Gundam Seed (2003-03-15)(Bandai)[tr zh][v.20040120]`，4 MiB，CRC32 `b338cae2`，SHA-1 `be172f59176af9841cc6c2d9fcd019f01f76a265` |
| TOSEC `Bandai WonderSwan - Games`（182 条） | **0**（7 条翻译版是 `[tr en]` ×6、`[tr de]` ×1） |
| No-Intro WS / WSC | **0**（政策上不收翻译 hack） |
| libretro `metadat/hacks/Bandai - WonderSwan.dat` | 1 条，`Kaze no Klonoa - Moonlight Museum (Japan) [T-En by PowerStone05]` → **英译** |
| libretro `metadat/hacks/Bandai - WonderSwan Color.dat` | 3 条，全部 `[T-En]` → **0 中文** |
| MAME `hash/wswan.xml` + `wscolor.xml` | **0**（grep `chinese\|zh\|汉化\|漢化\|translat` 无命中） |

**结论【事实】：公开数据库中 WonderSwan 系列的中文汉化条目共 1 条（WSC 的 Gundam Seed）。**

#### 3.1.11 实现成本评估

```rust
// 伪代码
if file_len < 65536 { return None; }               // Mednafen 的最小尺寸门槛
let h = &data[file_len - 16 ..];                    // 末 16 字节

// 弱 magic + 有效性
let jmp_ok    = h[0x0] == 0xEA;                     // 远跳转 opcode
let maint_ok  = h[0x5] & 0x0F == 0;                 // Maintenance bit0-3 必须为 0
let stored    = u16::from_le_bytes([h[0xE], h[0xF]]);
let calc: u16 = data[..file_len-2].iter().fold(0u16, |a, &b| a.wrapping_add(b as u16));
let sum_ok    = stored == calc;                     // ← 强指纹

// 字段
let publisher   = h[0x6];        // 查发行商表（采信 WSdev）
let is_color    = h[0x7] != 0;   // ← WS vs WSC 判定的唯一内容依据
let game_id_bcd = h[0x8];
let version     = h[0x9] & 0x7F;
let rom_size    = h[0xA];        // 查表
let save_type   = h[0xB];        // 查表（注意 $01/$02/$05 的源分歧）
let flags       = h[0xC];        // bit0 方向 / bit1 位宽 / bit2 等待
let mapper      = h[0xD];        // 0=Bandai2001/KARNAK, 1=Bandai2003(含 RTC)
```

特例分支（可选）：
- `.wsr`：`&data[len-0x20 .. len-0x1C] == b"WSRF"` → 音乐 rip，跳过普通头解析
- WonderWitch 固件：`len == 0x80000 && &data[0x70000..0x70005] == b"ELISA" && crc32(&data[0x7FFF0..]) == 0x0d05ed64`

| 项 | 评估 |
|---|---|
| 核心代码量 | **约 120–180 行 Rust**（含 3 张查表），零外部依赖 |
| 需要的表 | 发行商表（~43 项）、ROM 容量表（12 项）、存档类型表（9 项） |
| 识别强度 | `checksum` + `0xEA` + `Maintenance 低 4 位为 0` 三重条件，误判率极低。**但文件开头无 magic，必须靠末尾** |
| 风险点 | 存档类型码 `$01/$02/$05` 源分歧；发行商表 6 处 WSdev 与 Mednafen 冲突（**采信 WSdev**，已被序列号前缀独立佐证） |
| **现成 Rust crate** | **无**。crates.io 搜索 `wonderswan` / `swan` 全是无关结果；`file-format` crate（172 万下载）的 `src/signatures.rs` 中**完全没有 WonderSwan 条目**（因为文件开头无 magic） |
| 可参考实现 | MAME `slot.cpp::internal_header_logging()`（最完整）；Mednafen `wswan/main.cpp::Load()`；WSdev Wiki `ROM_header`（唯一完整的 16 字节布局，CC0） |
| **人日估算** | **0.3 人日**（头解析 + 校验和 + 单测），含 DAT 接入 **0.5 人日** |

---

### 3.2 Neo Geo Pocket / Color

> NGPC 的头在**文件开头**、长度固定 `0x40`、开头 28 字节是固定 magic —— **是本文 8 个平台里最容易识别的一个**。

#### 3.2.1 头结构：SNK 官方规格书

来源：SNK 官方 NGPC 开发规格书 `ngpcspec.txt`（<http://devrs.com/ngp/files/DoNotLink/ngpcspec.txt>），"Cart ROM Header Info" 表逐字：
```
ADDRESS   LENGTH     CONTENT
 0x200000  28 Bytes   Software cassette recognition code *
 0x20001C   4 Bytes   Software cassette startup address
 0x200020   2 Bytes   Software cassette ID code
 0x200022   1 Byte    Software cassette ID sub-code (version)
 0x200023   1 Byte    Compatible system code
 0x200024  12 Bytes   Software cassette title name
 0x200030  16 Bytes   (Reserved: please write in 0)
```
卡带 ROM 映射在 TLCS-900/H 地址空间的 `0x200000`，故**文件偏移 = 映射地址 − `0x200000`**。

第二来源：Mednafen/NeoPop `mednafen/ngp/rom.h`（<https://github.com/libretro/beetle-ngp-libretro/blob/master/mednafen/ngp/rom.h>），**结构体自带偏移注释**：
```c
typedef struct
{
    uint8_t   licence[28];      // 0x00 - 0x1B
    uint32_t  startPC;          // 0x1C - 0x1F
    uint16_t  catalog;          // 0x20 - 0x21
    uint8_t   subCatalog;       // 0x22
    uint8_t   mode;             // 0x23
    uint8_t   name[12];         // 0x24 - 0x2F
    uint32_t  reserved1;        // 0x30 - 0x33
    uint32_t  reserved2;        // 0x34 - 0x37
    uint32_t  reserved3;        // 0x38 - 0x3B
    uint32_t  reserved4;        // 0x3C - 0x3F
} __attribute__((__packed__)) RomHeader;
```
Mednafen 1.32.1 的 `src/ngp/neopop.h:83` 还有编译期断言：`static_assert(sizeof(RomHeader) == 0x40, "RomHeader wrong size!");`

#### 3.2.2 可识别字段表

NGP ROM **无外挂头**，文件偏移 == 卡带偏移。TLCS-900/H 是**小端**。

| 字段名 | 偏移量 | 长度 | 含义 | 可靠性 |
|---|---|---|---|---|
| **licence / 版权字符串** | `0x00`–`0x1B` | **28** | `"COPYRIGHT BY SNK CORPORATION"` 或 `" LICENSED BY SNK CORPORATION"` | **极高（magic）/ 零（区分游戏）** |
| `startPC` | `0x1C`–`0x1F` | 4 | 入口点地址（u32 LE，Mednafen 用时 `& 0xFFFFFF` 取 24 位） | 低 |
| **catalog** | `0x20`–`0x21` | **2**（LE，**BCD**） | 游戏目录号 | **极高**（NGPC 上的主序列号） |
| **subCatalog** | `0x22` | 1 | 目录号次级编号 / 版本 | 高 |
| **mode / color flag** | `0x23` | 1 | `0x00` = NGP 单色，`0x10` = NGPC 彩色 | **中**（至少 3 个已知 ROM 写错，见 3.2.4） |
| **name / 游戏名** | `0x24`–`0x2F` | **12** | ASCII，NeoPop 把 `<32` 或 `≥128` 的字节替换为空格 | 高 |
| reserved1–4 | `0x30`–`0x3F` | 4 ×4 | SNK 规格书原文 "(Reserved: please write in 0)" | 有效性校验 |

> **【事实】NGP 头里没有校验和字段** —— `rom.h` 的 64 字节全部有名字，四源均无 checksum。NGPC 与第一部分的 NES 一样，是少数完全无自校验结构的平台。

#### 3.2.3 版权字符串的精确形态（三重验证）

SNK 规格书列出两条：`"COPYRIGHT BY SNK CORPORATION"`（SNK use）与 `"LICENSED BY SNK CORPORATION"`（Third party use）。
**但字段长度固定 28 字节**，而 `"LICENSED BY SNK CORPORATION"` 只有 27 字符 —— 缺的那一位是**前导空格**。两项独立证据：

**证据 A —— libretro RACE `main.c:172,182-187` 逐字**：
```c
char *license_info = " BY SNK CORPORATION";
...
/* check NEOGEO POCKET
 * check license info */
for (i=0;i<19;i++)
{
   if (mainrom[0x000009 + i] != license_info[i])
      rom_found = 0;
}
```
即 `0x09`–`0x1B`（19 字节）恒为 `" BY SNK CORPORATION"`，于是 `0x00`–`0x08`（9 字节）必须是 `"COPYRIGHT"`（9 字符）或 `" LICENSED"`（空格 + 8 字符 = 9 字符）。

**证据 B —— freedesktop.org shared-mime-info**（<https://gitlab.freedesktop.org/xdg/shared-mime-info>）：
```xml
<mime-type type="application/x-neo-geo-pocket-rom">
    <glob pattern="*.ngp"/>
    <magic>
      <match offset="35" type="byte" value="0x0">
        <match offset="0" type="string" value="COPYRIGHT BY SNK CORPORATION"/>
        <match offset="0" type="string" value=" LICENSED BY SNK CORPORATION"/>
      </match>
    </magic>
</mime-type>

<mime-type type="application/x-neo-geo-pocket-color-rom">
    <glob pattern="*.ngc"/>
    <magic>
      <match offset="35" type="byte" value="0x10">
        <match offset="0" type="string" value="COPYRIGHT BY SNK CORPORATION"/>
        <match offset="0" type="string" value=" LICENSED BY SNK CORPORATION"/>
      </match>
    </magic>
</mime-type>
```
（Rust 的 `file-format` crate `src/signatures.rs:109-111` 也用同样两条字符串。）

**最终结论【事实】：**

| 字符串（严格 28 字节） | 用途 |
|---|---|
| `"COPYRIGHT BY SNK CORPORATION"` | **SNK 自社发行**（无前导空格，9+19=28） |
| `" LICENSED BY SNK CORPORATION"` | **第三方授权发行**（**有 1 个前导空格**，9+19=28） |

freedesktop 的 `<match offset="35">` 同时独立确认了：**偏移 35 = `0x23` 就是 `mode` 字段，`0x00` = 单色 / `0x10` = 彩色**。

#### 3.2.4 `0x23` 彩色标志的五重验证 —— 以及它会被写错

1. SNK 官方规格书：`Monochrome = 0x00 / Color = 0x10`
2. Mednafen `ngp/rom.cpp:67`：`if(rom_header->mode & 0x10) MDFN_printf(_("Color")); else MDFN_printf(_("Greyscale"));`
3. Mednafen `ngp/mem.cpp:622-624`：
   ```c
   /* Color Mode Selection: 0x00 = B&W, 0x10 = Colour */
   storeB(0x6F91, rom_header->mode);
   storeB(0x6F95, rom_header->mode);
   ```
4. libretro RACE `main.c:193-207`：`i = mainrom[0x000023]; if (i == 0x10 || i == 0x00) { ... if (i == 0x10) m = NGPC; else m = NGP; }`
5. Rust `file-format` crate：NGPC 判定 = 识别码 + `b"\x10"` at offset 35

⚠ **但 `0x23` 在真实 ROM 上会被写错。** NeoPop `rom.c` 的 `rom_hack()` 硬编码修正 3 个 ROM：
```c
#define MATCH_CATALOG(c, s) (rom_header->catalog == HTOLE16(c) && rom_header->subCatalog == (s))

/* "Neo-Neo! V1.0 (PD)" */
if (MATCH_CATALOG(0, 16))     ngpc_rom.data[0x23] = 0x10;   /* Fix ROM header */
/* "Cool Cool Jam SAMPLE (U)" */
if (MATCH_CATALOG(4660, 161)) ngpc_rom.data[0x23] = 0x10;   /* Fix ROM header */
/* "Dokodemo Mahjong (J)" */
if (MATCH_CATALOG(51, 33))    ngpc_rom.data[0x23] = 0x00;   /* Fix ROM header */
```
（RACE 另有针对 `0x20/0x21 == 0x34/0x12` 的同类修正。）
> **识别器不能把 `0x23` 当唯一判据**，需用文件大小、DAT 交叉校验兜底。

#### 3.2.5 ⭐ catalog 是 BCD —— 可直接算出 No-Intro 的 `serial`

| 游戏 | Mednafen `MATCH_CATALOG(c,s)` | No-Intro `serial=` | MAME `<info name="serial">` |
|---|---|---|---|
| Dokodemo Mahjong (J) | `(51, 33)` → 51 = `0x33` | `0033` | `NeoP00330` |
| Cool Cool Jam SAMPLE (U) | `(4660, 161)` → 4660 = `0x1234` | `1234` | — |
| Sonic - Pocket Adventure | RACE `case 0x0059` | `0059` | — |
| Metal Slug - 2nd Mission | RACE `case 0x0061` | `0061` | — |
| Bakumatsu Rouman Tokubetsu Hen | — | `0064` | `NeoP00640` |
| Big Bang Pro Wrestling | — | `0066` | `NeoP00660` |
| Baseball Stars Color | — | `0025` | `NeoP00250` |
| Infinity Cure (J) | — | `0109` | libretro `NEOP01090` |

**结论【事实】**：`catalog` 是小端 u16，其 **4 位十六进制数字即为十进制目录号**（BCD 编码）。
**【推断】**：印刷序列号 `NEOPccccS` = `NEOP` + catalog 的 4 位 BCD + subCatalog 的 1 位（欧版部分只有 8 位，如 `NEOP0090` = Faselei! (Europe)）。**No-Intro 的 `serial` 属性 = catalog 的 4 位 BCD。**

⭐ **工程价值**：这意味着**只读 2 字节就能算出 No-Intro serial，做无哈希匹配** —— 这是 NGPC 相对 WonderSwan 的巨大优势（No-Intro 的 WS DAT 根本没有 serial 字段）。

```rust
let serial = format!("{:02X}{:02X}", data[0x21], data[0x20]);   // BCD → "0059"
```

#### 3.2.6 MAME 对 NGP 头的处理

MAME `src/mame/snk/ngp.cpp::load_ngp_cart()` **完全不解析头**，只按尺寸判定 flash 芯片：
```c
if (size != 0x8000 && size != 0x8'0000 && size != 0x10'0000 && size != 0x20'0000 && size != 0x40'0000)
    return std::make_pair(image_error::INVALIDLENGTH, "Unsupported cartridge size (must be 32K, 512K, 1M, 2M or 4M)");
```

#### 3.2.7 磁盘上的"变体成型"形态

**单文件，无外挂头，无并存容器格式。**

| 扩展名 | 用途 | 出现于 |
|---|---|---|
| `.ngp` | 单色 NGP | No-Intro（NGP 集）、freedesktop MIME |
| `.ngc` | 彩色 NGPC | No-Intro（NGPC 集）、freedesktop MIME |
| `.npc` / `.ngpc` | 彩色别名 | Mednafen `TestMagic()` 接受 `ngp`/`ngpc`/`ngc`/`npc` |
| `.bin` | 通用 | MAME software list、TOSEC |

- **文件尺寸恒为 2 的幂**【实测】：No-Intro NGP `{65536, 524288, 1048576, 2097152}`、NGPC `{65536, 524288, 1048576, 2097152, 4194304}`，**非 2 的幂者 0 条**。65536 为 BIOS。
- ⚠ **Mednafen 只看扩展名，不做 magic 校验**（`neopop.cpp:161` 的 `TestMagic()`）。
- ⚠ **No-Intro 的 NGPC 集里 BIOS 用 `.ngp` 扩展名**（`[BIOS] SNK NeoGeo Pocket Color (World) (En,Ja).ngp`，65536 字节）—— **不要用扩展名决定平台，用 `0x23`**。
- **最后 16 KB 保留**（SNK 规格书逐字）：*"The last 1 block (16KB) of the software cassette is reserve for the system program. Please DO NOT use this area for program code, data or backup area."*

#### 3.2.8 DAT / 数据库覆盖

| 库 | 集合 | 条目 | 备注 |
|---|---|---|---|
| **No-Intro** | `SNK - NeoGeo Pocket`（20250904-215533） | **13** | 10 `.ngp` + 3 `.bin`；**9 条带 serial**；sha256 12/13 |
| **No-Intro** | `SNK - NeoGeo Pocket Color`（20260626-085623） | **128** | 127 `.ngc` + 1 `.ngp`（BIOS）；**118 条带 serial**；sha256 127/128 |
| **TOSEC** | NGP 2 个集合 | **31** | Games 29 / Firmware 2 |
| **TOSEC** | NGPC 7 个集合 | **371** | Games 311 / Demos 37 / Multimedia 8 / Samplers 8 / Applications 5 / Firmware 1 / Educational 1 |
| **MAME** | `hash/ngp.xml` **10** / `hash/ngpc.xml` **119** | 129 | serial `NeoP00070` 形式（10/10 与 97/119）；带 `release`/`alt_title`（日文原名） |
| **libretro-database** | `metadat/no-intro/` 9 + 115；`metadat/serial/` 3 + 77；`metadat/hacks/` NGP **404 不存在** / NGPC 3 条 | — | — |
| **libretro `metadat/headered/`** | **不存在** | — | 佐证无外挂头 |
| **Redump** | — | 0 | 卡带机 |

> 注意 No-Intro 官方命名是 **"SNK - NeoGeo Pocket"**（NeoGeo 连写），libretro 那份叫 "SNK - Neo Geo Pocket"（分写）。
> MAME 的 `<part name="cart" interface="ngp_cart">` 只有单个 `<dataarea name="rom">`（**无** width/endianness 属性，与 WS 不同），**没有** WS 那样的 sram/eeprom 附加 dataarea。
> `hash/ngp.xml` 头部注释记录了未 dump 项：`NEOP00060 Dokodemo Mahjong`。

哈希粒度：整文件，**headered == headerless**。

#### 3.2.9 中文汉化覆盖 —— ⭐ 本文 8 个平台里最多的一个

| 数据源 | `[tr zh]` | 具体条目 |
|---|---|---|
| **TOSEC `SNK Neo-Geo Pocket Color - Games`（311 条）** | **3** | `Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]`（CRC `4e3191c6`）<br>`Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]`（CRC `8aded757`）<br>`Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh][a]`（CRC `c2c5bcb4`） |
| TOSEC `SNK Neo-Geo Pocket - Games`（29 条） | **0** | 2 条翻译版是 `[tr es]` |
| No-Intro NGP / NGPC | **0** | 政策上不收翻译 hack |
| libretro `metadat/hacks/SNK - Neo Geo Pocket Color.dat` | **0** | 3 条全部 `[T-En]`（Nige-ron-pa、SNK vs. Capcom CFC2 Expand、Rockman Battle & Fighters） |
| MAME `hash/ngp.xml` / `ngpc.xml` | **0** | grep 无命中 |

TOSEC NGPC Games 的 26 条翻译版语言分布：`en` 14 / `pt` 6 / `fr` 3 / **`zh` 3**。

**结论【事实】：NGPC 是本文 8 个平台中中文汉化条目最多的一个 —— 公开数据库共 2 个汉化游戏、3 条 ROM 记录，全部来自 TOSEC。**

#### 3.2.10 实现成本评估

```rust
#[repr(C, packed)]
struct NgpHeader {              // 恰好 0x40 字节
    licence:     [u8; 28],      // 0x00
    start_pc:    [u8; 4],       // 0x1C  u32 LE, 使用时 & 0xFFFFFF
    catalog:     [u8; 2],       // 0x20  u16 LE, BCD
    sub_catalog: u8,            // 0x22
    mode:        u8,            // 0x23  0x00=mono 0x10=color
    name:        [u8; 12],      // 0x24  ASCII, 空格填充
    reserved:    [u8; 16],      // 0x30  应为 0
}

// 识别（推荐 RACE 的宽松写法，可容忍前 9 字节的变体/损坏）
let ok = &data[0x09..0x1C] == b" BY SNK CORPORATION";
// 或严格写法（file-format crate 的做法）
let ok = &data[0..28] == b"COPYRIGHT BY SNK CORPORATION"
      || &data[0..28] == b" LICENSED BY SNK CORPORATION";

let color  = matches!(data[0x23], 0x10);            // 0x00 = mono
let serial = format!("{:02X}{:02X}", data[0x21], data[0x20]);   // 直接对上 No-Intro
```

| 项 | 评估 |
|---|---|
| 核心代码量 | **约 50–80 行 Rust**，零外部依赖 —— **比 WS 简单得多** |
| 需要的表 | **无**（无发行商表、无容量表、无校验和） |
| 识别强度 | 开头 28 字节固定 magic，**误判率接近 0**；且可直接产出 No-Intro serial 做无哈希匹配 |
| 已知坑 | 3 个 ROM 的 `0x23` 被写错（Neo-Neo! PD、Cool Cool Jam SAMPLE、Dokodemo Mahjong），需按 catalog 特判；MAME 只接受 32K/512K/1M/2M/4M 五种尺寸 |
| **现成 Rust crate** | `file-format`（<https://github.com/mmalecot/file-format>，MIT/Apache-2.0，`src/signatures.rs:101-111`）已支持 `NeoGeoPocketRom` / `NeoGeoPocketColorRom` 的 **magic 检测**，但**不解析任何字段**（无 catalog / name / startPC 提取）。无任何 crate 提供完整 NGP 头解析 |
| 可参考实现 | Mednafen `ngp/neopop.h:69-83`（**最精确的 struct + `static_assert`**）；`ngp/rom.cpp::rom_display_header()`；libretro RACE `main.c::initRom()`（宽松识别 + 已知修正）；SNK `ngpcspec.txt` |
| **人日估算** | **0.2–0.3 人日** |

#### 3.2.11 WS 与 NGPC 工作量对比

| | WonderSwan | NGPC |
|---|---|---|
| 头位置 | **文件末尾 16 字节** | 文件开头 64 字节 |
| Magic | 无（只有 `0xEA` 弱指纹） | **有**，28 字节固定串 |
| 校验和 | **有**（末 2 字节，全和） | 无 |
| 查表需求 | 3 张表（发行商 43 / ROM 12 / 存档 9） | **0** |
| 能否直出 DAT serial | **否**（No-Intro WS DAT 无 serial 字段） | **能**（catalog BCD → No-Intro `serial`） |
| 源间分歧 | 存档码 `$01/$02/$05`、发行商 6 项、`$D` 语义 | 基本无 |
| 代码量 | ~120–180 行 | ~50–80 行 |
| 中文汉化 | 1 条（WSC） | **3 条 / 2 游戏**（NGPC） |

---

### 3.3 Atari Lynx

> ⭐ **Lynx 是本文唯一一个「同一游戏在 No-Intro 里以三套并存 DAT 收录」的平台**：`.lyx`（无头）、`.lnx`（含 64 字节头）、`.bll`（BS93 homebrew）。不做头剥离会漏掉一半。

#### 3.3.1 LNX 头（64 字节，magic `"LYNX"`）—— 六源一致

| 偏移 | 长度 | 字段名 | 含义 | 可靠性 |
|---|---|---|---|---|
| `0x00` | 4 | `magic[4]` | 固定 `"LYNX"`（`4C 59 4E 58`） | **极高（magic）** |
| `0x04` | 2 | `page_size_bank0` | Bank0 页大小，**小端 u16**；`0`=0K、`0x100`=64K、`0x200`=128K、`0x400`=256K、`0x800`=512K | 中（可与文件大小交叉校验） |
| `0x06` | 2 | `page_size_bank1` | Bank1 页大小，小端 u16 | 中 |
| `0x08` | 2 | `version` | 小端 u16，**必须 == 1** | **高（有效性判据）** |
| `0x0A` | 32 | `cartname[32]` | 卡带名，ASCII（写入侧最多 31 字符 + NUL） | 中–高 |
| `0x2A` | 16 | `manufname[16]` | 厂商名，ASCII（最多 15 字符 + NUL） | 中 |
| `0x3A` | 1 | `rotation` | 屏幕旋转，取值 0/1/2 | 低（**语义有源间分歧，见下**） |
| `0x3B` | 1 | `aud_bits` / AUDIN | `mAudinFlag = aud_bits & 0x01`（bit0 = 用 AUDIN 引脚做 bank 切换） | 低（2 源；早期实现视为 spare） |
| `0x3C` | 1 | `eeprom` | `1`=93C46、`2`=93C56、`3`=93C66、`4`=93C76、`5`=93C86；`0x40`=SD；`0x80`=8-bit 模式 | 低（3 源） |
| `0x3D`–`0x3F` | 3 | `spare[3]` | Handy 系称 spare；Gearlynx 定义 `0x3D`–`0x3E` reserved、`0x3F` = `sd_api`（0=GameDrive、1=ElCheapoSD） | `spare` 为【事实】；`sd_api` 语义【推断】（单源） |
| **合计** | **64** | | `HEADER_RAW_SIZE = 64` | **极高** |

**六个独立来源**：

| 来源 | 关键内容 |
|---|---|
| **Handy 原作者 K. Wilkins `make_lnx.c` V5** | `typedef struct { UBYTE magic[4]; UWORD page_size_bank0; UWORD page_size_bank1; UWORD version; char cartname[32]; char manufname[16]; char rotation; UBYTE spare[5]; } LYNX_HEADER_NEW;`，并有 `if(strlen(game)>31) … "(max 32)"` / `if(strlen(manuf)>15) … "(max 16)"`、`image_size=(256*page_size_bank0)+(256*page_size_bank1)` |
| **libretro-handy `lynx/cart.h`** | 最完整的旧式定义，含 `CART_AUDIN 1`、`CART_EEPROM_93C46 1` … `CART_EEPROM_93C86 5`、`CART_EEPROM_SD 0x40`、`CART_EEPROM_8BIT 0x80`。字节序交换仅在 `#ifdef MSB_FIRST` 下发生 → **文件为小端** |
| **Mednafen / beetle-lynx `lynx/cart.h` + `cart.cpp`** | `enum { HEADER_RAW_SIZE = 64 };`；`DecodeHeader()` 用 `MDFN_de16lsb()` 读三个 u16 → **明确小端**；校验 `if(magic != "LYNX" \|\| header.version != 1) { header_size = 0; … }` |
| **MAME `src/mame/atari/lynx_m.cpp`** | 注释 `// 64 byte header / LYNX / intelword lower counter size / 0 0 1 0 / 32 chars name / 22 chars manufacturer`；代码 `gran = header[4] \| (header[5] << 8);`（小端）、`size -= 0x40;` |
| **cc65 `libsrc/lynx/exehdr.s`** | `.byte 'L','Y','N','X'` / `.word __BANK0BLOCKSIZE__` / `.word __BANK1BLOCKSIZE__` / `.word 1` / 32 字节名 / 16 字节厂商 / `.byte 0 ; rotation 1=left / rotation 2=right` / `.byte 0,0,0,0,0 ; spare` |
| **Gearlynx `src/types.h`**（现代最完整） | `struct GLYNX_Cartridge_Header { u8 magic[4]; u16 bank0_page_size; u16 bank1_page_size; u16 version; char name[32]; char manufacturer[16]; u8 rotation; u8 audin; u8 eeprom; u8 reserved[2]; u8 sd_api; };` |
| **Holani（Rust）`src/cartridge/mod.rs`** | `const LNX_HEADER_LENGTH: usize = 64;`；切片 `[4..5]`=bank0、`[6..7]`=bank1、`[8..9]`=version、`[10..=41]`=title、`[42..=58]`=manufacturer、`[58]`=rotation、`[59..=63]`=spare；`fn eeprom() -> u8 { self.spare[1] }` → spare[1] = `0x3C` |

> ⚠ MAME 注释里的 "22 chars manufacturer" 与结构体的 16 字节不符 —— 22 = 16(厂商) + 1(rotation) + 5(spare)，属 MAME 注释笔误；其 `logerror` 里用 `header + 42`（= `0x2A`）作厂商起点，与 16 字节定义一致。
> ⚠ Holani 的 manufacturer 切片 `42..=58` 越界一格，是其自身的 off-by-one 小瑕疵。

**⭐ 两条独立的头长度实证**：

1. **No-Intro 官方 header skipper XML**（`header_lynx.zip` 内的 `No-Intro_LNX.xml`，作者 Yakushi~Kabuto，2007-04-08）：
```xml
<detector>
  <name>No-Intro Lynx Dat LNX Header Skipper</name>
  <author>Yakushi~Kabuto</author>
  <version>20070408</version>
  <rule start_offset="40">
    <data offset="0" value="4C594E58"/>
  </rule>
</detector>
```
`start_offset="40"` = `0x40` = **64 字节**；`4C594E58` = `"LYNX"`。

2. **SabreTools `Skippers/lynx.xml`**（作者 Roman Scherzer，独立实现）：
```xml
<detector>
  <name>Atari Lynx</name>
  <rule start_offset="40" end_offset="EOF" operation="none">
    <data offset="0" value="4C594E58" result="true"/>
  </rule>
  <rule start_offset="40" end_offset="EOF" operation="none">
    <data offset="6" value="425339" result="true"/>
  </rule>
</detector>
```
（第二条规则的 `425339` = `"BS9"`，指向 BS93 格式。）

3. **DAT 尺寸实证**【实测】：No-Intro `Atari - Atari Lynx (LNX)` 的 18 条尺寸只有 `262208`(×14)、`524352`(×2)、`131136`(×2)；`(LYX)` 的 155 条只有 `262144`(×88)、`131072`(×59)、`524288`(×7)、`512`(×1 BIOS)。**每一个 LNX 尺寸恰等于某个 LYX 尺寸 + 64。**

**rotation 语义分歧**（做识别时**只保存原始字节，不要下语义结论**）：

| 源 | `1` | `2` |
|---|---|---|
| `make_lnx.c`（Handy 原作者） | `CART_ROTATE_LEFT` | `CART_ROTATE_RIGHT` |
| cc65 `exehdr.s` 注释 | left | right |
| libretro-handy / beetle-lynx | `CART_ROTATE_LEFT=1` | `CART_ROTATE_RIGHT=2` |
| **Gearlynx `media.cpp`** | **RIGHT** | **LEFT** |
| Holani | `_270` | `_90` |

数值集合 {0,1,2}【事实】；`1=LEFT / 2=RIGHT`【推断】（4 源 vs 1 源，以 Handy 原始定义为准）。

#### 3.3.2 BLL / BS93 头（10 字节）

| 偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| `0x00` | 2 | `0x80 0x08` | 固定前导（Gearlynx 不匹配只告警，不拒绝） | 中（2 源） |
| `0x02` | 2 | `boot_address` | 加载/启动地址，**大端** | 高 |
| `0x04` | 2 | `size` | 总长度（**含这 10 字节头**），**大端** | 高 |
| **`0x06`** | 4 | **`"BS93"`** | 魔数 —— **在偏移 6，不在 0** | **极高（magic）** |

MAME `src/mame/atari/lynx.cpp` 逐字：
```c
u8 header[10]; // 80 08 dw Start dw Len B S 9 3
uint16_t const start  = header[3] | (header[2]<<8); //! big endian format in file format for little endian cpu
uint16_t const length = (header[5] | (header[4]<<8)) - 10;
```

⭐ **MAME `lynx_m.cpp::verify_cart()` 逐字**（回答"它到底检查哪些字符串"）：
```c
std::pair<std::error_condition, std::string> lynx_state::verify_cart(const char *header, int kind)
{
    if (kind)  // LYNX_QUICKLOAD
    {
        if (strncmp("BS93", &header[6], 4))
            return std::make_pair(image_error::INVALIDIMAGE, "This is not a valid Lynx image");
    }
    else       // LYNX_CART
    {
        if (strncmp("LYNX", &header[0], 4))
        {
            if (!strncmp("BS93", &header[6], 4))
                return std::make_pair(image_error::INVALIDIMAGE,
                        "This image is probably a Quickload image with .lnx extension\n"
                        "Try to load it with -quickload");
            else
                return std::make_pair(image_error::INVALIDIMAGE, "This is not a valid Lynx image");
        }
    }
    return std::make_pair(std::error_condition(), std::string());
}
```
→ 只检查两个字符串：**`"LYNX"@0x00`（卡带镜像）** 与 **`"BS93"@0x06`（quickload）**。

**"BLL" 的含义**：MAME `lynx.cpp` 文件头注释逐字 `info found in bastian schick's bll`；42Bastian（Bastian Schick）的仓库名为 `new_bll`。
> **"BLL = Bastian's Lynx Loader" 属【推断】——【未找到权威来源】明确给出这个缩写展开。**
Holani 内嵌了一段 246 字节的 `BLL_LOADER`（注释 `// Courtesy of https://github.com/42Bastian/new_bll/`），加载 BS93 时把 loader + 文件内容拼成 256 KB 卡带镜像。

#### 3.3.3 LNX2 头（magic `"LNX2"`，64 字节）—— **单源，慎用**

```c
struct GLYNX_Cartridge_Header_LNX2 { u8 magic[4]; u8 bank[4]; u16 version; char name[32];
  char manufacturer[16]; u8 rotation; u8 flags; u8 eeprom; u8 reserved; u8 custom; u8 sd_api; };
```
`0x00` magic `"LNX2"`；`0x04`–`0x07` 四个 bank 描述符（`type = bank >> 6`，0=UNUSED / 1=RAM_PERSISTENT / 2=RAM / 3=ROM）；`0x08` version(u16, ==1)；`0x0A` name[32]；`0x2A` manufacturer[16]；`0x3A` rotation；`0x3B` flags（bit0 = Lynx II）；`0x3C` eeprom；`0x3D` reserved；`0x3E` custom；`0x3F` sd_api。

> **可靠性：【推断】** —— 仅存在于 Gearlynx 源码，未在 Handy / Mednafen / MAME / cc65 / No-Intro / TOSEC 中找到对应支持；**未找到独立的格式规范文档**。Gearlynx 的探测顺序是 LNX2 → LYNX → BS93。

#### 3.3.4 磁盘上的"变体成型"形态

| 形态 | 扩展名 | 特征 | 证据 |
|---|---|---|---|
| **无头原始 ROM** | `.lyx` | 纯 ROM dump。大小恰为 `131072`/`262144`/`524288`（BIOS=512） | **No-Intro 主集**（155 条）；MAME `hash/lynx.xml` 的 ROM 也是裸 `.bin` |
| **带头卡带镜像** | `.lnx` | 64 字节 LYNX 头 + 原始 ROM。大小 = 2ⁿ + 64 | No-Intro（18 条）、**TOSEC 主集**（Games [LNX] 347 条） |
| **BS93 homebrew / quickload** | `.bll` / `.o` | 10 字节 BS93 头，小文件（几 KB–几十 KB） | No-Intro 用 `.bll`（3 条），TOSEC 用 `.o`（Games [O] 5 + Homebrew [O] 4） |
| **LNX2** | `.lnx` | 64 字节 `"LNX2"` 头 | 仅 Gearlynx【推断】 |

**`.lyx` 是真实的社区约定，且从一开始就存在**：K. Wilkins 1997 年 `make_lnx` 的 usage 逐字：
> `make_lnx cgames.lyx (Converts cgames.lyx to cgames.lnx)`

MAME 对 `.lyx` 有专门分支（因为无头，需靠大小猜 granularity），逐字注释：
```c
/* 2008-10 FP: FIXME: .lyx file don't have an header, hence they miss "lynx_granularity"
   (see above). What if bank 0 has to be loaded elsewhere? And what about bank 1?
   These should work with most .lyx files, but we need additional info on raw cart images */
if (size == 0x20000)      m_granularity = 0x0200;
else if (size == 0x80000) m_granularity = 0x0800;
else                      m_granularity = 0x0400;
```

**扩展名声明**：MAME cartslot `"lnx,lyx"` + `QUICKLOAD(config, "quickload", "o")`；beetle-lynx `MEDNAFEN_CORE_EXTENSIONS "lnx|lyx|bll|o"`。

⭐ **可直接照抄的三分支识别流程（Mednafen `system.cpp` 逐字）**：
```c
char clip[11];
file_read(fp, clip, 11, 1);
clip[4]=0; clip[10]=0;
if(!strcmp(&clip[6],"BS93"))       mFileType=HANDY_FILETYPE_HOMEBREW;
else if(!strcmp(&clip[0],"LYNX"))  mFileType=HANDY_FILETYPE_LNX;
else if(fp->size==128*1024 || fp->size==256*1024 || fp->size==512*1024)
    /* Invalid Cart (type). but 128/256/512k size -> set to RAW and try to load raw rom image */
    mFileType=HANDY_FILETYPE_RAW;
```

其它实现的探测顺序：
- **Gearlynx**：`size>0x40 && "LNX2"` → LNX2；`size>0x40 && "LYNX"` → LNX；`size>10 && "BS93"@6` → homebrew；否则 `DefaultLynxHeader()`（`bank0 = (size+255)>>8`）
- **Holani**：`is_lnx` → `is_bs93` → `is_nointro`（**对整文件算 MD5 查内置 No-Intro 表**）→ 都失败则报 `"Couldn't identify cart file format."`
- **beetle-lynx** 还有一层：无论有无头，都对**去头后**的数据算 CRC32，查内置 90 条 `lynxDB` 表（`{crc32, name, filesize, rotation, ...}`）来补 rotation / page size —— **这与第一部分 Mesen 用 CRC32 查库补 NES 头的做法同构**

#### 3.3.5 DAT / 数据库覆盖

**No-Intro 把 Lynx 拆成三个 DAT**（`<header><id>` 均为 **30**，即同一平台的三种格式集，均带 `<clrmamepro forcenodump="required"/>`）：

| DAT | 版本 | 条目 | 扩展名 | 尺寸分布 |
|---|---|---|---|---|
| `Atari - Atari Lynx (LYX)` | 20260625-122811 | **155** | 全 `.lyx` | 262144×88、131072×59、524288×7、512×1 |
| `Atari - Atari Lynx (LNX)` | 20260625-122811 | **18** | 全 `.lnx` | 262208×14、524352×2、131136×2 |
| `Atari - Atari Lynx (BLL)` | 20260625-122811 | **3** | 全 `.bll` | Mines 2 (Demo) 5087 B、T-Tris 15914 B、T-Tris (Alt) 15700 B |

> ⚠ **LNX 集不是 LYX 集的"含头副本"** —— 两者内容基本不重叠，LNX 集收的是**只以 LNX 形式存在的 dump**（Atari Lynx Collection 1、T-Tris、Zaku、Mines 2、Puzzler 2000、Raiden Beta 等）。

**同一游戏两种形态的哈希完全不同**【实测】：
```
A.P.B. (USA, Europe).lyx   size 262144  crc F6FB48FB  md5 B425941149874C6371C40E85CC5B6241
A.P.B. (USA, Europe).lnx   size 262208  crc 68A0ACAB  md5 c302613d0e837cab485ddcd8f4d8ea5e
```
No-Intro 官方靠 clrmamepro 的 **header skipper XML** 解决（见 3.3.1）。`auto-datfile-generator` README 逐字：
> "Some No-Intro dats require an extra XML file to detect headers. Download the following zips, extract them and place the XML files in clrmamepro's `headers` folder: … Atari Lynx …"

| 其它库 | 集合 | 条目 |
|---|---|---|
| **TOSEC** | 11 个集合 | **677**（Games [LNX] 347 / Demos [LNX] 191 / Homebrew [LNX] 103 / Games [LYX] 10 / Games [O] 5 / Applications 9 / Homebrew [O] 4 / Firmware 2 / Compilations 2+2 / Demos [Multipart] 2）。**TOSEC 以带头 `.lnx` 为主，与 No-Intro 正好相反** |
| **libretro `metadat/no-intro/Atari - Lynx.dat`** | 2026.08.01 | **681**（`.lyx` 364 / `.lnx` 235 / `.bll` 82；其中 **518 条含 "Aftermarket"**） |
| **libretro `metadat/headered/Atari - Lynx.dat`** | 2019.12.27 | **85**，全 `.lnx`，头部注释 `comment "libretro No-Intro Atari - Lynx Headered."` |
| **libretro `metadat/tosec/Atari - Lynx.dat`** | 2026.08.01 | **308**（`.lnx` 293 / `.lyx` 10 / `.o` 5） |
| **MAME `hash/lynx.xml`** | master | **119** 个 software；ROM 扩展 `.bin` 116 / `.lyx` 1 / `.u2` 1 → **无头**；size 全为 131072/262144/524288；`<info name="serial">` **79** 条（如 `PA2042`）；feature `granularity` = 1024(74)/512(37)/2048(8)、`rotation` = LEFT(4)/RIGHT(3)、`audin_offset` = 262144(**仅 1 条**) |
| **Redump** | — | 0（卡带机） |

> libretro 的 681 条 > No-Intro 官方三集之和 176 条，原因是 libretro 的源（`robloach/libretro-dats`）纳入了 No-Intro 的 Aftermarket 集，而本次镜像的 334-DAT 包里没有单独的 Lynx Aftermarket DAT。**两个数字都是实测值。**
> ⚠ libretro `metadat/serial/Atari - Lynx.dat` 里的 `serial "PA2078"` 是 **Atari 的印刷料号**，不是从 ROM 里读出来的 —— LNX 头里没有序列号字段。No-Intro 的三个 Lynx DAT **均无 `serial` 属性**【实测】。

#### 3.3.6 中文汉化覆盖

**零。**【实测】对 8 个 DAT 分别 grep `Chinese` / `Zh` / `[tr` / `汉化` / `China` / `Taiwan`：

| 文件 | Chinese | Zh | `[tr` | 汉化 |
|---|---|---|---|---|
| No-Intro `(LYX)` / `(LNX)` / `(BLL)` | 0 | 0 | 0 | 0 |
| libretro `no-intro/Atari - Lynx`（681） | 0 | 0 | 0 | 0 |
| libretro `tosec/Atari - Lynx`（308） | 0 | 0 | 0 | 0 |
| libretro `headered/Atari - Lynx`（85） | 0 | 0 | 0 | 0 |
| TOSEC `Games - [LNX]`（347） | 0 | 0 | **1** | 0 |
| TOSEC 其余 Lynx 集 | 0 | 0 | 0 | 0 |

TOSEC 全部 Lynx DAT 中的唯一翻译标记：`Switchblade II (1992)(Atari Corp)[tr es Wave]`（西班牙语）。

**结论【事实】：Atari Lynx 在 No-Intro / libretro-database / TOSEC / MAME 中，中文汉化条目数为 0。**

#### 3.3.7 实现成本评估

需要解析的东西：
1. 前 4 字节 `"LYNX"` / `"LNX2"`，或偏移 6 的 `"BS93"` —— 纯常量比较
2. LNX 头：3 个**小端** u16 + 2 个定长字节串 + 4 个单字节。无变长、无压缩、无校验和
3. BS93 头：2 个**大端** u16
4. 无头 `.lyx` 兜底：按文件大小分类（512 / 131072 / 262144 / 524288）

⭐ **比解析本身更重要的三个工程点**：
1. **必须同时计算含头与去头两套哈希**（CRC32/MD5/SHA1）—— No-Intro 主集是无头 `.lyx`，而 TOSEC 与 libretro-headered 集是含头 `.lnx`，差 64 字节就完全对不上（实测 A.P.B. 两套 CRC 无任何关系）。这与第一部分 D.1 对 NES/SNES 的结论完全一致。
2. **BS93 去头是 10 字节，不是 64。**
3. **`version` 必须 == 1 才认头**，否则会把恰好以 `LYNX` 开头的裸 ROM 误判。

| 项 | 评估 |
|---|---|
| 代码量 | **约 150–250 行 Rust** |
| **现成 Rust crate** | **无任何 LNX/Lynx 头解析 crate**（crates.io 搜 `lynx` / `lnx` 全是 HTTP 客户端、搜索引擎等无关物）。**但 `file-format` crate 支持 `AtariLynxRom` 的 magic 检测**（freedesktop MIME 也有：`<match type="string" value="LYNX" offset="0"/>`） |
| ⭐ 最佳参考实现 | **Holani**（**Rust**，MIT/GPL，<https://github.com/LLeny/holani>）—— `src/cartridge/lnx_header.rs` + `mod.rs` + `no_intro.rs`（内置 ~90 条 MD5→(名称, rotation) 表）。**未发布到 crates.io**，需 git 依赖或直接复制 |
| 其它参考 | **Gearlynx**（C++，字段最全：AUDIN/EEPROM/sd_api/LNX2）；**libretro-handy** `lynx/cart.h`（旧式最完整定义 + 内置 CRC 数据库）；Mednafen `lynx/system.cpp`（三分支识别）；MAME `lynx_m.cpp::verify_cart` |
| 配套 crate | `datary` v0.3.0（No-Intro/Logiqx/Redump/TOSEC DAT 读写）；`shiratsu-naming` v0.1.7（No-Intro/TOSEC/GoodTools 文件名解析） |
| **人日估算** | **0.4 人日**（含三种形态判定 + 双哈希），含 DAT 接入 **0.6 人日** |

---

### 3.4 N-Gage

> ⚠ **本文唯一一个建议直接跳过的平台。** No-Intro 只有 **1** 条（停更于 2022-02），TOSEC 只有 **5** 条（全是同一款原型），libretro-database **完全没有**。约 60 款商业游戏在三大库中基本无覆盖。
> 而与此同时，它的**解析成本是本文老平台里最高的** —— Symbian SIS/SISX 是带压缩的递归字段树，MMC 镜像还要实现 FAT。

#### 3.4.1 磁盘上的"变体成型"形态 —— 四种

**形态 A：MMC 整卡镜像（`.img` / `.bin`）—— 原版 N-Gage 游戏卡的正规 dump**

保存社区自己的 dump 指南 [dumping.guide《Nokia N-Gage》](https://dumping.guide/todo/carts/nokia/n-gage) 逐字：
> "Two disk images from different carts of the same game version seem to match."
```xml
<machine name="THPS (HB28H016MM2 JPN A103 DF69070 NOKIA V1.0 0630728)">
  <rom name="THPS.img" size="16056320" crc="ce8ae096" md5="3ffcf3f3ac96d716fd57a88849563cb8" .../>
</machine>
```
工具：**aaru**（可 dump 卡的 **CID / CSD / OCR**）、**dd**、**MMCDUMP**（N-Gage homebrew，<https://github.com/EKA2L1/Akudama>，能 dump aaru 拿不到的 CID）。

**镜像内部是 FAT 文件系统**。EKA2L1 快速上手页逐字：
> "N-Gage game files sit on MMC card, unprotected in FAT32 filesystem."

**形态 B：解包后的目录树（`System/Apps/<GameName>/`）**

TOSEC `Nokia N-Gage - Games - [Multipart]`（2024-07-03）直接证实 —— 1 个 game、**2933 个 rom 条目**，全部路径以 `System\` 开头：
```xml
<game name="8 Kings (2003-10-23)(Argonaut)(beta)">
  <rom name="System\Apps\8Kings\8Kings.aif"          size="1998" crc="e9ecbdb1" .../>
  <rom name="System\Apps\8Kings\8Kings.app"          size="460"  crc="888583c0" .../>
  <rom name="System\Apps\8Kings\8Kings_caption.rsc"  size="40"   crc="16e1c97b" .../>
  <rom name="System\Apps\Examples\Example4.exe"      size="1431960" .../>
```
扩展名分布【实测】：`.ASb`/`.asb` 904、`.TEX`/`.tex` 886、`.ani` 512、`.SKB` 460、`.cut` 90、`.smp` 51、`.txt` 13、`.pat` 5、`.ini` 4、`.csv` 3，加上 `.aif`/`.app`/`.rsc`/`.exe`/`.tmp` 各 1。

⭐ **EKA2L1 的扫描规则**（`services/applist/applist.cpp` 逐字，可直接照抄）：
```c
static const char *OLDARCH_REG_FILE_EXT = ".aif";   // EKA1 = S60v1/v2 = N-Gage
static const char *NEWARCH_REG_FILE_EXT = ".r??";   // EKA2 = S60v3+
...
const std::u16string base_dir = std::u16string(1, drive_to_char16(drv)) + u":\\System\\Apps\\";
// 对 base_dir 下每个子目录 <Name>，取 <Name>.aif 作为注册文件
reg.mandatory_info.app_path = eka2l1::replace_extension(path, u".app"); // It seems so.
```
→ 规范形态：`<drive>:\System\Apps\<Name>\` 内含 `<Name>.aif`（注册/图标）+ `<Name>.app`（E32Image DLL 型可执行体）+ `<Name>_caption.rsc`（本地化标题资源）+ 游戏私有数据。

EKA2L1 挂载说明逐字：
> "Mount the game folder. The root of the game folder should contains a System folder. … Note that the Folder field shows the name of the current folder. If the field shows System, it is not valid."

**形态 C：`.sis` / `.sisx` 安装包**
**形态 D：N-Gage 2.0 的 `.n-gage` 文件**（2008 年，S60v3 / Symbian 9.1–9.3，拷入 `E:\n-gage\` 后由 N-Gage 应用安装）
> **可靠性【推断】** —— 来源为 Emulation General Wiki / RetroBat Wiki 等社区文档，**未找到 Nokia 官方规范或模拟器源码级定义**（EKA2L1 全仓库 grep `ngage` / `n-gage` 命中数为 **0**，其 N-Gage 支持是作为普通 Symbian 应用实现的）。**`.n-gage` 文件的内部结构：【未找到权威来源】。**

⚠ **DRM 严重影响可识别性。** EKA2L1 逐字：
> "**DRM-ed game**: These games rely on the presence of a physical MMC card to check for its **MMC-ID**, to see if the game is contained on a copied-card or not sit on a MMC card at all. The executable of the game is obsfucated so that reverse engineering is harder. **Most of N-Gage libraries do this.**"

→ 原版 N-Gage 游戏的可执行体被混淆，且校验卡片 **MMC-ID（CID）**。EKA2L1 用 `config.yml` 的 `mmc-id:` 键模拟。**这意味着完整保存需要「卡片镜像 + CID 元数据」，仅靠文件哈希不足。**

#### 3.4.2 可识别字段表

**（一）旧版 SIS（EPOC R3/R4/R5/R6 —— N-Gage 原版游戏所属）**

来源：Alexander Thoukydides《SIS File Format》v1.19（<https://thoukydides.github.io/riscos-psifs/sis.html>，EKA2L1 源码顶部逐字注明 *"This old SIS installer and its code is based on this document from Mr. Thoukydides"*），与 EKA2L1 `sis_old.h` 的 `struct sis_old_header` 完全一致。

页面 Conventions 逐字：*"Numeric values larger than a byte are stored with the **least significant byte first**."* / *"Pointers are specified as offsets from the start of the SIS file."*

| 偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| **`0x00`** | 4 | **UID 1** | **应用 UID**；若无应用则为 `0x10000000` | **极高** |
| `0x04` | 4 | **UID 2** | `0x1000006D` = EPOC 3/4/5；`0x10003A12` = EPOC 6 | **极高（格式判别）** |
| `0x08` | 4 | **UID 3** | 恒为 `0x10000419` | **极高（magic）** |
| `0x0C` | 4 | UID 4 | UID 校验和 | 高 |
| `0x10` | 2 | Checksum | 整文件 CRC16（排除自身 2 字节与签名块） | 中 |
| `0x12` / `0x14` / `0x16` | 2 ×3 | 语言数 / 文件数 / 依赖数 | | 中 |
| `0x18` / `0x1A` / `0x1C` | 2 ×3 | 安装语言 / 安装文件数 / 安装盘符 | 盘符初值 `0x0000`(R3-5) / `0x0021`(R6, 字符 `!`) | 中 |
| `0x1E` | 2 | 能力数 | | 低 |
| `0x20` | 4 | Installer version | 68(`0x44`)/100(`0x64`) = R3-5；200(`0xC8`) = R6 | 高 |
| `0x24` | 2 | Options | `0x0001` IsUnicode / `0x0002` IsDistributable / `0x0008` NoCompress / `0x0010` ShutdownApps | 中 |
| `0x26` | 2 | Type | `0`SA `1`SY `2`SO `3`SC `4`SP `5`SU | 中 |
| `0x28` / `0x2A` | 2 ×2 | Major / Minor version | | **高** |
| `0x2C` | 4 | Variant | 通常 `0x00000000` | 低 |
| `0x30` – `0x40` | 4 ×5 | Languages / Files / Requisites / Certificates / **Component name** pointer | **Component name pointer 指向包名** | **高** |
| **基础头合计** | **68 (`0x44`)** | | | |
| `0x44` – `0x54` | | Signature / Capabilities pointer、Installed space、Max installed space、Reserved[16] | **仅 EPOC R6 扩展头** | |
| **R6 头合计** | **100 (`0x64`)** | | | |

**UID 定义逐字**：
> "The UID 1, UID 2 and UID 3 fields are the first three words of the file, and indicate the type of data it contains. **UID 1 is the UID of the application to be installed**, or 0x10000000 if none. UID 2 is 0x1000006D for EPOC releases 3, 4 and 5, and 0x10003A12 for EPOC release 6. UID 3 is always 0x10000419."

**UID 4 校验算法逐字**：
> "UID 4 is a checksum calculated from the preceding fields. The least significant 16 bits are given by the CRC16 of the bytes at **even** offsets from the start of the file, and the most significant 16 bits are given by the CRC16 of the bytes at **odd** offsets from the start of the file."
> （CRC 定义：`x^16 + x^12 + x^5 + 1`，初始余数为 0，即 **CRC-16-CCITT**。）

**→ 旧 SIS 里"应用 UID"在偏移 `0x00`（UID1）。**

EKA2L1 `loader/sis.cpp` 的判型：
```c
enum class epoc_sis_type { epocu6 = 0x1000006D, epoc6 = 0x10003A12 };
...
if ((uid2 == epocu6) || (uid2 == epoc6)) return sis_type_old;
```

**（二）SISX / SIS v9（Symbian OS 9.x）**

来源：**Symbian 官方开源码** `oss.FCL.sf.mw.appinstall / secureswitools/swisistools/source/sisxlibrary/siscontents.{h,cpp}`（Nokia，EPL v1.0）。

`siscontents.h` 逐字：
```c
const unsigned long KUidAppDllDoc16 	= 0x10003A12;
const unsigned long KUidSISXApp 		= 0x10201A7A;
const unsigned long KUidLegacySisFile 	= 0x10000419;
```
`siscontents.cpp::OutputHeaderUids` 逐字：
```c
uid [0] = KUidSISXApp;
uid [1] = 0;				// Reserved for future use
uid [2] = UID1 ();
uid [3] = UidChecksum (uid [0], uid [1], uid [2]);
```

| 偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| **`0x00`** | 4 | UID1 | 恒为 **`0x10201A7A`**（`KUidSISXApp`） | **极高（magic）** |
| `0x04` | 4 | UID2 | 恒为 `0`，官方注释 "Reserved for future use" | 高 |
| **`0x08`** | 4 | UID3 | **package UID** | **极高** |
| `0x0C` | 4 | UID checksum | `(CRC16(奇数偏移字节) << 16) + CRC16(偶数偏移字节)`，覆盖前 12 字节 | 高 |
| `0x10`… | | SISContents 字段树 | type/length 变长字段，**内容 zlib deflate 压缩** | — |

`siscontents.h` 的 doc 注释逐字定义 UID 语义：
> "@param aUid1 - UID of the application associated with SIS Files (0x10201A7A)
>  @param aUid2 - This UIDF is reserved for possible future use.
>  @param aUid3 - **This is the package UID which uniquely identifies a SIS file**, except for the case of upgrades, where both SIS files will share the same UID3."

⭐ **新旧格式判别**（官方 `CSISContents::IsSisFile` 逐字）：
```c
CSISUid::TUid uid [4];
aStream.seek (0);
aStream >> uid [0] >> uid [1] >> uid [2] >> uid [3];
bool isSisFile = false;
if( uid [0] == KUidSISXApp )                { isSisFile = true; }
else if(uid[2] == KUidLegacySisFile)        { throw ... L"file is a unspported legacy SIS file."; }
```
→ **新格式看 `UID1 == 0x10201A7A`；旧格式看 `UID3 == 0x10000419`。**

⚠ **扩展名不可靠**【实测】。EKA2L1 仓库自带的 `native/BitmapTest/sis/BitmapTest.sis` 与 `BitmapTest.sisx` 前 16 字节 hexdump：
```
BitmapTest.sis : 7a1a 2010 0000 0000 18a1 8fed 9721 109a
BitmapTest.sisx: 7a1a 2010 0000 0000 18a1 8fed 9721 109a
```
两者 UID1 都是 `0x10201A7A`（SISX 格式）。**`.sis` 扩展名下也可能是 SISX 格式，必须读 UID1。**

**SIS stub 特例**：EKA2L1 `sis.cpp`：若首个 u32 == `sis_field_type::SISController`（=13），则是 ROM stub SIS（**无 16 字节头**，直接从 SISController 开始）。

**（三）E32Image（`.app` / `.exe` / `.dll`）**

Symbian 官方 `sisxlibrary/utility.h` 逐字：
```c
const TInt32 KDynamicLibraryUid = 0x10000079;
const TInt32 KExecutableImageUid = 0x1000007a;
```
EKA2L1 `e32img.cpp` 逐字：
```c
static constexpr std::uint32_t E32IMG_SIGNATURE = 0x434F5045;   // 'EPOC'
bool is_e32img(common::ro_stream *stream, std::uint32_t *uid_array) {
    stream->read(&uid_array[0], 4);   // uid1
    stream->read(&uid_array[1], 4);   // uid2
    stream->read(&uid_array[2], 4);   // uid3
    stream->read(&check, 4);          // uid checksum
    stream->read(&sig, 4);            // signature
    const bool result = (sig == E32IMG_SIGNATURE);
```

| 偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| `0x00` | 4 | UID1 | `0x1000007A` = EXE，`0x10000079` = DLL（**`.app` 为 DLL 型**） | 极高 |
| `0x04` | 4 | UID2 | 子类别 | 中 |
| **`0x08`** | 4 | UID3 | **该可执行体的唯一 UID** | **极高** |
| `0x0C` | 4 | UID checksum | | 高 |
| **`0x10`** | 4 | signature | **`0x434F5045` = ASCII `"EPOC"`** | **极高（magic）** |
| `0x14`+ | | header_crc, major/minor, compression_type, petran 版本, 时间戳, flags, code_size, data_size, heap min/max, stack_size, bss_size, entry_point, code_base, data_base, … | 见 `struct e32img_header` | — |

> **【未找到权威来源】** `.app` 的 UID2 具体常量（如常被引用的 `KUidApp = 0x100039CE`）—— 在 EKA2L1 与 Symbian appinstall 两个仓库中均未 grep 到该常量定义。`sisfiledata.cpp` 只有一句注释 `// or for executables the UID value has to be set to KUidApp or NULL`，未给出数值。

**（四）⭐ AIF 注册文件（`.aif`）—— N-Gage 时代的应用注册，最实用的锚点**

EKA2L1 `services/applist/registeration.cpp` 逐字：
```c
static std::int8_t get_aif_version_from_uids(epoc::uid_type &uids) {
    static constexpr std::uint32_t DIRECT_STORE_UID = 0x10000037;
    static constexpr std::uint32_t AIF_UID_1        = 0x1000006A;
    static constexpr std::uint32_t AIF_V2_UID       = 0x101FB032;
    if ((uids[0] == DIRECT_STORE_UID) && (uids[1] != AIF_UID_1)) return 1;
    else if (uids[0] == AIF_V2_UID) return 2;
    return -1;
}
...
reg.mandatory_info.uid = uids[2];
```

| 偏移 | 长度 | 字段 | 含义 | 可靠性 |
|---|---|---|---|---|
| `0x00` | 4 | UID1 | `0x10000037`（AIF v1, DirectFileStore）或 `0x101FB032`（AIF v2） | **极高（magic）** |
| `0x04` | 4 | UID2 | v1 判定要求 != `0x1000006A` | 高 |
| **`0x08`** | 4 | **UID3** | **应用 UID**（`reg.mandatory_info.uid = uids[2]`） | **极高** |
| `0x0C` | 4 | UID checksum | EKA2L1 注释 `// Read the checksum. TODO: crc32`（未实现校验） | 低 |

> ⭐ **对一个 N-Gage 游戏目录做识别，最稳的锚点是 `System\Apps\<Name>\<Name>.aif` 偏移 `0x08` 的 4 字节应用 UID。**

**（五）Symbian UID 分配范围**

Symbian Developer Library 逐字：
> "UID1 specifies the **category** of an object"（构建工具自动设置）
> "UID2 specifies a **sub-category** of an object"（在 mmp 中手工指定）
> "UID3 identifies a **particular object**, e.g. a particular exe file or a particular dll file"

UID3 分配区间：

| 区间 | 用途 | 可靠性 |
|---|---|---|
| `0xE0000000`–`0xEFFFFFFF` | 测试/开发，**无需向 Symbian Signed 申请** | 【事实】官方文档 |
| `0xA0000000`–`0xAFFFFFFF` | 非保护区（unprotected），自签名分发 | 【事实】 |
| `0x20000000`–`0x2FFFFFFF` | 保护区（protected），需 Symbian Signed | 【事实】 |
| `0x10000000`–`0x1FFFFFFF` | 旧方案（pre-Platform-Security）由 Symbian 分配的 UID —— **N-Gage 时代（2003, Symbian 6.1/7.0）的游戏 UID 落在这里** | 【推断】 |

> ⚠ **重要限定**：上述"保护/非保护"区间是 **Symbian OS 9.x Platform Security（2005 年后）**的政策。N-Gage 原版（S60v1/v2，Symbian 6.1/7.0）**早于 Platform Security**，当时没有 protected range 概念。
> ⚠ **常见的 `0x1000 0000`–`0x0FFF FFFF` 这个写法不成立（上界小于下界）——【未找到权威来源】支持它。**

#### 3.4.3 DAT / 数据库覆盖

| 项目 | 有 N-Gage？ | 条目 | 详情 |
|---|---|---|---|
| **No-Intro** | **有**（WIP） | **1** | `Nokia - N-Gage (WIP) (20220220-010530).dat`，`<id>219</id>`，author `SonGoku`，`forcenodump="required"` |
| **No-Intro `Mobile - Symbian`** | 相关 | **27** | `<id>124</id>`，author `NovaAurora`，20220516-232715，全 `.sis`（Aqua Blocks+、Frozen Bubble、Space Impact、Sugatris、V-Rally CE 等，**均为通用 Symbian 游戏而非 N-Gage 独占**） |
| **TOSEC** | **有** | **5** | `Nokia N-Gage - Games - [SIS]` **4** games（**全是同一款原型 `8 Kings`**）+ `Nokia N-Gage - Games - [Multipart]` **1** game / 2933 files |
| **TOSEC-PIX** | 有 | 3 | `Nokia N-Gage - Manuals - Games`（PDF） |
| **TOSEC Symbian 集** | **无** | 0 | 4502-DAT 全库 grep `symbian` 零命中 |
| **libretro-database** | **无** | **0** | 全仓 grep `gage` 仅 2 处误匹配（"En**gage**ment"、"Gun**gage**"）。有 `metadat/no-intro/Mobile - Symbian.dat` **17** 条 |
| **Redump / MAME** | 无 | 0 | MAME `hash/ngage.xml` **HTTP 404** |
| **RetroAchievements** | 部分 | — | `rc_consoles.h` 有 `RC_CONSOLE_NOKIA_NGAGE = 61`，但 `hash.c` **零实现** |

**No-Intro N-Gage DAT 全文**（就这么长）：
```xml
<datafile>
	<header>
		<id>219</id>
		<name>Nokia - N-Gage (WIP)</name>
		<version>20220220-010530</version>
		<author>SonGoku</author>
		<clrmamepro forcenodump="required"/>
	</header>
	<game name="Worms World Party (USA) (En,Fr,De,Es,It)" id="0001">
		<rom name="Worms World Party (USA) (En,Fr,De,Es,It).bin" size="33546240"
		     crc="8409a30b" md5="6a920858110d3f5dd0b866dc8909d846"
		     sha1="19504bc3b9ba2e8de7aca3d0df55fa0374583e17"
		     sha256="e1d29525aefe5596b016d6656b2fb36719fc8420d6056f2348be9c79a21210b1"
		     serial="0710102"/>
	</game>
</datafile>
```
即 32 MB（33 546 240 B）整卡镜像，`serial` = 卡片料号。

TOSEC `[SIS]` 集的全部 4 条：
```
8 Kings (2003-05-09)(Argonaut)(proto)[E3 build].sis     1222626  crc a8f67524
8 Kings (2003-06-05)(Argonaut)(proto).sis               1924555  crc ac8a466f
8 Kings (2003-08-07)(Argonaut)(proto)(Part 1 of 2).sis  2161430  crc 35f8f4e9
8 Kings (2003-08-07)(Argonaut)(proto)(Part 2 of 2).sis  2346690  crc 8f51b670
```

#### 3.4.4 中文汉化覆盖

**零。**【实测】

| 文件 | Chinese | Zh | `[tr` | 汉化 | China | Taiwan |
|---|---|---|---|---|---|---|
| No-Intro `Nokia - N-Gage (WIP)`（1 条） | 0 | 0 | 0 | 0 | 0 | 0 |
| No-Intro `Mobile - Symbian`（27 条） | 0 | 0 | 0 | 0 | 0 | 0 |
| TOSEC `N-Gage - Games - [SIS]`（4 条） | 0 | 0 | 0 | 0 | 0 | 0 |
| TOSEC `N-Gage - Games - [Multipart]`（2933 files） | 0 | 0 | 0 | 0 | 0 | 0 |
| libretro `Mobile - Symbian`（17 条） | 0 | 0 | 0 | 0 | 0 | 0 |

**结论【事实】：N-Gage 与 Symbian 在 No-Intro / libretro-database / TOSEC 中，中文汉化条目数为 0。**

#### 3.4.5 实现成本评估

| 层级 | 内容 | 成本 | Rust 现状 |
|---|---|---|---|
| **L0 快速指纹**（推荐先做） | 读前 16 字节：`0x10201A7A`→SISX；`UID3==0x10000419`→旧 SIS；`UID2∈{0x1000006D, 0x10003A12}`→旧 SIS 且知 EPOC 代；`sig@0x10=='EPOC'`→E32Image；`UID1@0x00∈{0x10000037, 0x101FB032}`→AIF | **极低**（~80 行） | 手写即可 |
| **L1 旧 SIS 元数据** | 68/100 字节定长头 + 按 `Component name pointer` / `Languages pointer` 跳转读包名。全部小端定长，**无压缩**。可选 CRC-16-CCITT 校验 | **低**（~300 行） | `crc16` v0.4.0 或 `crc` |
| **L2 SISX 元数据** | 16 字节头 + **递归 type/length 字段树（41 种 field type）**；`SISController` 被 `SISCompressed`(deflate) 包裹，取包名/UID/版本必须先 inflate | **中高**（~1500 行） | `flate2` v1.1.10；**无现成 SIS crate** |
| **L3 MMC 镜像** | 解析 FAT16/FAT32，枚举 `System/Apps/*/`，读 `.aif` 的 UID3 | **中**（~400 行 + crate） | `fatfs` v0.3.6 或 `fat32` v0.2.0 |
| **L4 DRM / CID** | 需要卡片 CID/CSD/OCR 元数据（**不在文件里**），需自定义容器（如 aaru 格式） | **很高 / 部分不可行** | 无 |

**Rust 生态【实测】**：crates.io 搜 `symbian` → **0 结果**；搜 `sisx` → **0 结果**；搜 `sis` → 全是无关物（Sui 登录、学生信息系统…）；搜 `e32` → 全是 EByte E32 LoRa 模块驱动。
**→ Rust 生态里没有任何 Symbian SIS/SISX/E32Image 解析 crate。必须自己写或做 FFI。**

**可直接参考的实现**：

| 实现 | 语言 | 覆盖 |
|---|---|---|
| **Symbian 官方 `swisistools/sisxlibrary`**（<https://github.com/SymbianSource/oss.FCL.sf.mw.appinstall>，EPL-1.0） | C++ | SISX 完整读写，含 `UidChecksum`、`IsSisFile`、全部 field 类型 |
| **EKA2L1 `src/emu/loader/`**（<https://github.com/EKA2L1/EKA2L1>，GPL-3.0） | C++ | `sis.cpp`(判型) / `sis_fields.{h,cpp}`(SISX) / `sis_old.{h,cpp}`(旧 SIS) / `e32img.{h,cpp}` |
| **EKA2L1 `services/applist/registeration.cpp`** | C++ | AIF v1/v2 解析、`System\Apps` 扫描 |
| **Thoukydides SIS 规范 v1.19** | 文档 | 旧 SIS 全字段 + 语言/文件/依赖记录 |
| **MMCDUMP / Akudama** | N-Gage homebrew | 卡片 CID dump |

**⭐ 建议的优先级**：
1. **N-Gage 只做 L0 + L1（旧 SIS）** —— 覆盖 No-Intro `Mobile - Symbian` 27 条 + TOSEC `[SIS]` 4 条。约 **1 人日**。
2. L3（FAT 镜像）视需求 —— 覆盖 No-Intro WIP 的 1 条整卡 dump 与 TOSEC `[Multipart]` 的 1 条。约 **+1.5 人日**。
3. **L2（SISX）与 L4（DRM）暂缓** —— 投入产出比在 N-Gage 场景下很差（DAT 覆盖本来就只有个位数）。

| 方案 | 人日估算 |
|---|---|
| 只做 L0（魔数判型，能说清"这是什么文件"） | **0.3 人日** |
| L0 + L1（旧 SIS 元数据） | **1 人日** |
| L0 + L1 + L3（含 MMC/FAT 镜像） | **2.5 人日** |
| 全套（含 SISX 字段树） | **6+ 人日，不建议** |

> **总评：以 8 个平台的横向性价比看，N-Gage 应当排在最后，或直接跳过。**

---

### 3.5 3DO Interactive Multiplayer

> ⭐ **本节的证据链是全文最强的**：3DO 的 Portfolio OS **原始源码已公开**，`DiscLabel` 结构体带完整字段名与注释，另有 3 个独立实现（rcheevos、3dt、opticaldiscs）与之逐字节吻合。
> ⚠ **但同时也是"看起来最有用、实际最不可靠"的一节** —— 盘上的卷标几乎恒为 `"CD-ROM"`，卷唯一 ID 有 **31.3% 的撞号率**。

#### 3.5.1 `DiscLabel` —— Portfolio OS 原始源码

来源：[`trapexit/portfolio_os` — `src/filesystem/includes/discdata.h`](https://github.com/trapexit/portfolio_os/blob/master/src/filesystem/includes/discdata.h)
（文件头 RCS 标记 `$Id: discdata.h,v 1.16 1994/10/25 15:57:33 vertex Exp $` —— 这是 3DO 官方操作系统源码本身）

```c
#define VOLUME_SYNC_BYTE          0x5A
#define VOLUME_SYNC_BYTE_LEN      5
#define VOLUME_COM_LEN            32
#define VOLUME_ID_LEN             32

typedef struct DiscLabel {
  uchar    dl_RecordType;                   /* Should contain 1 */
  uint8    dl_VolumeSyncBytes[VOLUME_SYNC_BYTE_LEN]; /* Synchronization byte */
  uchar    dl_VolumeStructureVersion;       /* Should contain 1 */
  uchar    dl_VolumeFlags;                  /* Should contain 0 */
  uchar    dl_VolumeCommentary[VOLUME_COM_LEN];
                                            /* Random comments about volume */
  uchar    dl_VolumeIdentifier[VOLUME_ID_LEN]; /* Should contain disc name */
  uint32   dl_VolumeUniqueIdentifier;       /* Roll a billion-sided die */
  uint32   dl_VolumeBlockSize;              /* Usually contains 2048 */
  uint32   dl_VolumeBlockCount;             /* # of blocks on disc */
  uint32   dl_RootUniqueIdentifier;         /* Roll a billion-sided die */
  uint32   dl_RootDirectoryBlockCount;      /* # of blocks in root */
  uint32   dl_RootDirectoryBlockSize;       /* usually same as vol blk size */
  uint32   dl_RootDirectoryLastAvatarIndex; /* should contain 7 */
  uint32   dl_RootDirectoryAvatarList[ROOT_HIGHEST_AVATAR+1];
} DiscLabel;
```

**字节序**：全部多字节整数为**大端**（3DO CPU 为 ARM60 大端）。3dt 文档原文：*"All multi-byte integers in OperaFS structures are stored in **big-endian** format"* —— <https://github.com/trapexit/3dt/blob/master/docs/disc-format.md>

#### 3.5.2 四个独立实现的交叉验证

| 来源 | 类型 | 关键佐证 |
|---|---|---|
| **Portfolio OS `discdata.h`** | **3DO 官方 OS 源码** | 结构体定义本身 |
| [OperaFS-Format.md](https://github.com/barbeque/3dodump/blob/master/OperaFS-Format.md) | 格式文档（源自 stack.nl 的 Linux operafs 实现自带文档） | 字段表逐项对应 |
| [rcheevos `rc_hash_3do()`](https://github.com/RetroAchievements/rcheevos/blob/develop/src/rhash/hash_disc.c) | 生产级 C 实现 | 硬编码 `0x28` 标题 / `0x4C` 块大小 / `0x64` 根目录 / 总长 132 |
| [`opticaldiscs` v0.15.0 `src/browse/opera.rs`](https://github.com/danifunker/opticaldiscs-rs) | **Rust 实现** | 注释逐字：*"a 32-byte volume label at offset 40, the block size at 0x4C, and the root directory's block location at 0x64 (all big-endian)"* |

rcheevos 的注释与 magic：
```c
const uint8_t operafs_identifier[7] = { 0x01, 0x5A, 0x5A, 0x5A, 0x5A, 0x5A, 0x01 };
/* the Opera filesystem stores the volume information in the first 132 bytes of sector 0 */
if (rc_cd_read_sector(iterator, track_handle, 0, buffer, 132) >= 132 &&
    memcmp(buffer, operafs_identifier, sizeof(operafs_identifier)) == 0) {
  rc_hash_iterator_verbose_formatted(iterator, "Found 3DO CD, title=%.32s", &buffer[0x28]);
  md5_init(&md5);
  md5_append(&md5, buffer, 132);
  /* the block size is at offset 0x4C (assume 0x4C is always 0) */
  block_size = buffer[0x4D] * 65536 + buffer[0x4E] * 256 + buffer[0x4F];
  /* the root directory block location is at offset 0x64 ... */
  block_location = buffer[0x65] * 65536 + buffer[0x66] * 256 + buffer[0x67];
```
四个来源在 magic、`0x28`、`0x4C`、`0x64`、总长 132 五个锚点上**完全一致**。

#### 3.5.3 可识别字段表

| 字段名 | 偏移量 | 长度 | 含义 | 可靠性 |
|---|---|---|---|---|
| `dl_RecordType` | `0x00` | 1 | 恒 `0x01` | **极高（magic）/ 零（区分游戏）** |
| `dl_VolumeSyncBytes` | `0x01` | 5 | 恒 `5A 5A 5A 5A 5A` | **极高（magic）/ 零** |
| `dl_VolumeStructureVersion` | `0x06` | 1 | `1` = Opera 只读，`2` = LinkedMem | **极高（magic）/ 零** |
| `dl_VolumeFlags` | `0x07` | 1 | M1 零售盘几乎恒 0 | 低 |
| `dl_VolumeCommentary` | `0x08` | 32 | 卷注释 ASCII | **不可用** —— 3dodev 原文：*"Volume Commentary doesn't ever appear to be filled"* |
| `dl_VolumeIdentifier` | **`0x28`** | **32** | 卷名 ASCII | ⚠ **极低** —— 592 张盘只有 3 种取值：`CD-ROM` ×545、`cd-rom` ×38、`TECD` ×9 |
| **`dl_VolumeUniqueIdentifier`** | **`0x48`** | **4**（BE u32） | *"Roll a billion-sided die"* | ⭐ **中** —— 最强的单一字段，但**撞号率 31.3%**（见 3.5.4） |
| `dl_VolumeBlockSize` | `0x4C` | 4（BE） | 通常 2048 | 合理性检查 |
| **`dl_VolumeBlockCount`** | `0x50` | 4（BE） | 全盘块数 | **中**（很好的辅助判别键） |
| `dl_RootUniqueIdentifier` | `0x54` | 4（BE） | 根目录随机 ID | ⚠ **低** —— 与 VUID 完全共变，不提供额外区分度（实测） |
| `dl_RootDirectoryBlockCount` | `0x58` | 4（BE） | — | 低 |
| `dl_RootDirectoryBlockSize` | `0x5C` | 4（BE） | — | 低 |
| `dl_RootDirectoryLastAvatarIndex` | `0x60` | 4（BE） | 恒 7 | 校验用 |
| `dl_RootDirectoryAvatarList` | `0x64` | 4×8 = 32 | 根目录 8 份副本块号，`[0]` 为主副本 | 遍历 FS 必需 |
| **结构总长** | — | **132（`0x84`）** | `sizeof(DiscLabel)` | 与 rcheevos 的 "first 132 bytes" 一致 |

**7 字节 magic**：`01 5A 5A 5A 5A 5A 01`（偏移 `0x00`）—— 判定"这是一张 3DO 盘"的权威依据。

3DO 官方开发资料库对识别问题的自述（逐字）：
> "The 3DO disc format has a few fields used for identification. These were not used like other platform's identifiers to publicly identify them but could be used by emulators or other software which intend to uniquely identify software without file hashes or filename heuristics."
> —— <https://3dodev.com/documentation/games/opera/game_identification>

**扩展结构（M2 时代）**：3dt 定义了 `ExtDiscLabel`，在 132 字节后追加 `num_rom_tags`(`0x84`,4B)、`application_id`(`0x88`,4B)、`reserved[9]`(`0x8C`,36B)，总长 176 字节，仅当 `dl_VolumeFlags & VOLUME_FLAG_M2` 时存在。
> ⚠ `application_id` 看似是理想的序列号，但**【未找到权威来源】证明 Opera（M1）零售盘会填充它** —— 3dt 自己打印 `application_id` 的代码是注释掉的。**不要依赖它。**

#### 3.5.4 ⚠ 关键可靠性数据：卷标与 VUID 的实际区分度

对 [3dodev.com 官方对照表](https://3dodev.com/documentation/games/opera/game_identification)（**592 条实测记录**）统计：

| 指标 | 结果 |
|---|---|
| 光盘条目数 | 592 |
| `Volume ID` 不同取值 | **仅 3 种**（`CD-ROM` 545 / `cd-rom` 38 / `TECD` 9） |
| `Volume Unique ID` 不同取值 | 494 / 592 |
| **与他盘撞号的条目** | **185 条（31.3%）**，分布在 87 个撞号组 |
| `(VUID, RootUID)` 二元组不同取值 | **仍是 494** —— RootUID **零**额外贡献 |
| VUID 取值范围 | `0x00010AA8` – `0x3FF066B0`（全 < 2³⁰，符合 "billion-sided die"） |

**撞号不只是同名区域变体，不同游戏之间也会撞**：

| VUID | 撞号的盘 |
|---|---|
| `0x06563716` | `3DO Magazine 06 (UK)`、`OnSide Soccer (U)`、`Powerslide (Disc 1) (U)`、`Powerslide (Footie) (Disc 2) (U)` |
| `0x043DCD69` | `Decathlon (U)` 与 `Lost Eden (U)` |
| `0x3BF9DDB7` | `3DO Magazine 02`、`3DO Magazine 10`、`Super Street Fighter II X (Demo) (J)` |
| `0x2D731C98` | 4 个 `FIFA International Soccer` 变体 |

87 个撞号组中只有 38 组是"同名不同区"，**其余 49 组是完全不同的标题**。

对 3dt 内建识别库 `src/tdo_disc_ids_list.c`（**596 条**）的独立统计：

| 键 | 不同取值 / 596 | 仍撞号条目 |
|---|---|---|
| `volume_unique_id` 单键 | 537 | 101（17%） |
| `(volume_id, VUID, block_count, root_uid)` | 544 | 94（16%） |
| **完整 6 元组**（再加 `file_count`、`total_data_size`） | **557** | **77（13%）** |

**对照组**：Redump 3DO DAT 的 `.bin` SHA-1 **全部互异**（旧镜像 672/672；`redump.info` 现为 **669** 条，2026-08-27）。

> ⭐ **结论**：**卷标元组只能做"快速判是什么盘"的启发式（约 84–87% 唯一），要 100% 精确到 Redump 条目必须回退到全盘哈希。** 这与第一部分里 PS1 `SLUS_xxx.xx` 那种"序列号即主键"的情形**完全不同**，是本文最需要注意的一个反直觉结论。

#### 3.5.5 卷标位置：0 号扇区，还是 225 块？

3DO OS 的挂载代码 `MountFileSystem.c` 依次探测 **8+1 个 avatar**：先 `avatarBlock = 0`，再 `DISC_LABEL_OFFSET (=225)`，之后每次 `+= DISC_LABEL_AVATAR_DELTA (=32786)`：

```c
  gotLabel = 0;
  avatarBlock = 0;
  for (avatarIndex = -1;
       avatarIndex <= DISC_LABEL_HIGHEST_AVATAR && !gotLabel;
       avatarIndex++) {
    ...
    if (avatarIndex < 0) { avatarBlock = DISC_LABEL_OFFSET; }
    else                 { avatarBlock += DISC_LABEL_AVATAR_DELTA; }
  }
```
同文件还有：`if (blockOffset == 0 && devStatus.ds_FamilyCode == DS_DEVTYPE_CDROM) { blockOffset = 150; }` —— CD 设备加 150 扇区 pregap 偏移。
来源：<https://github.com/trapexit/portfolio_os/blob/master/src/filesystem/MountFileSystem.c>

> **【推断】** 在**镜像文件**里（镜像扇区 0 = 绝对 LBA 150 = 轨 1 的第 0 扇区），主卷标就落在**镜像扇区 0 的第 0 字节**。这解释了为何 rcheevos / opticaldiscs / Opera-libretro 全都直接读扇区 0。
> ⚠ 3dt 的 `docs/disc-format.md` 把 `DISC_LABEL_OFFSET` 写成 *"Byte offset to find the disc label marker"* —— **这是该文档的措辞错误**，OS 源码证明它是**块号**。以 OS 源码为准。
> 兜底方案：3dt 的 `find_label()` 会在**前 1 MiB**（`MAX_RECOGNITION_SCAN_BYTES = 1024*1024`）内暴力扫描 6 字节模式 `01 5A 5A 5A 5A 5A`。

#### 3.5.6 目录结构与 `LaunchMe`

**`DirectoryHeader`**（`sizeof` = 20 字节）：

| 偏移 | 长度 | 字段 | rcheevos 对应 |
|---|---|---|---|
| `0x00` | 4 | `dh_NextBlock`（int32，-1 = 末块） | `buffer[0x02..0x03]`，`== 0xFFFF` 判结束 ✅ |
| `0x04` | 4 | `dh_PrevBlock`（int32） | — |
| `0x08` | 4 | `dh_Flags` | — |
| `0x0C` | 4 | `dh_FirstFreeByte`（条目区结束偏移） | `buffer[0x0D..0x0F]` ✅ |
| `0x10` | 4 | `dh_FirstEntryOffset`（通常 20） | `buffer[0x12..0x13]` ✅ |

**`DirectoryRecord`**（固定 68 字节 + `(dir_LastAvatarIndex+1)×4`，最小 72 = `0x48`）：

| 偏移 | 长度 | 字段 | rcheevos 对应 |
|---|---|---|---|
| `0x00` | 4 | `dir_Flags`（低字节 `0x02`=文件 / `0x06`=特殊文件 / `0x07`=目录；`0x40000000`=块内末条目、`0x80000000`=目录末条目） | `buffer[off+0x03] == 0x02` ✅ |
| `0x04` | 4 | `dir_UniqueIdentifier` | — |
| `0x08` | 4 | `dir_Type`（FourCC：`*dir` / `*lbl` / `*zap`） | — |
| `0x0C` | 4 | `dir_BlockSize` | `buffer[off+0x0D..0x0F]` ✅ |
| `0x10` | 4 | `dir_ByteCount`（文件字节数） | `buffer[off+0x11..0x13]` ✅ |
| `0x14` | 4 | `dir_BlockCount` | — |
| `0x18` | 4 | `dir_Burst` | — |
| `0x1C` | 4 | `dir_Gap` | — |
| **`0x20`** | **32** | **`dir_FileName`**（NUL 结尾 ASCII） | `strcasecmp(&buffer[off+0x20], "LaunchMe")` ✅ |
| `0x40` | 4 | `dir_LastAvatarIndex` | `buffer[off+0x43]` ✅ |
| `0x44` | 4×N | `dir_AvatarList[]`，`[0]` 为数据起始块 | `buffer[off+0x45..0x47]` ✅ |

**条目步进** = `0x48 + dir_LastAvatarIndex × 4`。

**3DO 盘的判定完全靠"根目录下有没有 `LaunchMe`"**，Portfolio OS shell 源码 `src/app/shell/launchapp.c` 逐字：
```c
static bool Is3DOCD(Item cdromio, DeviceStatus *ds)
{
    Item file;
    if (ds->ds_DeviceBlockSize == 2048) {
        file = OpenDiskFile("$app/LaunchMe");
        if (file >= 0) { CloseDiskFile(file); return TRUE; }
    }
    return FALSE;
}

static const CDType cdTypes[] = {
    {IsNoCD,      "$RunNoCD"},
    {Is3DOCD,     "$app/LaunchMe"},
    {IsAudioCD,   "$RunAudioCD"},
    {IsPhotoCD,   "$RunPhotoCD"},
    {IsVideoCD,   "$RunVideoCD"},
    {IsNavikenCD, "$RunNavikenCD"},
};
```

- **`LaunchMe` 内部有无识别信息？** **【未找到权威来源】** —— 它是普通 ARM 可执行体，Portfolio OS 与 3dt 均未定义其"标题/序列号"字段。**唯一被公开利用的属性是它的字节内容本身**。
- **`AppStartup` / `takeme` 都不是零售盘标记**：`takeme` 只是 SDK 构建 CD 时的暂存目录名（`portfolio_os/GNUmakefile`）；`AppStartup` 是开发机启动脚本（`src/app/shell/shell.c`）。
- **ROM Tags**：位于卷标之后一块（`romtags_block = disc_label_block + 1`），24 字节/条，含 `sub_systype/type/version/revision/offset/size`。**这里没有游戏标题或序列号**，只有版本/修订号。（<https://github.com/trapexit/3dt/blob/master/docs/romtags.md>）

**RetroAchievements 的 3DO 哈希**：
```
MD5( DiscLabel 前 132 字节  ‖  根目录中 LaunchMe 文件的完整内容 )
```
优点：与容器格式、pregap、padding、subcode 全部无关，只依赖逻辑内容，比全盘哈希快数百倍。
> **【推断】** 与第一部分 C.3.4 里 PS1/PS2/PSP 只哈希启动 EXE 同理 —— **只替换数据文件而不动 `LaunchMe` 的汉化盘，其 RA 哈希与原版完全一致**；反过来说它也无法区分这类汉化版。

#### 3.5.7 磁盘上的"变体成型"形态

**多种封装格式并存**，各数据库/模拟器的选择各不相同：

| 形态 | 使用者 | 说明 |
|---|---|---|
| `.cue` + **单个** `.bin` | **Redump**（`redump.info` 现 **669** 条；下方统计基于旧镜像的 672 条快照） | 【实测】672 个 `.bin` 全部是 2352 的整数倍；**0 个 `(Track N).bin`** |
| `.cue` + `.iso` | **TOSEC**（15 个集合） | 与 Redump 的 `.bin` 不同 |
| 单个 `.iso` | No-Intro `Non-Redump`（部分） | 例：`Ballz - The Director's Cut … (Beta 1.0a).iso` 168 386 560 B |
| `.cue` + `.bin` | No-Intro `Non-Redump`（另一部分） | 12 条中 8 个 `.bin` + 9 个 `.cue` + 4 个 `.iso` |
| `.chd` | MAME `hash/3do.xml`、libretro Opera 核心 | 只有 SHA-1（CHD 的 combined raw+meta SHA-1，详见第一部分 B.2.4） |
| `.img` + `.ccd` / `.gdi` | ❌ | **Opera 核心不支持** |

**Opera-libretro 声明的扩展名**（`libretro.c` 逐字）：
```c
  info_->need_fullpath    = true;
  info_->valid_extensions = "iso|bin|chd|cue";
```
`opera_libretro.info` 中还有 `disk_control = "false"` —— **Opera 核心不支持 `.m3u` 多碟切换**，而 3DO 有大量多碟游戏（`D`、`Daedalus Encounter` 4 碟、`Brain Dead 13`、`Creature Shock` 等）。

**扇区形态嗅探**（`retro_cdimage.c`）：
- `size % 2048 == 0` → 2048 字节扇区，数据偏移 0
- `size % 2352 == 0` → 2352 字节扇区，抽查判定数据偏移 16（MODE1）或 0（音频）
- CUE 模式映射：`MODE1_2048`→(2048,0)、`MODE1_2352`→(2352,16)、`MODE2_2352`→(2352,24)、`AUDIO`→(2352,0)

⭐ **CHD 的 3DO 专用启发式**（Opera-libretro 逐字注释，非常实用）：
```c
  static const uint8_t pattern[8] =
    { 0x01, 0x5a, 0x5a, 0x5a, 0x5a, 0x5a, 0x01, 0x00 };
  /*
   * Heuristic: check the first sector header for a 3DO MODE1 disc.
   * 3DO MODE1 discs use 2448-byte sectors (2352 + 96 subcode).
   * Discs whose first sector does not start with this pattern
   * are treated as MODE1_RAW (2352-byte sectors + 16-byte data offset).
   */
  if(!memcmp(buf,pattern,sizeof(pattern)))
    { cdimage_->sector_size = 2448; cdimage_->sector_offset = 0; }
  else
    { cdimage_->sector_size = 2352; cdimage_->sector_offset = 16; }
```
> **3DO 的整盘 CHD 常见为 2448 字节/扇区（2352 + 96 字节 subcode）**，此时卷标直接落在扇区起始处（offset 0）。这是很容易踩的坑。

**3DO 盘有没有 CD 音轨？—— 实测：几乎没有**

| 检查项 | Redump 3DO | 对照组 |
|---|---|---|
| 条目数 | 672（旧镜像快照；现站 669） | PC Engine CD 551 / PSX 10914 |
| 拥有 >1 个 `.bin`（多轨）的条目 | **0** | PCECD 15751 个 `(Track N).bin`；PSX **4605 条**多轨 |
| `.bin` 大小全部 % 2352 == 0 | 是 | — |
| `.cue` 大小范围 | 78 – 194 字节 | — |

> **【推断，证据强】** Redump 收录的 3DO 光盘 100% 是单条 MODE1/2352 数据轨，**没有 Red Book 音轨** —— 3DO 的音频走文件系统内的 AIFF/SDX2，不用 CD-DA。
> 补充事实（MAME `hash/3do.xml` 逐字注释）：*"several games have multiple indices over a given track. These are intentionally omitted until we have a solution."* —— 是**同一条轨内的多个 INDEX**，不是多条轨。

#### 3.5.8 DAT / 数据库覆盖

| 库 | 集合 | 条目 | 哈希粒度 |
|---|---|---|---|
| **Redump** ⭐ | `3DO Interactive Multiplayer`（`redump.info`，2026-08-27） | **669** | 每条 `.cue` + `.bin` 各一，crc+md5+sha1（Logiqx DTD，**无 sha256**）。`.bin` SHA-1 全互异。⚠ 旧镜像 `redump.org` 上叫 `Panasonic - 3DO Interactive Multiplayer`、672 条，**已冻结，见 [1.2](#12-redump必须走-redumpinfo旧镜像少-46-个系统实测)** |
| **TOSEC** | 15 个 TOSEC-ISO 集合 | ~660 | Games 459、Multimedia 48、Educational 40、Samplers 39、Coverdiscs 34、Applications 13、Homebrew 16、Bonus 4、Firmware 9。`.cue` + `.iso` |
| **No-Intro** | `Non-Redump - Panasonic - 3DO …` | 12 | Beta/Proto，8×`.bin` + 9×`.cue` + 4×`.iso` |
| **No-Intro** | `Source Code - Panasonic - 3DO …` | 7 | 源码泄露：Portfolio OS、Doom、Escape from Monster Manor、Icebreaker、Icebreaker 2、Star Control II、Star Fighter |
| **libretro-database** | `metadat/redump/The 3DO Company - 3DO.dat` | **706** | 扁平化：每条只有一个 `.bin`（整盘），**不含 `.cue`**；**467 条带 `serial`**（432 个不同值，如 `FZ-SJ4452`、`IMP-SD0101`、`GDT-GA401`） |
| **MAME** | `hash/3do.xml` | **20** | `<disk sha1=…>`（CHD）。`barcode` ×19、`serial` ×14、`alt_title` ×8。极不完整 |
| **MAME** | `hash/3do_m2.xml` | 3 | 头部注释：*"This list is here only to document available dumps and it's not used (yet) by MAME!"* |

Redump 的 3DO 转储指南**全文只有两行**：
> ==General Notes==
> * These discs may be treated like normal CD-ROMs for the purpose of dumping.
> * 3DO based Arcade systems also go under this dat.

—— <https://wiki.redump.info/index.php?title=3DO_Interactive_Multiplayer_Guide>

> ⭐ **"3DO 光盘有特殊 ECC" 的说法：【未找到权威来源】。** Redump 官方明确说"当作普通 CD-ROM 处理"。唯一的特殊性是 3DO 的 **CD-DIPIR 拷贝保护 / RSA 签名**（`portfolio_os/src/dipir/cdromdipir.c`），但那是盘内数据层的签名，不影响转储。
> ⚠ **libretro / MAME 的 `serial` 字段（`FZ-SJ4452`、`EA 9107`）是印在盘上的厂商目录号，不是从盘内数据读出的** —— `DiscLabel` 里根本没有这样的字段。No-Intro / Redump 的 3DO DAT 均**不带 `serial` 属性**。

⭐ **另有一个非 DAT 的宝贵资源**：[3dt 的 `src/tdo_disc_ids_list.c`](https://github.com/trapexit/3dt/blob/master/src/tdo_disc_ids_list.c)，**596 条**内建卷标 ID 库（ISC 许可），结构为：
```c
struct tdoid_t {
  const char *name;
  const char *volume_id;
  uint32_t    volume_unique_id;
  uint32_t    volume_block_count;
  uint32_t    root_unique_id;
  uint32_t    file_count;
  uint32_t    total_data_size;
};
/* 样例 */
{"IMSA World Championship Racing","CD-ROM",0x21F3B3FC,327680,0x38F6F285,650,606482695},
```
**这是唯一一个"卷标元组 → 游戏名"的现成映射表**，可直接内嵌进识别器。

#### 3.5.9 中文汉化覆盖

**零。** 逐库核实：

| 数据源 | `[tr zh]` | 其他中文相关 |
|---|---|---|
| TOSEC `3DO … - Games`（459 条） | **0** | 27 条翻译版**全部是 `[tr ru]`**（俄语），如 `Alone in the Dark (1994)(Pony Canyon)(JP)[tr ru aliast - aspyd - FantasyNik][v20170807]` |
| TOSEC `3DO … - Homebrew - Compilations`（7 条） | **0** | 5 条翻译版，无中文 |
| Redump 3DO DAT（669 条） | **0** | 仅 2 条**官方**多语言发行：`Supreme Warrior (Japan) (Ja,Zh) (Disc 1) (Fire & Earth)` / `(Disc 2) (Wind & Fang Tu)` |
| libretro `metadat/redump/The 3DO Company - 3DO.dat`（706 条） | **0** | 该 DAT 命名方案不带语言标签 |
| No-Intro `Non-Redump` / `Source Code`（12 + 7 条） | **0** | 0 |
| libretro `metadat/hacks/` 与 `metadat/homebrew/` | 无 3DO 文件 | — |
| MAME `hash/3do.xml`（20 条） | **0** | `language` 字段仅 5 条，无中文 |

**明确声明：在所有可用的一手 DAT 源中，3DO 平台的中文汉化条目数为 0。**
（排除干扰项：DAT 中的 `Zhadnost - The People's Party` 只是标题拼写巧合。）

#### 3.5.10 实现成本评估

**分层策略建议**：

| 层 | 键 | 命中/精度 | 成本 |
|---|---|---|---|
| **L0 探测** | 扇区 0 前 7 字节 `01 5A×5 01` | 100% 判"是不是 3DO 盘" | 极低 |
| **L1 快速识别** | `(volume_id, VUID, block_count, root_uid)` 查 3dt 596 条库 | ~91%（544/596 唯一） | 低，只读 1 扇区 |
| **L2 强化识别** | 再加 `file_count` + `total_data_size` | ~93%（557/596 唯一） | 中，需遍历全 FS |
| **L3 RA 兼容** | `MD5(132B ‖ LaunchMe)` | 与 RetroAchievements 数据库对齐 | 中，读 LaunchMe（几十 KB–几 MB） |
| **L4 精确定位** | 全盘 `.bin` SHA-1 → Redump 669 条 | **100%** | 高，需读 ~700 MB |

3dt 的两级匹配语义值得直接照抄（`src/tdo_disc_ids.c`）：
```c
int tdo_disc_ids_equal(const tdoid_t *a_, const tdoid_t *b_)      /* 完全匹配 */
{
  if(a_->volume_unique_id   != b_->volume_unique_id)   return 0;
  if(a_->volume_block_count != b_->volume_block_count) return 0;
  if(a_->root_unique_id     != b_->root_unique_id)     return 0;
  if(strcmp(a_->volume_id,b_->volume_id))              return 0;
  if((a_->file_count && b_->file_count) && (a_->file_count != b_->file_count)) return 0;
  if((a_->total_data_size && b_->total_data_size) && (a_->total_data_size != b_->total_data_size)) return 0;
  return 1;
}
int tdo_disc_ids_equalish(const tdoid_t *a_, const tdoid_t *b_)   /* 模糊匹配 */
{
  if(a_->volume_unique_id == b_->volume_unique_id) return 1;
  if(a_->root_unique_id   == b_->root_unique_id)   return 1;
  if((a_->file_count && b_->file_count) && (a_->file_count == b_->file_count) &&
     (a_->total_data_size && b_->total_data_size) && (a_->total_data_size == b_->total_data_size)) return 1;
  return 0;
}
```

**Rust 生态**：

| crate | 版本 | 价值 |
|---|---|---|
| ⭐ **`opticaldiscs`** | 0.15.0（2026-08-10，1819 下载） | **已实现 OperaFS**：`src/browse/opera.rs`（266 行，`detect_opera()` / `read_label()` / `OperaFilesystem`）、`src/detect.rs`（`FilesystemType::Opera`）、`src/gameid.rs::probe_3do()`。同时提供 ISO / BIN-CUE / CHD 容器层。<br>⚠ 但 `probe_3do()` **只取 `0x28` 卷名做 title**，按 3.5.4 的实测这等于恒返回 `"CD-ROM"`。**复用时必须自行补上 VUID / block_count / root_uid 的提取。** |
| `chd` | 0.3.4 | 纯 Rust CHD 读取 |
| `cue_sheet` / `rcue` | 0.3.0 / 0.1.3 | CUE 解析（`cue` crate 是 libcue 绑定，需 C 依赖） |

**C/C++ 参考实现**：
- ⭐ [**3dt**](https://github.com/trapexit/3dt)（ISC 许可，`3dt identify` 子命令）—— 最完整，含 596 条 ID 库、full/partial 两级匹配、CSV 输出、MODE1 同步头嗅探、1 MiB 扫描兜底
- [rcheevos `rc_hash_3do()`](https://github.com/RetroAchievements/rcheevos/blob/develop/src/rhash/hash_disc.c) —— **约 110 行，可直接逐行移植**
- [Opera-libretro](https://github.com/libretro/opera-libretro) —— 容器层与 CHD 启发式

**工作量**：
- 纯 OperaFS 解析 + 识别：**约 400–600 行 Rust**
- 若自行实现 CUE/CHD 层再加 500–800 行（**建议直接用 crate**）
- **这是所有 CD 主机里最简单的一档** —— 结构体固定 132 字节、无 ISO9660、无 PVD、无路径表、无 UDF

| 情形 | 人日估算 |
|---|---|
| 容器层已有（PS1/Saturn/DC 已做过） | **0.5–1 人日** |
| 从零写容器层 | **2–3 人日** |

**3DO 特有的坑**：
1. **多碟游戏**（`D`、`Daedalus Encounter`、`Brain Dead 13`、`Creature Shock`…）每碟 VUID 不同，需在上层做 disc-set 归并；Opera 核心 `disk_control = "false"`，不能靠 `.m3u`
2. **同母盘的美/欧/韩版 VUID 完全相同**（BattleSport、Blade Force、Captain Quazar、Immercenary…），**区域信息盘内不可得**，只能靠 Redump DAT 的文件名
3. **CHD 的 2448 字节扇区**（2352 + 96 subcode）—— 不处理会读不到卷标
4. 3DO 无区域锁（MAME 注释："3DO notoriously has no explicit region lock"），但部分 PAL 游戏在 NTSC 机上拒绝运行，日版可能需要 kanji ROM

---

## 第 4 章 · 汇总与优先级建议

### 4.1 ⭐ 八平台总表

| 平台 | 识别难度 | 成型规则复杂度 | DAT 覆盖 | 中文汉化覆盖 | 实现成本（人日） | 建议优先级 |
|---|---|---|---|---|---|---|
| **Neo Geo Pocket / Color** | **极低**<br>开头 28 字节固定 magic + 64 字节定长头，无校验和 | **极低**<br>单文件、无外挂头、无并存容器 | **好**<br>No-Intro 13+128（118 条带 serial）· TOSEC 402 · MAME 129 | ⭐ **最好的一个**<br>TOSEC 3 条 / 2 游戏 | **0.2–0.3** | **P0** |
| **WonderSwan / WSC** | **低**<br>头在**文件末尾** 16 字节，需 3 张查表，有全 ROM 校验和 | **极低**<br>单文件、无外挂头（`.wsr` 例外） | **好**<br>No-Intro 257+242 · TOSEC 419 · MAME 234（**芯片级**） | 差<br>TOSEC 1 条（WSC Gundam Seed） | **0.3–0.5** | **P0** |
| **Atari Lynx** | **低**<br>64 字节 LNX 头（6 源一致）/ 10 字节 BS93 头 | **中**<br>⚠ `.lyx`(无头) / `.lnx`(含头) / `.bll` **三套并存**，必须双哈希 | **好**<br>No-Intro 三 DAT 176 · TOSEC 677 · MAME 119 | **零** | **0.4–0.6** | **P0** |
| **3DO** | **低–中**<br>132 字节定长卷头（4 源交叉验证），但**需容器层**（cue/bin/iso/chd） | **中**<br>`.cue+.bin` / `.cue+.iso` / `.iso` / `.chd`（**CHD 常为 2448 字节扇区**） | **好**<br>Redump 669 · TOSEC 679 · No-Intro 19 · MAME 20 | **零**（27 条翻译版全是俄语） | **0.5–1**（容器层已有）<br>**2–3**（从零） | **P0–P1** |
| **Xbox 360** | **中**<br>⚠ **XEX 头不加密，TitleID 免密钥可读**；但 GDF 小端 / XEX·STFS 大端混用 | **中–高**<br>ISO(XGD2/3，6 个候选偏移) / 裸 XEX / STFS 单文件 / **GOD 分片目录** | **很好**<br>Redump 3707（**唯一稳定整盘哈希**）· No-Intro Digital 17438 | **零**（官中 Redump ~305 条） | **3**（P0）<br>**4**（+XeMID+SS 世代） | **P1** |
| **Wii U** | **中**<br>⭐ **WUD 前 22 字节明文、TMD/TIK 完全明文 → 识别全程零密钥**；但 XML 根元素与 title id 权威性有坑 | **高**<br>7 种形态：HOST_FS / WUD / **分卷 WUD** / NUS(两套命名) / `.wua`(**魔数在文件尾**) / `.wuhb` / WUMAD | **好**<br>⭐ Redump **541**（尺寸全部 `25025314816`）· No-Intro CDN 3682 · libretro/GameTDB 2871 · **TOSEC 0** | **零**<br>⚠ **连官中都没有**（Wii U 未在中国大陆发售；region 位掩码只有 JPN/USA/EUR） | **3**（最小可用）<br>**10**（工程化，1800 行）<br>**16.5**（含解密，不建议） | **P1** |
| **PS Vita** | **中**<br>SFO 极简单，但**形态多**；⭐ `work.bin` 512 B 可直接命中 DAT | **最高**<br>**8 种并存形态**（5 目录 + 3 单文件），卡带镜像还需 exFAT | **中**<br>No-Intro 8 个 DAT（PSN 17505 / VPK 240 / NoNpDrm 452）· **Redump 无** · TOSEC 仅 41 条 homebrew | **民间汉化零**<br>⭐ **但官中 290 条，No-Intro 用 `(Zh/Zh-Hant/Zh-Hans)` 标记，覆盖良好** | **8.5**（不含 exFAT）<br>**12–14**（含） | **P2** |
| **N-Gage** | **高**<br>SIS/SISX 递归压缩字段树；MMC 镜像需 FAT；**可执行体被混淆且校验 MMC-ID** | 高<br>MMC 镜像 / `System/Apps/` 目录 / `.sis`/`.sisx` / `.n-gage` | ⚠ **近乎为零**<br>No-Intro **1** 条（停更 2022-02）· TOSEC **5** 条（全是同一款原型）· libretro **0** | **零** | **1**（L0+L1）<br>**2.5**（+FAT） | **P4 / 跳过** |

**合计**：
- **P0 四个平台**（NGPC + WS + Lynx + 3DO）：约 **1.4–2.4 人日**
- **+ P1 两个平台**最小可用版（X360 3 + Wii U 3）：约 **8–9 人日**
- **+ P1 工程化版**（X360 4 + Wii U 10）：约 **16–17 人日**
- **全做（含 PSV 8.5，不含 Vita 卡带 exFAT 与 Wii U 解密层）**：约 **25 人日**

### 4.2 一句话行动建议

1. **先做 P0 四件套（NGPC → WS → Lynx → 3DO）**，约 2 人日，直接复用第一部分的 L0→L1→L2 流水线，DAT 覆盖完整、收益立刻可见。
2. **Lynx 必须实现 64 字节 LNX 头剥离并同时算含头/去头两套哈希**，否则会漏掉一半（No-Intro 主集是 `.lyx` 无头，TOSEC 主集是 `.lnx` 含头）。
3. **P1 的 Xbox 360 与 Wii U 只做"形态判定 + TitleID 读取 + 整盘尺寸预筛"**，**不要碰解密** —— 两者的识别所需字段**全部是明文**（X360 的 XEX 头、Wii U 的 WUD 前 22 字节与 TMD），且都有 Redump 的稳定整盘哈希兜底。Wii U 的解密层是唯一会引入 GPL 传染与密钥合规问题的部分。
4. **PS Vita 排到最后，且第一版只做 `work.bin`（512 字节）哈希 + `param.sfo` 读取**，放弃卡带镜像的 exFAT。
5. **N-Gage 直接跳过。**

### 4.3 ⚠ 本次调研中被推翻的三个判断（务必记住）

| # | 一度以为 | 实际 | 根因 |
|---|---|---|---|
| 1 | 「Redump 没有 Wii U」 | **Redump 有 541 条 Wii U 光盘 DAT** | **`redump.org` 自 2026-06-20 起是冻结镜像**（60 系统），Redump 已迁至 **`redump.info`**（106 系统）。`auto-datfile-generator` 的 `redump.py` 硬编码旧域名，因此**一切基于它的镜像都是旧数据** |
| 2 | 「TOSEC 没有 NGP/NGPC/N-Gage/PS Vita 集」 | **全都有**（NGPC 7 个集 371 条，含 **3 条 `[tr zh]`**） | GitHub 的 **`/contents` API 在 1000 条处硬截断且不报错**。必须改用 `git/trees?recursive=1`（该 TOSEC 镜像实际有 **4502** 个 DAT，不是 2300） |
| 3 | 「WonderSwan 头只有 10 字节，没有版本号」 | **WSdev 定义的是 16 字节，`$9` 是 Game version / Safe mode** | `wstech24.txt`（2003 年）只文档化了最后 10 字节并把 `$9` 标为 `??`；MAME/Mednafen 也不读它。**单一来源不足以下"没有"的结论** |

> **方法论教训**：**"某库没有某平台"这类否定性结论，必须用两个独立途径验证**，且要警惕 ①分页/条数截断 ②域名迁移后的僵尸镜像 ③单一文档的覆盖范围小于格式本身。

### 4.4 与第一部分的衔接

| 第一部分的结论 | 在本文 8 个平台上的适用性 |
|---|---|
| **同时算含头/去头两套哈希**（D.1） | ✅ **Lynx 必须做**（64 字节 LNX 头 / 10 字节 BS93 头）。WS / NGPC / 3DO / 现代三平台**无外挂头，不需要** |
| **优先用内部序列号作主键**（D.3） | ✅ 全部适用。NGPC 的 `catalog` 可直接算出 No-Intro `serial`；PSV/Wii U/X360 的 TitleID 被 No-Intro 自己当作 `<game_id>` 存储 |
| **复用 rc_hash 作结构不变量**（C.3.4） | ⚠ **只有 Lynx 与 3DO 可用**。Wii U / Xbox / N-Gage 的 console ID 已定义但 `hash.c` 零实现；**PS Vita 连 enum 都没有** |
| **TOSEC 的 `[tr zh]` 是汉化主力数据源**（C.4.1） | ⚠ **在本文 8 个平台上几乎失效** —— 全部 53 个 DAT、2240 个 game 里只有 **4 条 `[tr zh]`**（NGPC 3 + WSC 1） |
| **Hasheous API 兜底**（D.2.2） | 本文未测试其对这 8 个平台的覆盖，**【未找到权威来源】** |
| **`.rxdelta` 反向补丁 + xattr 持久化**（C.5） | ✅ 适用于 WS/NGPC/Lynx 这类单文件平台；**对 PSV/Wii U/X360 的目录树形态需改造为"目录内容清单哈希"** |

---

## 附录 · 一手来源清单

### 平台硬件文档 / 开发者 Wiki

**PS Vita**
- psdevwiki — [PARAM.SFO](https://www.psdevwiki.com/vita/PARAM.SFO) / [PKG_files](https://www.psdevwiki.com/ps3/PKG_files) / [Productcode](https://www.psdevwiki.com/ps3/Productcode) / [Content ID](https://www.psdevwiki.com/ps3/Content_ID) / [Files on the PS Vita](https://www.psdevwiki.com/vita/Files_on_the_PS_Vita) / [Talk:Title_ID](https://www.psdevwiki.com/vita/Talk:Title_ID)（⚠ 直连常被 Cloudflare 403，可走 Wayback）
- henkaku wiki — [Applications](https://wiki.henkaku.xyz/vita/Applications) / [Packages](https://wiki.henkaku.xyz/vita/Packages) / [PSVIMG](https://wiki.henkaku.xyz/vita/PSVIMG) / [Game Card](https://wiki.henkaku.xyz/vita/Game_Card) / [SELF](https://wiki.henkaku.xyz/vita/SELF)
- [wiki.no-intro.org — Sony - Playstation Vita undumped](https://wiki.no-intro.org/index.php?title=Sony_-_Playstation_Vita_undumped)（⚠ 与被禁的 `datomatic.no-intro.org` 是**不同主机**）

**Wii U**
- wiiubrew（⚠ **对默认 UA 返回 403，需带浏览器 UA**）— [Title metadata](https://wiiubrew.org/wiki/Title_metadata) / [Title](https://wiiubrew.org/wiki/Title)（**title id 权威性排序**）/ [Title_database](https://wiiubrew.org/wiki/Title_database) / [Meta.xml](https://wiiubrew.org/wiki/Meta.xml) / [/vol/code](https://wiiubrew.org/wiki//vol/code)（app.xml / cos.xml 真实样例）/ [FST](https://wiiubrew.org/wiki/FST) / [Ticket](https://wiiubrew.org/wiki/Ticket) / [RPL](https://wiiubrew.org/wiki/RPL) / [Encryption keys](https://wiiubrew.org/wiki/Encryption_keys)
  > ⚠ **wiiubrew 上不存在 WUD / WUX / Loadiine 的格式页**（全站 340 页穷举确认）—— 这三者只能靠实现源码
- [wiki.redump.info — Nintendo Wii U Dumping Guide](https://wiki.redump.info/Nintendo_Wii_U_Dumping_Guide)（密钥政策，修订 66296）
- [rom-properties `wiiu_structs.h`](https://github.com/GerbilSoft/rom-properties/blob/master/src/libromdata/Console/wiiu_structs.h)（**WUD 22 字节明文头的唯一来源**）
- [WudCompress `main.cpp`](https://github.com/cemu-project/WudCompress/blob/master/WudCompress/main.cpp)（WUX 格式规范原文；⚠ 仓库**无 LICENSE 文件**）
- [bodgit/wud](https://pkg.go.dev/github.com/bodgit/wud)（**唯一文档化 WUX 对齐填充**）
- [Exzap/ZArchive](https://github.com/Exzap/ZArchive)（`.wua` 的底层格式，`zarchivecommon.h` 的 Footer）
- [ique.com](https://www.ique.com/)（神游产品线，Wii U 未在中国大陆发售的直接证据）

**Xbox 360**
- Free60 — [XEX](https://free60.org/System-Software/Formats/XEX/) / [STFS](https://free60.org/System-Software/Formats/STFS/) / [GDFX](https://free60.org/Systems/GDFX/)（源仓库 <https://github.com/Free60Project/wiki>；⚠ 旧路径 `free60project.github.io/wiki/*.html` 已全部 404）
- xboxdevwiki — [XDVDFS](https://xboxdevwiki.net/index.php?title=XDVDFS&action=raw) / [Xbox_Game_Disc](https://xboxdevwiki.net/index.php?title=Xbox_Game_Disc&action=raw)
- [wiki.redump.info — Microsoft Xbox 360 Guide](https://wiki.redump.info/Microsoft_Xbox_360_Guide) / [Stock Xbox360 drives](https://wiki.redump.info/Stock_Xbox360_drives)（PFI↔wave 表、XeMID 解析）/ [Disc Dumping Guide (MPF)](https://wiki.redump.info/Disc_Dumping_Guide_(MPF))

**WonderSwan**
- [WSdev Wiki — ROM header](https://ws.nesdev.org/wiki/ROM_header)（CC0，**唯一完整的 16 字节布局**）/ [Cartridge](https://ws.nesdev.org/wiki/Cartridge) / [Memory map](https://ws.nesdev.org/wiki/Memory_map) / [Bandai 2003](https://ws.nesdev.org/wiki/Bandai_2003)
- `wstech24.txt`（WStech doc v2.4, Judge & Dox, 2003-12-26）— <https://github.com/libretro/beetle-wswan-libretro/blob/master/mednafen/wswan/wstech24.txt>

**Neo Geo Pocket**
- SNK 官方开发规格书 `ngpcspec.txt` — <http://devrs.com/ngp/files/DoNotLink/ngpcspec.txt>
- [hiddenpalaceorg/rom-info issue #24](https://github.com/hiddenpalaceorg/rom-info/issues/24)（偏移整理）

**Atari Lynx**
- K. Wilkins `make_lnx.c` V5 — <https://codeberg.org/42Bastian/new_bll>（GitHub 镜像 <https://github.com/42Bastian/new_bll>）
- cc65 `libsrc/lynx/exehdr.s` — <https://github.com/cc65/cc65/blob/master/libsrc/lynx/exehdr.s>

**N-Gage / Symbian**
- Alexander Thoukydides《SIS File Format》v1.19 — <https://thoukydides.github.io/riscos-psifs/sis.html>
- Symbian 官方开源码 `secureswitools/swisistools/source/sisxlibrary/` — <https://github.com/SymbianSource/oss.FCL.sf.mw.appinstall>（EPL-1.0）
- [EKA2L1 N-Gage 快速上手](https://eka2l1.github.io/quickstart/ngage/)（MMC / FAT32 / DRM / MMC-ID）
- [dumping.guide — Nokia N-Gage](https://dumping.guide/todo/carts/nokia/n-gage)
- [Symbian Developer Library — Application UIDs](https://docs.huihoo.com/symbian/nokia-symbian3-developers-library-v0.8/GUID-EA05F9B6-52C7-4BD9-8B9A-4BA3456E70B5.html)

**3DO**
- ⭐ **Portfolio OS 原始源码** — [`src/filesystem/includes/discdata.h`](https://github.com/trapexit/portfolio_os/blob/master/src/filesystem/includes/discdata.h) / [`src/filesystem/MountFileSystem.c`](https://github.com/trapexit/portfolio_os/blob/master/src/filesystem/MountFileSystem.c) / [`src/app/shell/launchapp.c`](https://github.com/trapexit/portfolio_os/blob/master/src/app/shell/launchapp.c) / `src/dipir/`
- [OperaFS-Format.md](https://github.com/barbeque/3dodump/blob/master/OperaFS-Format.md)（内容源自 stack.nl 的 Linux operafs 实现自带文档）
- [3dodev.com — Game identification](https://3dodev.com/documentation/games/opera/game_identification)（**592 条 Volume ID / VUID / RootUID 实测对照表**）
- **3dt** — <https://github.com/trapexit/3dt>（`docs/disc-format.md`、`docs/filesystem.md`、`docs/romtags.md`、`src/tdo_disc_ids_list.c` 596 条 ID 库）
- [wiki.redump.info — 3DO Interactive Multiplayer Guide](https://wiki.redump.info/index.php?title=3DO_Interactive_Multiplayer_Guide)

**通用**
- freedesktop.org shared-mime-info — <https://gitlab.freedesktop.org/xdg/shared-mime-info>（NGP/NGPC/Lynx/SISX 的 magic 定义）

### 模拟器 / 工具源码

| 平台 | 项目 | 关键文件 |
|---|---|---|
| PSV | [Vita3K](https://github.com/Vita3K/Vita3K) | `vita3k/packages/{sfo,pkg,license,vci}.{h,cpp}`、`interface.cpp`、`app/src/apps_list.cpp` |
| PSV | [vitasdk/vita-toolchain](https://github.com/vitasdk/vita-toolchain) | `src/vita-mksfoex/vita-mksfoex.c`、`src/vita-pack-vpk/vita-pack-vpk.c` |
| PSV | [mmozeiko/pkg2zip](https://github.com/mmozeiko/pkg2zip) | `pkg2zip.c`、`zrif2rif.py`（已 archived，Unlicense） |
| PSV | [TheOfficialFloW/NoNpDrm](https://github.com/TheOfficialFloW/NoNpDrm) | `main.c` + `readme.md` |
| PSV | [motoharu-gosuto/psvgamesd](https://github.com/motoharu-gosuto/psvgamesd) | `driver/psv_types.h`（`.psv` 归档规范） |
| PSV | [motoharu-gosuto/psvpfstools](https://github.com/motoharu-gosuto/psvpfstools) | PFS 魔数（⚠ `psvpfsparser` 在这里，**不在** vita-toolchain） |
| PSV | [yifanlu/psvimgtools](https://github.com/yifanlu/psvimgtools) | `psvimg.h`、`crypto.h` |
| PSV | [oestriot/VCI-TOOLS](https://github.com/oestriot/VCI-TOOLS) · [codestation/qcma](https://github.com/codestation/qcma) · [TheOfficialFloW/VitaShell](https://github.com/TheOfficialFloW/VitaShell) | — |
| Wii U | ⭐ [Cemu](https://github.com/cemu-project/Cemu)（MPL-2.0） | `src/Cafe/TitleList/{TitleInfo,ParsedMetaXml,TitleId,AppType}.{h,cpp}`、`src/Cafe/Filesystem/{WUD/wud.h,FST/FST.cpp,WUHB/WUHBReader.h}`、`src/Cafe/OS/RPL/{rpl_structs.h,rpl.cpp}`、`src/Cafe/CafeSystem.cpp` |
| Wii U | ⭐ [rom-properties](https://github.com/GerbilSoft/rom-properties)（GPL-2.0+） | `src/libromdata/Console/{wiiu_structs.h,WiiU.cpp,WiiUPackage.cpp,WiiUPackage_xml.cpp,WiiUFst.cpp,WiiUH3Reader.cpp}` —— **本身就是 ROM 识别器，最直接对标** |
| Wii U | ⭐ [JNUSLib](https://github.com/Maschell/JNUSLib)（GPL-3.0） | `implementations/wud/{WUDImage,WUDImageCompressedInfo,GamePartitionHeader}.java`、`parser/WUDInfoParser.java`、`reader/WUDDiscReaderSplitted.java`、`Settings.java` —— **唯一统一建模六种形态** |
| Wii U | [FIX94/wudump](https://github.com/FIX94/wudump)（**MIT**）· [FIX94/wud2app](https://github.com/FIX94/wud2app)（**MIT**） | 许可最安全的参考 |
| Wii U | [VitaSmith/cdecrypt](https://github.com/VitaSmith/cdecrypt) · [decaf-emu](https://github.com/decaf-emu/decaf-emu) · [dimok789/loadiine_gx2](https://github.com/dimok789/loadiine_gx2) · [CarlKenner/dolphin (WiiU 分支)](https://github.com/CarlKenner/dolphin/tree/WiiU) | TMD 判别 / RPL 结构 / ID6 规则 / 明文头读取 |
| X360 | ⭐ [Xenia](https://github.com/xenia-project/xenia) | `kernel/util/xex2_info.h`、`cpu/xex_module.cc`、`kernel/user_module.cc`、`vfs/devices/{stfs_xbox.h,stfs_container_device.cc,disc_image_device.cc}`、`xbox.h` |
| X360 | [emoose/idaxex](https://github.com/emoose/idaxex) | `formats/{xex.hpp,xex_headerids.hpp,xex_structs.hpp,xex_optheaders.hpp}`（**唯一支持全 6 种 XEX magic**） |
| X360 | [XboxDev/extract-xiso](https://github.com/XboxDev/extract-xiso) | `extract-xiso.c`（XGD 偏移常量权威） |
| X360 | [iliazeus/iso2god-rs](https://github.com/iliazeus/iso2god-rs) | `src/god/{file_layout,con_header,mod}.rs`（**Rust**，ISO→GOD 端到端） |
| X360 | [vin047/abgx360](https://github.com/vin047/abgx360) | `src/abgx360.c`（**唯一**处理 SS/DMI/PFI/stealth） |
| X360 | [saramibreak/DiscImageCreator](https://github.com/saramibreak/DiscImageCreator) · [superg/redumper](https://github.com/superg/redumper) · [SabreTools/MPF](https://github.com/SabreTools/MPF) · [hetelek/Velocity](https://github.com/hetelek/Velocity) | — |
| WS | MAME | [`src/devices/bus/wswan/slot.cpp`](https://github.com/mamedev/mame/blob/master/src/devices/bus/wswan/slot.cpp)、`rom.cpp`、[`src/mame/bandai/wswan.cpp`](https://github.com/mamedev/mame/blob/master/src/mame/bandai/wswan.cpp)（⚠ **不在** `src/mame/handheld/`） |
| WS | [beetle-wswan](https://github.com/libretro/beetle-wswan-libretro) | `mednafen/wswan/main.cpp`、`v30mz.cpp`、`wstech24.txt` |
| NGPC | [beetle-ngp](https://github.com/libretro/beetle-ngp-libretro) | `mednafen/ngp/{rom.h,rom.c,mem.c,neopop.h,neopop.cpp}` |
| NGPC | [libretro/RACE](https://github.com/libretro/RACE) | `main.c::initRom()` |
| NGPC | MAME | [`src/mame/snk/ngp.cpp`](https://github.com/mamedev/mame/blob/master/src/mame/snk/ngp.cpp) |
| Lynx | MAME | [`src/mame/atari/lynx.cpp`](https://github.com/mamedev/mame/blob/master/src/mame/atari/lynx.cpp)、[`lynx_m.cpp`](https://github.com/mamedev/mame/blob/master/src/mame/atari/lynx_m.cpp)（`verify_cart`） |
| Lynx | [beetle-lynx](https://github.com/libretro/beetle-lynx-libretro) · [libretro-handy](https://github.com/libretro/libretro-handy) · [Gearlynx](https://github.com/drhelius/Gearlynx) · ⭐ [Holani（**Rust**）](https://github.com/LLeny/holani) | `lynx/{cart.h,cart.cpp,system.cpp}` / `src/cartridge/{mod.rs,lnx_header.rs,no_intro.rs}` |
| N-Gage | [EKA2L1](https://github.com/EKA2L1/EKA2L1) | `src/emu/loader/{sis.cpp,sis_fields.*,sis_old.*,e32img.*}`、`src/emu/services/src/applist/{applist.cpp,registeration.cpp}` |
| N-Gage | [EKA2L1/Akudama](https://github.com/EKA2L1/Akudama) | MMCDUMP（卡片 CID dump） |
| 3DO | [libretro/opera-libretro](https://github.com/libretro/opera-libretro) | `libopera/discdata.h`、`libretro.c`、`retro_cdimage.c`、`cuefile.{c,h}` |
| 3DO | MAME | [`hash/3do.xml`](https://github.com/mamedev/mame/blob/master/hash/3do.xml)、`hash/3do_m2.xml`、`src/mame/misc/3do.cpp`（⚠ **不在** `src/mame/3do/`） |
| 通用 | ⭐ [RetroAchievements/rcheevos](https://github.com/RetroAchievements/rcheevos) | `src/rhash/{hash.c,hash_disc.c,hash_rom.c,cdreader.c}`、`include/rc_consoles.h` |
| 通用 | MAME software lists | `hash/{wswan,wscolor,ngp,ngpc,lynx,3do,3do_m2}.xml`（⚠ `hash/ngage.xml` **404**） |

### 数据库

| 来源 | URL | 本次用法 |
|---|---|---|
| ⭐ **Redump 官方**（**必须用这个**） | <https://redump.info/downloads/> · `https://redump.info/datfile/<CODE>` | 106 个系统；WIIU / XBOX360 / 3DO / SYMBIAN |
| ⚠ Redump 旧镜像（**已冻结**） | `redump.org`（2026-06-20 起不再更新，60 系统） | 仅作对照 |
| **No-Intro 镜像**（禁止直连 datomatic） | <https://github.com/hugo19941994/auto-datfile-generator> → `releases/latest/download/{no-intro.xml,no-intro.zip}` | 334 个 DAT |
| **libretro-database** | <https://github.com/libretro/libretro-database> | `metadat/{no-intro,redump,tosec,serial,headered,hacks,homebrew}/`、`rdb/` |
| **TOSEC 镜像** | <https://github.com/smesgr9000/TOSEC-DAT>（官方 2025-03-13 release，**4502 个 DAT**） | ⚠ 必须用 `git/trees?recursive=1` 列举，`/contents` API 在 1000 条处截断 |
| TOSEC 官方 | <https://www.tosecdev.org/downloads> · [命名规范](https://www.tosecdev.org/tosec-naming-convention) | `[tr]` 规范 |
| **NoPayStation** | <https://nopaystation.com/> · `tsv/PSV_{GAMES,DEMOS,DLCS,THEMES,UPDATES}.tsv` | Vita zRIF / SHA256 |
| Renascene | <https://renascene.com/psv/> | Vita 元数据（无哈希） |
| 3dt 内建 ID 库 | <https://github.com/trapexit/3dt/blob/master/src/tdo_disc_ids_list.c> | 3DO 596 条卷标映射 |

### Rust crates（crates.io 实测）

| 平台 | 可用 | 不存在 / 干扰项 |
|---|---|---|
| **Xbox 360** | ⭐ `xdvdfs` 0.8.3（18 187 下载）· `xdvdfs-cli` · `xex2` / `stfs` / `xcontent` / `xecrypt` / `lzxc` / `xenon_types`（均 0.1.0，[landaire/acceleration](https://github.com/landaire/acceleration)）· `iso2god` 1.8.0 | `xdvdfs-core`（是目录名非 crate）· `xbox360` · `xiso` · `gdfx` · `xextool`；⚠ `xbox` 0.2.0 是 **Xbox Live 认证库** |
| **3DO** | ⭐ `opticaldiscs` 0.15.0（**已实现 OperaFS**：`src/browse/opera.rs`）· `chd` 0.3.4 · `cue_sheet` / `rcue` | 无专门 OperaFS crate |
| **PS Vita** | `pkg2zip` 0.1.11（Codeberg）· ⭐ `gumshoe` 0.0.4-beta（**已内建完整 Vita 支持**，⚠ 登记仓库 404）· `orbis-unpkg` 0.1.1（`src/psf.rs`） | `param-sfo` · `psvpfs` · `npdrm` 均 **0 结果**；⚠ 所有 `psf*` crate 都是**点阵字体/仿真波形**；所有 `vpk` crate 都是 **Valve Pak** |
| **Wii U** | ⭐ `wux` 0.1.0（**2105 下载，154 行，MIT/Apache**）· `sachet` 0.0.14（NUS/TMD/Ticket/FST，crypto 可关）· `cargo-wiiu` 0.2.2（`src/elf.rs` 全套 RPL 常量）· `zelzip_niiebla` 0.4.0 | `wud` 专用 crate **不存在**；⚠ `nod` 2.0（37k 下载）**不支持 Wii U**（其 "NFS (Wii U VC)" 是 Wii U 包装的 **Wii** 盘）；⚠ `wiiu_swizzle` 是纹理 tiling |
| **Lynx** | ⭐ [Holani](https://github.com/LLeny/holani)（**Rust 实现，但未发布到 crates.io**）· `file-format`（仅 magic 检测） | 搜 `lynx`/`lnx` 全是 HTTP 客户端 |
| **NGPC** | `file-format`（`NeoGeoPocketRom`/`NeoGeoPocketColorRom` **仅 magic，不解析字段**） | 无完整解析 crate |
| **WonderSwan** | **无**（`file-format` 也没有，因文件开头无 magic） | 搜 `wonderswan`/`swan` 全无关 |
| **N-Gage** | **无**（`symbian` / `sisx` 搜索 **0 结果**）；可用配套：`fatfs` 0.3.6 · `flate2` · `crc16` 0.4.0 | 搜 `sis` 全是学生信息系统；搜 `e32` 全是 LoRa 模块 |
| **通用** | ⭐ `datary` 0.3.0（**读写 No-Intro/Logiqx/Redump/TOSEC DAT**）· `shiratsu-naming` 0.1.7（No-Intro/TOSEC/GoodTools 文件名解析）· `rom-weaver-*` 0.13.0 | — |

---

## 调研方法与局限

### 证据等级

本文的结论按可信度分四档：

| 等级 | 说明 | 本文占比（估计） |
|---|---|---|
| **多源交叉验证** | ≥2 个独立一手来源逐字节吻合（如 3DO 的 4 源、WS 的 3 源、NGPC 的 3 源、Lynx 的 6 源、X360 XGD 偏移的 5 源） | 最核心的字段表全部属此类 |
| **【实测】** | 本次直接下载 DAT / 读取源码得出的数字，附可复现命令 | 全部 DAT 覆盖与汉化统计 |
| **单源【事实】** | 只有一个一手来源，但该来源权威（如 Portfolio OS 源码、SNK 官方规格书、Symbian 官方开源码） | 少量 |
| **【推断】** | 本文的推理，已逐条标注 | 已在正文中显式标记 |

### 明确未能取得权威来源的事项

1. **WS**：`$D` 字节到底是 Mapper 还是 RTC 标志（两种读法在实卡上等价，但无来源明说二者关系）
2. **WS**：存档类型码 `$01/$02/$05` 的真实容量（WSdev 与三个模拟器实现不一致）
3. **Lynx**：`rotation` 字段中 `1`/`2` 的确切语义（4 源 vs 1 源互相矛盾）
4. **Lynx**：`LNX2` 头（magic `"LNX2"`）的独立格式规范（仅 Gearlynx 单源）
5. **Lynx**："BLL" 缩写的完整展开
6. **3DO**：`LaunchMe` 内部是否有版本号/序列号字段
7. **3DO**：`ExtDiscLabel` 的 `application_id` 在 Opera(M1) 零售盘上是否被填充
8. **PSV**：`sce_sys/package/head.bin` / `stat.bin` / `tail.bin` / `cert.bin` / `clearsign` / `keystone` 的内部布局
9. **PSV**：MaiDumpTool 的产物格式规范（原作者仓库闭源）
10. **Wii U**：wiiubrew 上**不存在 WUD / WUX / Loadiine 的格式页**（全站 340 页穷举确认），三者只能靠实现源码
11. **Wii U**：`"RPL_CRCS"` / `"RPL_FILEINFO"` 字符串常量在 Cemu 与 decaf 中均不存在（只有宏名），**不能靠 grep 识别**
12. **Wii U**：WUD 明文头 `0x00B`–`0x015` 各字段语义（rom-properties 是唯一来源，且自带两处 `TODO`）
13. **Wii U**：WUX `flags` 的"正确"偏移（原始 C 结构 `0x18` vs JNUSLib `0x0C` 分歧；字段未使用且恒 0，**跳过即可**）
14. **Wii U**：`title.cert` 的解析（Cemu 里是 `// todo - parse certificates`）
15. **Wii U**：GameTDB 站点本身不可达（TLS 握手失败），准确条目数无法核实
16. **X360**：把 XEX TitleID 的 `PublisherID` 明文定义为 ASCII 的一手文档，以及权威发行商代号对照表
17. **X360**：Xbox 360 版 SS.bin 的逐字段结构（大小 2048 已确认，内部布局只有 XGD1 版文档）
18. **X360**：Xenia 额外探测的 `0x0000FB20` / `0x00020600` 两个偏移的用途
19. **X360**：权威的「X360 汉化总数」统计
20. **N-Gage**：`.app` 文件的 UID2 常量（常被引用的 `KUidApp = 0x100039CE` 在两个官方仓库中均未找到定义）
21. **N-Gage**：`.n-gage` 文件（N-Gage 2.0）的内部二进制结构
22. **通用**：Hasheous API 对本文 8 个平台的覆盖情况

### 访问约束的遵守

- **全程未访问 `datomatic.no-intro.org`。** 所有 No-Intro 数据来自 [`hugo19941994/auto-datfile-generator`](https://github.com/hugo19941994/auto-datfile-generator) 的 Release 附件与 [`libretro/libretro-database`](https://github.com/libretro/libretro-database)。
- 本文引用的 [`wiki.no-intro.org`](https://wiki.no-intro.org/) 与 `datomatic.no-intro.org` 是**不同主机**，不在禁令范围。
- 本文提到的 No-Intro Lynx header skipper（`header_lynx.zip`）**其内容取自本机 scratchpad 中的既有副本与 SabreTools 的独立实现**，未重新访问 datomatic。
