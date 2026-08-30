# 模拟器 ROM 自动识别（Identification）技术调研

> **调研日期**：2026-08-30
> **来源约束**：仅采用一手来源 —— 官方规范、平台开发者 wiki、数据库项目自身说明、模拟器/工具源码。
> **标注约定**：
> - **【事实】**（不加标记的正文默认为此类）= 有一手来源 URL 直接支撑
> - **【推断】** = 本文基于事实的推理，**未经来源直接证实**
> - **【实测】** = 本次调研中作者直接向 API / 数据文件发起请求验证的结果，附可复现命令

---

## 目录

- [第 0 章 · 核心结论速览](#第-0-章--核心结论速览)
- [第 A 章 · 各平台 ROM 内部结构化识别信息](#第-a-章--各平台-rom-内部结构化识别信息)
  - [A.1 NES / Famicom](#a1-nes--famicom)
  - [A.2 SNES / SFC](#a2-snes--sfc)
  - [A.3 Game Boy / Game Boy Color](#a3-game-boy--game-boy-color)
  - [A.4 Game Boy Advance](#a4-game-boy-advance)
  - [A.5 Nintendo DS / DSi](#a5-nintendo-ds--dsi)
  - [A.6 Nintendo 3DS](#a6-nintendo-3ds)
  - [A.7 Nintendo 64](#a7-nintendo-64)
  - [A.8 Mega Drive / Genesis / Sega CD / 32X](#a8-mega-drive--genesis--sega-cd--32x)
  - [A.9 Master System / Game Gear](#a9-master-system--game-gear)
  - [A.10 PC Engine / TurboGrafx-16](#a10-pc-engine--turbografx-16)
  - [A.11 PlayStation 1](#a11-playstation-1)
  - [A.12 PlayStation 2](#a12-playstation-2)
  - [A.13 PSP](#a13-psp)
  - [A.14 Sega Saturn](#a14-sega-saturn)
  - [A.15 Sega Dreamcast](#a15-sega-dreamcast)
  - [A.16 GameCube / Wii](#a16-gamecube--wii)
  - [A.17 Arcade / MAME](#a17-arcade--mame)
  - [A.18 平台横向对比](#a18-平台横向对比)
- [第 B 章 · 基于哈希与 DAT 数据库的识别](#第-b-章--基于哈希与-dat-数据库的识别)
  - [B.1 No-Intro](#b1-no-intro)
  - [B.2 Redump（含 CHD 与 Redump 哈希的关系）](#b2-redump)
  - [B.3 TOSEC（含 `[tr zh]` 汉化标记）](#b3-tosec)
  - [B.4 libretro-database 与 RetroArch 的实现](#b4-libretro-database-与-retroarch-的实现)
  - [B.5 ROM 管理器的 header skipper 机制](#b5-rom-管理器的-header-skipper-机制)
  - [B.6 Logiqx XML DAT 格式](#b6-logiqx-xml-dat-格式datafiledtd-v15--三库共用的历史基础)
- [第 C 章 · 汉化版 / 魔改版的识别](#第-c-章--汉化版--魔改版的识别)
  - [C.1 补丁格式与原版可识别特征](#c1-补丁格式哪些格式自带原版-rom-的可识别特征)
  - [C.2 补丁会改动哪些区域](#c2-补丁会改动哪些区域内部头会不会被改)
  - [C.3 现有的结构化 / 模糊识别项目](#c3-现有做结构化模糊识别的项目)
  - [C.4 ⭐ 可用的汉化 ROM 哈希数据源（实测）](#c4-可用的汉化-rom-哈希数据源实测)
  - [C.5 `.rxdelta` 反向补丁 + 扩展属性](#c5-已被验证可行的工程手段rxdelta-反向补丁--扩展属性)
- [第 D 章 · 对自动化刮削工具的可执行建议](#第-d-章--对自动化刮削工具的可执行建议)
  - [D.0 分层总览](#d0-分层总览)
  - [D.1 L0 容器归一化](#d1-l0--容器归一化做对这一步l1-命中率翻倍)
  - [D.2 L1 精确哈希](#d2-l1--精确哈希首选层且对汉化版比想象中有效)
  - [D.3 L2 头部 / 序列号](#d3-l2--头部--序列号)
  - [D.4 L3 结构指纹](#d4-l3--结构指纹)
  - [D.5 L4 文件名解析](#d5-l4--文件名解析)
  - [D.6 L5 模型推断与持久化](#d6-l5--模型推断以及一次识别永久记住)
  - [D.7 推荐的完整流水线](#d7-推荐的完整流水线伪代码)
  - [D.8 具体行动清单](#d8-针对库里大部分是汉化版的具体行动清单)
- [附录 · 一手来源清单](#附录--一手来源清单)

---

## 第 0 章 · 核心结论速览

### 0.1 一句话结论

**ROM 识别的正确架构是「分层置信度」而非「单一算法」**：先做容器归一化 → 精确哈希（**同时算含头/去头两套**）→ 内部序列号 → 结构指纹 → 文件名 → 模型推断，每层都输出置信度与证据来源。

### 0.2 本次调研推翻的两个常见判断

| 常见说法 | 实测结论 |
|---|---|
| ❌「不存在带哈希的汉化 ROM 数据库」 | ✅ **错。TOSEC 用 ISO 639-1 语言码把汉化版正式收录为 `[tr zh]`，带完整 CRC32/MD5/SHA1。** 仅 libretro 的 TOSEC 镜像（33 平台子集）中就实测到 **960 条**中文汉化条目（NES 722、MD 86、GB 74、SFC 48、GBA 26、GBC 4）。另有 BizHawk 随发行版附带的 GoodNES 数据库中 **646 条** `[T+Chi]`/`[T-Chi]` NES 条目与 **172 条** MD 中文条目，以 SHA-1 为键。见 [C.4](#c4-可用的汉化-rom-哈希数据源实测) |
| ❌「RetroArch 会剥离 iNES/SMC 头再匹配」 | ✅ **错。RetroArch 的扫描器完全没有头剥离逻辑。** 它把问题推给数据库 —— No-Intro NES DAT 同时收录 `.nes`（7066 条，含头）与 `.unh`（7065 条，无头）两套记录。**代价是：SNES 没有含头变体记录，所以带 512 字节 copier 头的 `.smc` 在 RetroArch 严格扫描下无法匹配。** 见 [B.4.5](#b45--retroarch-扫描器不做任何头部剥离) |

### 0.3 五个最有操作价值的发现

1. **⭐ Hasheous 免费 API 实测可直接识别汉化 ROM**
   `GET https://hasheous.org/api/v1/Lookup/ByHash/sha1/{sha1}` —— 无需 API key。实测用汉化 NES ROM `1942 [tr zh MS emumax]` 的 SHA-1 查询，返回游戏名、平台、发行商与 IGDB/TGDB/RetroAchievements 链接。SNES 汉化版同样命中。见 [D.2.2](#d22--最省事的做法直接用-hasheous-api)

2. **⭐ 必须同时计算「含头」与「去头」两套哈希**
   No-Intro headerless DAT 按**去头**哈希，而 **TOSEC 按含头原样哈希**。只算一套就会丢掉 TOSEC 里全部汉化条目。这是一次读取就能完成的零成本操作（RomVault 的 `Alt*` 字段、igir 都这么做）。见 [D.1](#d1-l0--容器归一化做对这一步l1-命中率翻倍)

3. **⭐ 补丁文件的 footer 直接给出原版 CRC32**
   **BPS / UPS** 在文件末尾 `len-12` 处存有源 ROM 的 CRC32；**RUP/NINJA2** 存源与目标的 MD5；**APS(N64)** 存卡带 ID + 区域 + 内部 CRC；**APS(GBA)** 存每 64 KB 一个 CRC16。
   ⚠ **但 IPS 一个字节的源信息都没有** —— 而 IPS 恰恰主导 FC/NES 汉化。见 [C.1](#c1-补丁格式哪些格式自带原版-rom-的可识别特征)

4. **⭐ RetroAchievements 的 `rc_hash` 是现成可复用的「结构不变量」方案**
   对 **PS1/PS2/PSP** 它只哈希**启动可执行文件**（`MD5(EXE 文件名 ‖ EXE 字节)`），光盘其余部分完全不参与。**这意味着只改数据文件、不改主 EXE 的光盘汉化，其 RA 哈希与原版完全一致。** 见 [C.3.4](#c34--retroachievements-的-rc_hash--逐平台自定义哈希最有价值的现有实践)

5. **⭐ 「一次识别，永久记住」比任何模糊匹配都更有价值**
   `rhdndat` 的做法已在实践中运行：SHA-1 匹配 DAT，用 `.rxdelta`（xdelta3 反向补丁）把硬补丁 ROM 还原回原版，并把原版 SHA-1 写入文件**扩展属性** `user.rhdndat.rom_sha1`。见 [C.5](#c5-已被验证可行的工程手段rxdelta-反向补丁--扩展属性)

### 0.4 必须接受的现实

| 事实 | 影响 |
|---|---|
| **No-Intro 与 Redump 在政策上不收录任何修改版** | 汉化版永远不可能匹配这两个库。No-Intro 的 `(Aftermarket)`/`(Unl)`/`(Pirate)` 覆盖的是**未授权原创游戏**，不是修改过的 dump |
| **GBA 与 NDS 的汉化覆盖最差** | TOSEC GBA 仅 26 条、**NDS 无 TOSEC DAT**；BizHawk GBA/NDS gamedb 为 0；RetroAchievements 中文仅 13 条。**而这正是中文玩家库存量最大的两个平台** |
| **ROM 领域没有任何模糊哈希（ssdeep/TLSH）先例** | 唯一实际部署的分片方案是 APS(GBA) 的每 64 KB CRC16。且 ssdeep 在扩容 >2 倍时**直接返回 0 分无法比较**，而汉化扩容 2–6 倍是常态 |
| **文件大小不是不变量** | 实测 libretro hacks DAT：`Metroid - Rogue Dawn` 为 786448 字节，原版 `Metroid (USA)` 仅 128KB —— **扩容 6 倍**。绝不能用文件大小分桶 |
| **DAT-o-MATIC 会因单个畸形 URL 参数永久封 IP** | 实测触发；解封需人工邮件申请。**不要自建 No-Intro 抓取器**，改用 libretro-database 的 git 镜像 |
| **romhacking.net 已于 2024-08-01 转为只读** | 全库导出到 archive.org，其中 `romhacking.sql.zip` 仅 6.8 MB，含每个 hack 的基准 ROM 哈希 —— 但需登录账号才能下载 |

### 0.5 立即可做的四件事

| # | 行动 | 收益 |
|---|---|---|
| 1 | **在 No-Intro / Redump 之外引入 TOSEC DAT** | 立刻覆盖 960+ 条中文汉化版 |
| 2 | **同时计算含头 / 去头两套 CRC32+MD5+SHA1** | 修复 NES `.nes`/`.unh`、SNES `.smc` 的全部匹配失败，并让 TOSEC 可命中 |
| 3 | **接入 Hasheous API 作在线兜底** | 零成本，覆盖离线 DAT 之外的长尾 |
| 4 | **把人工确认结果写入 xattr / SQLite 持久化** | 对以汉化版为主的库，边际收益随时间线性增长 |

---

## 第 A 章 · 各平台 ROM 内部结构化识别信息

> **本章表格通用说明**
> - **偏移量**均为**文件内偏移**（已注明需要先剥离外挂头 / 归一化字节序的情形）。
> - **可靠性**一列是本文对「该字段用于唯一识别一份 ROM」的评级，属 **【推断】**；字段本身的定义与偏移均为【事实】。
> - 评级含义：`极高` = 可单独作主键；`高` = 组合后可作主键；`中` = 可作交叉校验 / 分类；`低` = 仅供参考；`不可用` = 实测常被写错，不应依赖。

### A.1 NES / Famicom

来源：[NESdev Wiki – INES](https://www.nesdev.org/wiki/INES)、[NESdev Wiki – NES 2.0](https://www.nesdev.org/wiki/NES_2.0)

#### A.1.1 iNES 文件整体布局

| # | 区段 | 长度 | 说明 |
|---|---|---|---|
| 1 | Header | 16 bytes | 固定 |
| 2 | Trainer（可选） | 0 或 **512** bytes | 「512-byte trainer at $7000-$71FF (stored before PRG data)」 |
| 3 | PRG ROM | 16384 × x | |
| 4 | CHR ROM（可选） | 8192 × y | |
| 5 | PlayChoice INST-ROM（可选） | 0 或 8192 | |
| 6 | PlayChoice PROM（可选） | 16 + 16 | |

> **【推断】** trainer 存在时 PRG 数据整体后移 512 字节。任何「固定跳过 16 字节取 PRG」的实现在 trainer ROM 上都会错位。

#### A.1.2 iNES 16 字节头

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| magic | `0x00`–`0x03` | 4 | 常量 `4E 45 53 1A`（"NES" + MS-DOS EOF） | **极高**（仅用于判定文件类型） |
| PRG ROM size (LSB) | `0x04` | 1 | 16 KB 单位 | 中 |
| CHR ROM size (LSB) | `0x05` | 1 | 8 KB 单位；`0` = 使用 CHR RAM | 中 |
| Flags 6 | `0x06` | 1 | bit7-4 mapper 低半字节；bit3 四屏；**bit2 trainer**；bit1 电池；bit0 镜像 | 低–中 |
| Flags 7 | `0x07` | 1 | bit7-4 mapper 高半字节；**bit3-2 = `0b10` 表示 NES 2.0**；bit1 PlayChoice-10；bit0 VS Unisystem | 低–中（bit3-2 关键） |
| Flags 8 (PRG-RAM size) | `0x08` | 1 | 8 KB 单位；`0` 视为 8 KB | 极低 |
| Flags 9 (TV system) | `0x09` | 1 | bit0：0=NTSC / 1=PAL | 极低 |
| Flags 10（非官方） | `0x0A` | 1 | bit1-0 TV system；bit2 PRG RAM presence；bit3 bus conflicts | 极低 |
| padding | `0x0B`–`0x0F` | 5 | 「should be filled with zero, but **some rippers put their name across bytes 7-15**」 | **不可用** |

#### A.1.3 NES 2.0 扩展字段

判定：`(header[7] & 0x0C) == 0x08`。

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| Mapper MSB / Submapper | `0x08` | 1 | `SSSS NNNN`：S=**Submapper**，N=Mapper D11-D8 | 中–高 |
| PRG/CHR Size MSB | `0x09` | 1 | `CCCC PPPP` | 中 |
| PRG-RAM / EEPROM Size | `0x0A` | 1 | 高/低半字节各为 shift；大小 = `64 << shift` 字节 | 低 |
| CHR-RAM Size | `0x0B` | 1 | 同上 | 低 |
| CPU/PPU Timing | `0x0C` | 1 | 0=NTSC，1=PAL，2=Multiple，3=Dendy | 低（区域辅助） |
| Vs. / Extended Console Type | `0x0D` | 1 | — | 低 |
| Miscellaneous ROMs | `0x0E` | 1 | 杂项 ROM 芯片数 | 低 |
| Default Expansion Device | `0x0F` | 1 | — | 低 |

**指数-乘数记法（exponent-multiplier）**：当对应 MSB 半字节 = `$F` 时，LSB 字节重解释为 `EEEEEEMM`，大小 = `2^E × (MM×2 + 1)` **字节**（此时单位是字节而非 16KiB/8KiB）。

#### A.1.4 变体判定（nesdev 官方推荐流程，原文四步）

1. `byte7 & 0x0C == 0x08` **且** 按 byte9 算出的尺寸不超过实际文件大小 → **NES 2.0**
2. `byte7 & 0x0C == 0x04` → **archaic iNES**
3. `byte7 & 0x0C == 0x00` **且** bytes 12–15 全为 0 → **iNES 1.0**
4. 其他 → **iNES 0.7 或 archaic iNES**

> **【推断】** 第 3 步的「bytes 12-15 全 0」是唯一能区分「干净 iNES 1.0」与「被 ripper 污染的脏头」的判据。最著名的污染是 `"DiskDude!"`，它令 `byte7 = 'D' = 0x44`，`0x44 & 0x0C = 0x04`，从而被误判为 archaic iNES。**结论：NES 头部字节整体不可信，识别绝不能依赖它。**

#### A.1.5 无头 ROM 与分段哈希 —— NES 识别的核心问题

**关键事实：iNES 格式本身不含任何校验和字段。** 对 [nesdev INES 页](https://www.nesdev.org/wiki/INES) 全页检索，未出现 CRC / checksum / hash。NES 是本文所有平台中**唯一完全没有自校验结构**的。

**No-Intro 的双 DAT 方案（实测）**：libretro 镜像的 No-Intro NES DAT（version 2026.08.01）中同一游戏有两条记录：

```
game ( name "Super Mario Bros. (World)"
  rom ( name "Super Mario Bros. (World).nes" size 40976 crc 393A432F sha1 33D23C2F2CFA4C9EFEC87F7BC1321CE3CE6C89BD ) )
game ( name "Super Mario Bros. (World)"
  rom ( name "Super Mario Bros. (World).unh" size 40960 crc D445F698 sha1 FACEE9C577A5262DBE33AC4930BB0B58C8C037F7 ) )
```

| 变体 | 扩展名 | size | 关系 |
|---|---|---|---|
| headered | `.nes` | 40976 | = headerless + 16 字节 iNES 头 |
| headerless | `.unh` | 40960 | 纯 PRG + CHR |

该 DAT 中 `.nes` 条目 7066 条、`.unh` 条目 7065 条，基本一一配对。40976 − 40960 = 16，与 iNES 头长度吻合。
来源：[libretro-database `metadat/no-intro/`](https://github.com/libretro/libretro-database/tree/master/metadat/no-intro)

**模拟器如何处理无头 .nes（Mesen2 源码）**：
`Core/NES/Loaders/RomLoader.cpp` —— 先对**整个文件**算 CRC32，若首 4 字节不是任何已知 magic，则**用该 CRC32 查内置游戏数据库，由数据库返回一个合成的 iNES 头**，再走正常加载路径；查不到就判为无效文件。
来源：[Mesen2](https://github.com/SourMesen/Mesen2)

**分段（PRG/CHR）哈希 —— 确实存在**：

| 来源 | 粒度 | 说明 |
|---|---|---|
| **NesCartDB（bootgod）XML DTD** | **逐芯片** | `<prg>` / `<chr>` 元素各自带 `crc` 与 `sha1` 属性，`<cartridge>` 层再带整体 `crc`/`sha1`。来源：[NESdev 论坛 DTD 存档](https://archive.nes.science/nesdev-forums/f2/t5998.xhtml) |
| **Mesen2 `PrgChrCrc32`** | PRG+CHR 合并段 | `Core/NES/Loaders/iNesLoader.cpp`：跳过 16 字节头 **+ 512 字节 trainer（若有）** 后到 EOF 的 CRC32。Mesen 用它反查数据库以**修正错误的头部尺寸字段** |
| **MAME software list** | 逐 dataarea | `hash/nes.xml` 中 PRG 与 CHR 是分开的 `<dataarea>`，各有独立 `crc`/`sha1`（详见 B 章） |
| nesdev iNES wiki 页 | 无 | 已核实：完全未提及 CRC |

---

### A.2 SNES / SFC

来源：[SNESdev Wiki – ROM header](https://snes.nesdev.org/wiki/ROM_header)、[fullsnes (no$sns / Martin Korth)](https://problemkaputt.de/fullsnes.htm)

#### A.2.1 内部头的四个候选位置

fullsnes 原文：
> "In ROM-images it is found at offset **007Fxxh (LoROM)**, **00FFxxh (HiROM)**, or **40FFxxh (ExHiROM)**; add **+200h** to that offsets if **`(imagesize AND 3FFh)=200h`**"

| Map 模式 | 主头偏移 | 扩展头偏移 |
|---|---|---|
| LoROM | `0x007FC0` | `0x007FB0` |
| HiROM | `0x00FFC0` | `0x00FFB0` |
| ExHiROM | `0x40FFC0` | `0x40FFB0` |
| ExLoROM | `0x407FC0` | `0x407FB0` |

> 存在 512 字节 copier 头时上述全部 **+0x200**。ExLoROM 未见于 fullsnes / snes.nesdev，但 **bsnes 会对该位置评分**。

#### A.2.2 主头字段（`base` = `0x7FC0` / `0xFFC0` / `0x40FFC0`）

| 字段名 | 相对偏移 | LoROM 偏移 | HiROM 偏移 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|---|---|
| ROM name / Cartridge title | `+0x00` | `0x7FC0` | `0xFFC0` | **21** | 大写 ASCII，空格（`$20`）补白 | 中–高（补白方式不统一；日文卡用 Shift-JIS 半角片假名） |
| Map mode / ROM speed | `+0x15` | `0x7FD5` | `0xFFD5` | 1 | bit5 恒 1；bit4 Speed；bit3-0 Map Mode | 中（定位交叉验证关键） |
| Cartridge type / Chipset | `+0x16` | `0x7FD6` | `0xFFD6` | 1 | ROM/RAM/电池/协处理器组合 | 低–中 |
| ROM size | `+0x17` | `0x7FD7` | `0xFFD7` | 1 | `(1 << N)` KB，向上取整 | 低–中 |
| RAM size | `+0x18` | `0x7FD8` | `0xFFD8` | 1 | `(1 << N)` KB | 低 |
| Country / Region | `+0x19` | `0x7FD9` | `0xFFD9` | 1 | 见下表；隐含 PAL/NTSC | **不可用**（见 A.2.5） |
| Developer / Maker code (old) | `+0x1A` | `0x7FDA` | `0xFFDA` | 1 | `00h`=None/Homebrew；**`33h` = 启用扩展头** | 中（`33h` 是开关，价值在此） |
| ROM version | `+0x1B` | `0x7FDB` | `0xFFDB` | 1 | `00h` = 初版 | 中 |
| Checksum complement | `+0x1C` | `0x7FDC` | `0xFFDC` | 2 (LE) | = Checksum XOR `$FFFF` | 高（定位判据） |
| Checksum | `+0x1E` | `0x7FDE` | `0xFFDE` | 2 (LE) | 全 ROM 字节和 | 中–高 |
| Interrupt vectors | `+0x20` | `0x7FE0` | `0xFFE0` | 32 | RESET vector 在 `+0x3C`（= `$FFFC`） | **高（用于定位）** |

**Map Mode 常见完整值**：`0x20` LoROM/Slow · `0x21` HiROM/Slow · `0x30` LoROM/Fast · `0x31` HiROM/Fast · `0x35` ExHiROM/Fast · `0x23` SA-1 · `0x22` S-DD1。

#### A.2.3 扩展头（`base − 0x10`，触发条件 `+0x1A == 0x33`）

| 字段名 | 相对偏移 | LoROM 偏移 | HiROM 偏移 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|---|---|
| Maker code | `−0x10` | `0x7FB0` | `0xFFB0` | 2 | 大写 ASCII，如 `"01"`=Nintendo | 中–高 |
| **Game code** | `−0x0E` | `0x7FB2` | `0xFFB2` | **4** | 大写 ASCII 4 字符；旧式为 2 字符 + 2 空格 | **高**（SNES 上最接近唯一序列号的字段） |
| Reserved | `−0x0A` | `0x7FB6` | `0xFFB6` | 6 | 必须为 0 | 有效性校验 |
| Expansion FLASH size | `−0x04` | `0x7FBC` | `0xFFBC` | 1 | `(1 << N)` KB | 极低 |
| Expansion RAM size | `−0x03` | `0x7FBD` | `0xFFBD` | 1 | `(1 << N)` KB | 极低 |
| Special version | `−0x02` | `0x7FBE` | `0xFFBE` | 1 | 通常 0 | 低 |
| Chipset sub-type | `−0x01` | `0x7FBF` | `0xFFBF` | 1 | `+0x16 == Fxh` 时给出定制协处理器类型 | 低（SPC7110/CX4/ST01x 判定必需） |

#### A.2.4 校验和算法 —— 两个来源措辞不一致

| 来源 | 表述 |
|---|---|
| [snes.nesdev](https://snes.nesdev.org/wiki/ROM_header) | 计算前把校验和字段清零，对全部字节求和（16 位溢出丢弃），complement = checksum XOR `0xFFFF` |
| [fullsnes](https://problemkaputt.de/fullsnes.htm) | 「all bytes in ROM added together; **assume `[FFDC-F] = FF,FF,00,00`**」 |

> **【推断】** 两者相差常数 `0x1FE`。fullsnes 的版本是自洽的：complement + checksum 恒等于 `0xFFFF`，故这 4 字节的**字节和**恒为 `0xFF+0xFF+0x00+0x00 = 0x1FE`，与实际填值无关。**实现时建议采用 fullsnes 定义并用已知良品 ROM 验证。**

fullsnes 另给出非 2 的幂 ROM 的分段校验方式（实用）：
> "the 'bigger half' is mapped to address 0, followed by the 'smaller half', then followed by mirror(s) of the smaller half. Eg. a 10MBit game would be rounded to 16MBit, and mapped (and checksummed) as **'8Mbit + 4×2Mbit'**."

| 游戏 | 大小 | Checksum 计算方式 |
|---|---|---|
| Dai Kaiju Monogatari 2 (J) | 5MB | 4MB + 4 × Last 1MB |
| Star Ocean (J) | 6MB | 4MB + 2 × Last 2MB |
| Momotaro Dentetsu Happy (J) | 3MB | 2 × 3MB |
| Sufami Turbo BIOS / Games | — | **无校验和** |

#### A.2.5 头部位置检测启发式 —— bsnes 打分算法（源码）

来源：[bsnes `bsnes/heuristics/super-famicom.cpp`](https://github.com/bsnes-emu/bsnes)

对 `0x7FB0` / `0xFFB0` / `0x407FB0` / `0x40FFB0` 四点分别打分，取最高者；ExLoROM/ExHiROM 得分非 0 时额外 +4。

| 规则 | 分值 |
|---|---|
| 前置：`size() < address + 0x50` | 直接 0 分 |
| 前置：`resetVector < 0x8000`（"$00:0000-7fff is never ROM data"） | 直接 0 分 |
| RESET 入口首指令 ∈ {`78 sei`, `18 clc`, `38 sec`, `9c stz`, `4c jmp`, `5c jml`} | **+8** |
| ∈ {`c2 rep`, `e2 sep`, `ad lda`, `ae ldx`, `ac ldy`, `af lda long`, `a9 lda#`, `a2 ldx#`, `a0 ldy#`, `20 jsr`, `22 jsl`} | **+4** |
| ∈ {`40 rti`, `60 rts`, `6b rtl`, `cd cmp`, `ec cpx`, `cc cpy`} | **−4** |
| ∈ {`00 brk`, `02 cop`, `db stp`, `42 wdm`, `ff sbc`} | **−8** |
| `checksum + complement == 0xFFFF` | **+4** |
| `address == 0x7fb0 && mapMode == 0x20` | **+2** |
| `address == 0xffb0 && mapMode == 0x21` | **+2** |

> opcode 取址为 `data[(address & ~0x7fff) | (resetVector & 0x7fff)]`。

**Snes9x（`memmap.cpp`）额外判据（值得借鉴）**：
- `allASCII(&buf[0xc0], ROM_NAME_LEN-1)` —— **标题必须全 ASCII**，否则 −1
- `allASCII(&buf[0xb0], 6)` —— 扩展头前 6 字节全 ASCII，否则 −1
- `!(buf[0xfd] & 0x80)` → **−6**（RESET vector 必须 ≥ `$8000`）

**⚠ region 字段不可信 —— bsnes 源码注释原文**：
> "Unlicensed software (homebrew, ROM hacks, etc) often change the standard region code, and then neglect to change the extended header region code. Thanks to that, **we can't decode and display the full game serial + region code**."

bsnes 因此**直接短路了**完整 region 解码，只返回 `videoRegion()`。这是「不要信任 SNES 区域字段」的权威背书，**对识别汉化/魔改 ROM 尤其重要**。

#### A.2.6 SMC / copier 512 字节头

| 来源 | 检测规则 | 等价表述 |
|---|---|---|
| [fullsnes](https://problemkaputt.de/fullsnes.htm) | `IF (filesize AND 3FFh)=200h THEN HeaderPresent=True` | `filesize % 1024 == 512` |
| [snes.nesdev](https://snes.nesdev.org/wiki/ROM_header) | 同上 | 同上 |
| **bsnes 源码** | `if((size() & 0x7fff) == 512)` | **`filesize % 32768 == 512`** |
| **igir 源码** | 偏移 3 起 509 个 `00` 字节，头长 512 | 见 B 章 |

fullsnes 原文：
> "Many of these files do have 512-byte headers. **The headers don't contain any useful information. So, if they are present: Just ignore them.** … Headerless cartridges are always sized N×1024 bytes, Carts with header are N×1024+512 bytes."

> **【推断】** bsnes 的 mod 32768 比文档的 mod 1024 严格。对合法 ROM（总是 32KB 整数倍）两者一致；对截断/拼接过的异常文件会分歧。
> **⚠ 扩展名完全不可作判据** —— fullsnes 原文：`.SMC` "is often used for **ANY** type of SNES ROM-images"。

**copier 头内可用的 magic**：`.SWC`（Super Wild Card）在 `0x008`–`0x009` 有 File ID `AA BB`。

---

### A.3 Game Boy / Game Boy Color

来源：[Pan Docs – The Cartridge Header](https://gbdev.io/pandocs/The_Cartridge_Header.html)
**GB ROM 文件无外挂头，文件偏移 == 卡带地址。**

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| Entry point | `0x0100`–`0x0103` | 4 | 多数商业卡为 `00 C3 50 01`（`nop; jp $0150`） | 低 |
| **Nintendo logo** | `0x0104`–`0x0133` | **48** | 固定位图，boot ROM 校验失败则死机 | **极高（magic）/ 零（区分游戏）** |
| Title | `0x0134`–`0x0143` | 16 | 大写 ASCII，`$00` 补白。**「Parts of this area actually have a different meaning on later cartridges, reducing the actual title size to 15 (`$0134`–`$0142`) or 11 (`$0134`–`$013E`)」** | 中–高 |
| Manufacturer code | `0x013F`–`0x0142` | 4 | 「In older cartridges these bytes were part of the Title」「The purpose of the manufacturer code is unknown」 | 中（新卡近似序列号；**老卡上是标题的一部分，会污染解析**） |
| CGB flag | `0x0143` | 1 | `$80` = 支持 CGB 且兼容单色；`$C0` = 仅 CGB | 中 |
| New licensee code | `0x0144`–`0x0145` | 2 | 仅当 Old licensee == `$33` 时有意义 | 低–中 |
| SGB flag | `0x0146` | 1 | 非 `$03` 时 SGB 忽略命令包 | 低 |
| Cartridge type | `0x0147` | 1 | MBC 类型（`$00` ROM ONLY … `$FF` HuC1+RAM+BATTERY） | 低（分类用） |
| ROM size | `0x0148` | 1 | `32 KiB × (1 << value)` | 中（可与文件大小交叉校验） |
| RAM size | `0x0149` | 1 | `$00`=0，`$02`=8KiB，`$03`=32KiB，**`$04`=128KiB，`$05`=64KiB**（注意乱序） | 低 |
| Destination code | `0x014A` | 1 | `$00` = Japan，`$01` = Overseas only | 低–中 |
| Old licensee code | `0x014B` | 1 | **`$33` 表示改用 New licensee code** | 低–中 |
| Mask ROM version | `0x014C` | 1 | 通常 `$00` | 中（区分修订版关键） |
| **Header checksum** | `0x014D` | 1 | 见下 | 高（一致性校验） |
| **Global checksum** | `0x014E`–`0x014F` | 2（**big-endian**） | 见下 | 中–高（弱识别键） |

**Nintendo logo 精确字节**（Pan Docs 原文）：
```
CE ED 66 66 CC 0D 00 0B 03 73 00 83 00 0C 00 0D
00 08 11 1F 88 89 00 0E DC CC 6E E6 DD DD D9 99
BB BB 67 63 6E 0E EC CC DD DC 99 9F BB B9 33 3E
```
> **「The CGB and later models only check the top half of the logo (the first `$18` bytes)」**（即 `0x0104`–`0x011B`）。
> mGBA 的实现只比对**前 4 字节 `CE ED 66 66`**，并额外识别 Sachen 盗版 logo `7C E7 C0 00` 及两种打乱布局（`src/gb/gb.c`）。

**Header checksum 算法**（Pan Docs 原文，boot ROM **会**校验，失败则不启动）：
```c
uint8_t checksum = 0;
for (uint16_t address = 0x0134; address <= 0x014C; address++)
    checksum = checksum - rom[address] - 1;
```
覆盖 `0x0134`–`0x014C` 共 **25 字节**，不含 logo、不含自身、不含 global checksum。

> **【推断】** 这是一个强「元数据一致性」指纹：任何对标题/版本/类型的篡改都会破坏它，且真机拒绝启动 —— 所以**合法 ROM 上它必然正确**。反过来，**若一份汉化 ROM 改了标题但 header checksum 仍然自洽，说明汉化者修复了它**（多数汉化工具会）。

**Global checksum 算法**（Pan Docs 原文）：
> 「These bytes contain a **16-bit (big-endian)** checksum simply computed as the **sum of all the bytes of the cartridge ROM (except these two checksum bytes)**.」
> 「**This checksum is not verified**, except by Pokémon Stadium's 'GB Tower' emulator.」

> **【推断】** Global checksum 覆盖整个 ROM，是一个 16-bit 的全 ROM 指纹。碰撞概率约 1/65536，单独用不够，但 **「Title + Global checksum + Mask ROM version」三元组基本能唯一定位一个 GB ROM**，计算成本远低于全文件 SHA-1，适合快速预筛。

---

### A.4 Game Boy Advance

来源：[GBATEK – GBA Cartridge Header](https://problemkaputt.de/gbatek-gba-cartridge-header.htm)
**GBA ROM 文件无外挂头，文件偏移 == 卡带偏移。**

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| ROM Entry Point | `0x000`–`0x003` | 4 | 32-bit ARM 分支指令（`B rom_start`），**最高字节恒为 `0xEA`** | 中（文件类型判据） |
| **Nintendo Logo** | `0x004`–`0x09F` | **156** | 压缩位图，BIOS 逐字节比对，不符则死机 | 极高（magic）/ 零（区分游戏） |
| Game Title | `0x0A0`–`0x0AB` | 12 | 大写 ASCII，`00h` 补白 | 中–高 |
| **Game Code** | `0x0AC`–`0x0AF` | **4** | 「the same code as the **AGB-UTTD** code which is printed on the package and sticker」 | **极高**（GBA 上的官方唯一序列号） |
| Maker Code | `0x0B0`–`0x0B1` | 2 | 大写 ASCII，`"01"`=Nintendo | 中 |
| **Fixed value** | `0x0B2` | 1 | **必须为 `96h`** | **极高（magic）** |
| Main unit code | `0x0B3` | 1 | 现行 GBA 恒为 `00h` | 极低 |
| Device type | `0x0B4` | 1 | 通常 `00h`；bit7 与 DACS 调试相关 | 极低 |
| Reserved | `0x0B5`–`0x0BB` | 7 | 应为 0 | 辅助校验 |
| Software version | `0x0BC` | 1 | 通常 `00h` | 中（区分修订版） |
| **Complement check** | `0x0BD` | 1 | 见下，「cartridge won't work if incorrect」 | 高（校验用） |
| Reserved | `0x0BE`–`0x0BF` | 2 | 应为 0 | 辅助校验 |
| RAM Entry Point | `0x0C0`–`0x0C3` | 4 | Multiboot 专用 | — |
| Boot mode | `0x0C4` | 1 | 「BIOS overwrites this value!」 | **不可用** |
| Slave ID | `0x0C5` | 1 | 同上 | **不可用** |
| JOYBUS Entry Point | `0x0E0`–`0x0E3` | 4 | — | — |

**Complement check 算法**（GBATEK 原文）：
```
chk=0 : for i=0A0h to 0BCh : chk=chk-[i] : next : chk=(chk-19h) and 0FFh
```
覆盖 `0x0A0`–`0x0BC` 共 **29 字节**（Title + Game Code + Maker Code + `96h` + 主机码 + 设备类型 + 7 保留 + 版本），魔术常数 `−0x19`，**硬件强制校验**。

**Game Code 结构 `UTTD`**（GBATEK 原文）：

| 位 | 含义 |
|---|---|
| `U` | Unique Code：`A`=2001–2003 常规 · `B`=2003+ 常规 · `C`=预留 · **`F`=Classic NES Series** · **`K`=倾斜传感器（Yoshi/Koro Koro）** · **`P`=e-Reader** · **`R`=震动+陀螺仪（Warioware Twisted）** · **`U`=RTC+太阳能传感器（Boktai）** · **`V`=震动（Drill Dozer）** |
| `TT` | Short Title，游戏名缩写（如 `PM` = Pac Man）；「unless that gamecode was already used for another game, then TT is just random」 |
| `D` | Destination/Language：`J` Japan · `E` USA/English · `P` Europe/Elsewhere · `D` German · `F` French · `S` Spanish · `I` Italian |

> **【推断】** `U` 位不只是年代标记，而是**直接指示特殊卡带硬件**（震动、陀螺仪、光敏、RTC、e-Reader），对模拟器配置与 ROM 分类价值很高。
> **【推断】** GBATEK 只列了 7 个 `D` 字母。实际 No-Intro 集中还常见 `K`(Korea)、`C`(China/iQue)、`U`(Australia)、`X`/`Y`(欧洲多语种)，但**未被 GBATEK 文档化**，属社区惯例。

**⚠ Nintendo Logo 不是 100% 固定**（GBATEK 原文明确列出两个例外）：

| 字段 | 偏移量 | 常规值 | 可变位 |
|---|---|---|---|
| Debugging Enable | `0x09C` | `21h` | **bit 2、bit 7**（两者都置位即 `A5h` 时解锁 BIOS 的 FIQ/Undefined 处理转发） |
| Cartridge Key Number MSBs | `0x09E` | `F8h` | **bit 0、bit 1** |

> **【推断】** 若用整段 156 字节 logo 做 magic 比对，必须对 `0x09C` 屏蔽 `0x84`、对 `0x09E` 屏蔽 `0x03`，否则会误杀少量合法卡带。

**mGBA 的 GBA 文件判定规则**（`src/gba/gba.c`，`GBAIsROM`）：
1. `file[0x03] == 0xEA`
2. `file[0x0B2] == 0x96`
3. 若第 2 条不满足，允许例外：`0x004`–`0x09F`（156 字节 logo 区）**全为 0** —— 即未经 `gbafix` 处理的 "unfixed ROM"
4. 排除 GBA BIOS 文件

> **【推断】** `file[0x03] == 0xEA && file[0xB2] == 0x96` 是最实用的 GBA magic：只需 2 字节，误判率极低，且能容忍被改过的 logo。

---

### A.5 Nintendo DS / DSi

来源：[GBATEK – DS Cartridge Header](https://www.problemkaputt.de/gbatek-ds-cartridge-header.htm)、[DSi Cartridge Header](https://www.problemkaputt.de/gbatek-dsi-cartridge-header.htm)、[DS Cartridge Icon/Title](https://www.problemkaputt.de/gbatek-ds-cartridge-icon-title.htm)

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| Game Title | `0x000` | 12 | 大写 ASCII，`00h` 补白 | 中 |
| **Gamecode** | `0x00C` | **4** | 大写 ASCII，`NTR-<code>`；`0` = homebrew | **极高**（DS 主识别键） |
| Makercode | `0x010` | 2 | 大写 ASCII，`"01"`=Nintendo | 高（组合键的一部分） |
| Unitcode | `0x012` | 1 | `00h`=NDS，`02h`=NDS+DSi，`03h`=DSi | 平台判别 |
| Encryption Seed Select | `0x013` | 1 | `00..07h` | 无 |
| Devicecapacity | `0x014` | 1 | **Chipsize = `128KB << n`** | 低（合理性检查） |
| NDS Region | `0x01D` | 1 | `00h`=Normal，`80h`=China，`40h`=Korea | 中 |
| **ROM Version** | `0x01E` | 1 | 通常 `00h` | **高** |
| Autostart | `0x01F` | 1 | bit2 跳过健康警告 | 无 |
| ARM9 rom_offset / entry / ram_address / size | `0x020`–`0x02F` | 4 ×4 | — | — |
| ARM7 rom_offset / entry / ram_address / size | `0x030`–`0x03F` | 4 ×4 | — | — |
| FNT offset / size | `0x040` / `0x044` | 4 / 4 | File Name Table | — |
| FAT offset / size | `0x048` / `0x04C` | 4 / 4 | File Allocation Table | — |
| **Icon/Title offset** | `0x068` | 4 | `0`=None；`8000h` 及以上 | 高（多语言标题入口） |
| **Secure Area CRC-16** | `0x06C` | 2 | 覆盖 `[[020h] .. 00007FFFh]` | 中 |
| Secure Area Delay | `0x06E` | 2 | — | 无 |
| Total Used ROM size | `0x080` | 4 | 余下通常 `FFh` 填充 | 中（trim 检测） |
| ROM Header Size | `0x084` | 4 | `4000h` | 合理性检查 |
| Nintendo Logo | `0x0C0` | `0x9C` | 压缩位图，与 GBA 头相同 | 高（格式嗅探） |
| **Nintendo Logo CRC-16** | `0x15C` | 2 | 覆盖 `[0C0h-15Bh]`，**固定值 `CF56h`** | **极佳的格式嗅探常量** |
| **Header CRC-16** | `0x15E` | 2 | 覆盖 `[000h-15Dh]` | **极佳的有效性校验** |

> ⚠ **常见错误更正**：`0x15C` **不是** secure-area CRC，而是 **Nintendo Logo 的 CRC16（固定 `CF56h`）**；Secure Area CRC16 在 **`0x06C`**。
> GBATEK 原文：「the BIOS verifies only `[15Ch]=CF56h`, it does NOT verify the actual data at `[0C0h-15Bh]`」

**CRC-16 算法**：GBATEK 在 `SWI 0Eh GetCRC16` 给出表 `C0C1,C181,C301,C601,CC01,D801,F001,A001`。
参考实现：[devkitPro ndstool `source/crc.h`](https://github.com/devkitPro/ndstool/blob/master/source/crc.h) —— `crc = (crc >> 8) ^ crc16tab[(crc ^ data[i]) & 0xFF]`，init `0xFFFF`，无最终取反。
形式上即 **CRC-16/MODBUS**（poly `0x8005` 反射 = `0xA001`，init `0xFFFF`，refin/refout=true，xorout=0，`"123456789"` 校验值 `0x4B37`）。

> ⚠ **【推断】** GBATEK 的 `val[j] shl (7-j)` 伪代码逐字实现**无法**复现 `0x4B37`。**请按 ndstool 实现，不要按 GBATEK 伪代码实现。**

**Icon/Title（banner）块**，相对 `0x068` 指向的地址：

| 偏移量 | 长度 | 字段 | 可靠性 |
|---|---|---|---|
| `0x0000` | 2 | Version（`0001h`/`0002h`/`0003h`/`0103h`） | — |
| `0x0002` / `0x0004` / `0x0006` / `0x0008` | 2 ×4 | 各版本的 CRC16 | 有效性 |
| `0x0020` | `0x200` | 图标位图 32×32 4bpp | — |
| `0x0240` | `0x100` | **Title 0 — 日文**（128 字符 UTF-16） | 高 |
| `0x0340` | `0x100` | **Title 1 — 英文** | **显示名首选** |
| `0x0440`/`0x0540`/`0x0640`/`0x0740` | `0x100` ×4 | 法/德/意/西 | 高 |
| **`0x0840`** | `0x100` | **中文（v2+）** | 高 |
| `0x0940` | `0x100` | 韩文（v3+） | 高 |

> **【推断】** banner 的中文标题槽（`0x0840`）对识别汉化 NDS ROM 有特殊价值：**汉化组常把中文名写进这里**。

**DSi 扩展**：

| 字段名 | 偏移量 | 长度 | 内容 |
|---|---|---|---|
| DSi Flags | `0x01C` | 1 | `03h`=Normal，`0Bh`=Sys，`0Fh`=Debug；bit0=有 TWL 专属区，bit1=Modcrypted |
| DSi Region flags | `0x1B0` | 4 | bit0=JPN,1=USA,2=EUR,3=AUS,**4=CHN**,5=KOR；`FFFFFFFFh`=Region Free |
| **Title ID（"Emagcode"）** | `0x230` | 4 | gamecode 倒序 |
| Title ID Filetype | `0x234` | 1 | `00h`=Cartridge，`04h`=DSiWare，`0Fh`=非可执行数据 |
| Title ID platform | `0x236` | 1 | **`03h` = DSi** |

**推荐 DS 主键**：`gamecode(0x00C,4)` + `makercode(0x010,2)` + `ROM version(0x01E,1)`，用 `0x15E` 的 header CRC16 与 `0x15C == CF56h` 验证。

---

### A.6 Nintendo 3DS

来源：[3dbrew NCSD](https://www.3dbrew.org/wiki/NCSD)、[NCCH](https://www.3dbrew.org/wiki/NCCH)、[CIA](https://www.3dbrew.org/wiki/CIA)、[Titles](https://www.3dbrew.org/wiki/Titles)、[Title metadata](https://www.3dbrew.org/wiki/Title_metadata)、[Ticket](https://www.3dbrew.org/wiki/Ticket)

#### A.6.1 NCSD 头（CCI / `.3ds` / `.cci`）

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| RSA-2048 签名 | `0x000` | `0x100` | SHA-256 签名 | 完整性 |
| **Magic `'NCSD'`** | `0x100` | 4 | — | **极高（格式嗅探）** |
| NCSD image size | `0x104` | 4 | 以 media unit 计（1 unit = `0x200` 字节） | 合理性检查 |
| **Media ID** | `0x108` | **8** | 即 Title ID | **极高** |
| Partitions FS type | `0x110` | 8 | 0=None,1=Normal,3=FIRM | — |
| 分区偏移/长度表 | `0x120` | `0x40` | (offset,length) ×8，media unit 计 | 定位 NCCH 必需 |
| Exheader SHA-256 | `0x160` | `0x20` | CCI 专用 | 完整性 |
| Partition Flags | `0x188` | 8 | `[5]` Media Type，`[6]` **MediaUnitSize = `0x200 << flags[6]`** | — |
| CARD2 writable addr | `0x200` | 4 | CARD1 恒为 `0xFFFFFFFF` | 卡类型提示 |
| Filled size of cartridge | `0x300` | 4 | — | **trim 检测** |
| **Title version** | `0x310` | 2 | — | **高** |
| Card revision | `0x312` | 2 | — | 中 |

#### A.6.2 NCCH 头（CXI / CFA）

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| RSA-2048 签名 | `0x000` | `0x100` | — | 完整性 |
| **Magic `'NCCH'`** | `0x100` | 4 | — | **极高** |
| Content size | `0x104` | 4 | media unit | 合理性 |
| **Partition ID** | `0x108` | **8** | = Title ID / Media ID | **极高** |
| **Maker code** | `0x110` | 2 | — | 高 |
| Version | `0x112` | 2 | 也决定 AES-CTR 派生方案 | 高 |
| **Program ID** | `0x118` | 8 | 通常 == Partition ID | 高 |
| Logo Region SHA-256 | `0x130` | `0x20` | SDK 5+ | — |
| **Product code** | `0x150` | **`0x10`** | NCCH 序列码 | **极高（人可读 ID）** |
| Exheader SHA-256 | `0x160` | `0x20` | — | 完整性 |
| Flags (`ncchflag[]`) | `0x188` | 8 | 加密/类型 | — |

**Product code 约定**（3dbrew 原文）：
> "Retail CFAs use the default NCCH product code **`CTR-P-CTAP`**, while retail title/gamecard CXIs use NCCH product code **`CTR-X-XXXX`**."

> ⭐ **加密关键点**：3dbrew 列出的加密区域是「the extended header, the ExeFS, and the RomFS」—— **NCCH 头本身不在其中**。因此 **Title ID、Maker Code、Product Code 可以在没有密钥的情况下从加密的 `.3ds`/`.cci` 中直接读出**。（前半为【事实】，结论为【推断】）

#### A.6.3 CIA —— Title ID 的取得方式

**CIA 头本身不含 Title ID**，必须从 Ticket 或 TMD 中读取。

| CIA 头字段 | 偏移量 | 长度 |
|---|---|---|
| Archive Header Size | `0x00` | 4（通常 `0x2020`） |
| Type / Version | `0x04` / `0x06` | 2 / 2 |
| Certificate chain size | `0x08` | 4 |
| Ticket size | `0x0C` | 4 |
| TMD size | `0x10` | 4 |
| Meta size | `0x14` | 4 |
| Content size | `0x18` | 8 |
| Content Index | `0x20` | `0x2000` |

区段顺序：**cert chain → Ticket → TMD → Content → Meta**，各自 64 字节对齐。

| 容器 | 签名块内偏移 | 长度 | 字段 |
|---|---|---|---|
| **TMD** | `+0x4C` | 8 | **Title ID** |
| TMD | `+0x54` | 4 | Title Type |
| **Ticket** | `+0x9C` | 8 | **TitleID** |
| Ticket | `+0x7F` | `0x10` | TitleKey（加密） |

签名块长度由起始的 u32 签名类型决定。零售常用 `0x010004`（RSA_2048 SHA256）：`4 + 0x100 + 0x3C` = **`0x140`**。

> **【推断/推导】** 零售场景下：Ticket TitleID 在 ticket 起始 + `0x1DC`；TMD TitleID 在 TMD 起始 + `0x18C`。可用 3dbrew 自身的陈述交叉验证 ——「The encrypted Title Key of a CIA can be found at offset `0x1BF` in a CIA's Ticket」，正好 = `0x140 + 0x7F`。**实现时应读 u32 签名类型再查表，不要硬编码 `0x140`。**

#### A.6.4 Title ID 结构

```
TitleID: 0xCCCCABCDLLLLLLRR
```

| 部分 | 位数 | 含义 |
|---|---|---|
| `CCCC` | 16 | 平台：`5`=WiiU，**`4`=3DS**，`3`=DSi，`1`=Wii |
| `ABCD` | 16 | Content Category 位掩码：Normal `0x0` · Demo `0x2` · AddOnContents `0x4` · Patch `0x6` · System `0x10` · **TWL `0x8000`** |
| `LLLLLL` | 24 | Unique ID（**Application 区间 `0x300–0xF7FFF`**） |
| `RR` | 8 | Title ID Variation |

> **区域不在 Title ID 中**（3dbrew 明确说明）。区域锁信息在 NCCH 的 `icon` 中。**【推断】** 区域差异实际体现在 **Product Code 的末位字母**上。

**推荐 3DS 主键**：CCI 用 `Title ID (NCSD 0x108)`；CIA 用 `TMD 内 +0x4C 的 Title ID`；辅以 `NCCH Product Code (0x150)` + `Title version`。

---

### A.7 Nintendo 64

来源：[n64brew Wiki – ROM Header](https://n64brew.dev/wiki/ROM_Header)、[CIC-NUS](https://n64brew.dev/wiki/CIC-NUS)
**以下偏移均针对归一化后的 big-endian（`.z64`）镜像。**

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| （未定义） | `0x00` | 1 | 已知商业卡均为 `0x80`。n64brew：「It is sometimes **erroneously** believed to be part of the PI BSD DOM1 configuration flags … but it is actually not」 | 仅用于字节序嗅探 |
| PI BSD DOM1 config | `0x01` | 3 | `0x37`,`0x12`,`0x40` | 仅字节序嗅探 |
| Clock Rate | `0x04` | 4 | 掩码 `0xFFFFFFF0` | 低 |
| Boot Address | `0x08` | 4 | 最常见 `0x80000400` | 低 |
| **libultra version** | `0x0C` | 4 | ⚠ **不是 `0x10`**。「many games report the wrong version here」 | 低 |
| **Check Code** | `0x10` | **8** | 「64-bit check code calculated on **1 Mbyte of ROM contents starting from offset `0x1000`**」。IPL3 校验，不符则死机。n64brew：「the check code algorithm **is not a CRC**」 | **极高**（各数据库实际主键） |
| Reserved | `0x18` | 8 | — | 低 |
| **Game Title / Image Name** | `0x20` | **0x14 (20)** | ASCII 或 JIS X 0201，`0x20` 补白 | 高 |
| Reserved | `0x34` | 7 | — | — |
| **Game Code** | `0x3B` | **4** | 见下拆分 | **极高（人可读 ID）** |
| ├ Category Code | `0x3B` | 1 | `N`=Game Pak · `D`=64DD Disk · `C`/`E`=可扩展游戏的卡带/磁盘部分 · `Z`=Aleck64 | 媒介类型 |
| ├ Unique Code | `0x3C` | 2 | 两个字母数字 | **极高** |
| └ Destination Code | `0x3E` | 1 | 区域，见下 | 高 |
| **ROM Version** | `0x3F` | 1 | `0` = 初版 | **高** |
| IPL3 | `0x40` | `0xFC0` | 引导代码，与卡上 CIC 配对 | 见 A.7.3 |

**Destination Code 表**：`A`=All · `B`=Brazil · `C`=China · `D`=Germany · `E`=North America · `F`=France · `G`=Gateway 64 (NTSC) · `H`=Netherlands · `I`=Italy · `J`=Japan · `K`=Korea · `L`=Gateway 64 (PAL) · `N`=Canada · `P`=Europe · `S`=Spain · `U`=Australia · `W`=Scandinavia · `X`/`Y`/`Z`=Europe。`0x00` 是 homebrew 的「region free」惯例。

#### A.7.2 字节序判定

n64brew 明确警告：
> "many emulators expect to find the 4 'standard' values here and actually use them as a 'fixed ID' … **If you are an emulator author, please make sure that your emulator can load a ROM with arbitrary values in these first 4 bytes.**"

实际 magic 来自 [mupen64plus-core `src/main/rom.c`](https://github.com/mupen64plus/mupen64plus-core/blob/master/src/main/rom.c)：

| 扩展名 | 首 4 字节 | 字节序 | 归一化 | 附加约束 |
|---|---|---|---|---|
| `.z64` | `80 37 12 40` | **Big-endian（原生）** | 无 | — |
| `.v64` | `37 80 40 12` | 半字（16-bit）交换 | `swap16` | `size % 2 == 0` |
| `.n64` / `.u64` | `40 12 37 80` | 字（32-bit）交换 / little-endian | `swap32` | `size % 4 == 0` |

> mupen64plus 源码注释：「The data extraction routines and **MD5 hashing function may only act on the .z64 big-endian format**」—— **所有哈希与头解析都必须在归一化之后进行。**

#### A.7.3 CIC / bootcode 判定 —— 已文档化且有实现

n64brew（CIC-NUS 页）：
> "each variant of the CIC comes in pair with a different boot software (called IPL3) … which is part of the secure boot, and is embedded in the cartridge itself (**in a special area of the ROM: offset `0x40` - `0x1000`**)"

**无法从头字段读出 CIC，只能对 IPL3 指纹化。** mupen64plus（`src/device/pif/cic.c`）把 IPL3 区按 `0xFC0/4 = 1008` 个 u32 累加到 64 位：

| Sum (u64) | CIC | Seed |
|---|---|---|
| `0x000000A5F80BF620` | 5101 | `0xAC` |
| `0x000000D0027FDF31` | X101 (6101/7102) | `0x3F` |
| `0x000000CFFB631223` | X101 | `0x3F` |
| `0x000000D057C85244` | **X102 (6102/7101) — 默认回退** | `0x3F` |
| `0x000000D6497E414B` | X103 | `0x78` |
| `0x0000011A49F60E96` | X105 | `0x91` |
| `0x000000D6D5BE5580` | X106 | `0x85` |
| `0x000001053BC19870` | 5167（64DD ROM 转换） | `0xDD` |
| `0x000000D2E53EF008` | 8303 | `0xDD` |
| `0x000000D2E53EF39F` | 8401（64DD IPL Dev J） | `0xDD` |
| `0x000000D2E53E5DDA` | 8501 | `0xDE` |

**推荐 N64 主键**：`0x10` 的 8 字节 check code；辅以 `Game Code (0x3B,4)` + `ROM Version (0x3F)`。**先归一化字节序**。

---

### A.8 Mega Drive / Genesis / Sega CD / 32X

来源：[Plutiedev – ROM header](https://plutiedev.com/rom-header)。字段与 [Genesis Plus GX `core/loadrom.c`](https://github.com/ekeeke/Genesis-Plus-GX/blob/master/core/loadrom.c) 的常量逐一吻合。

| 字段名 | 偏移量 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| **System type / console name** | `0x100` | 16 | 唯一被主机读取的字段；TMSS 机型无 `"SEGA"` 前缀不启动 | **极高（格式嗅探）** |
| Copyright & release date | `0x110` | 16 | `"(C)XXXX YYYY.ZZZ"`，如 `(C)SEGA 1992.NOV` | 高 |
| **Domestic (Japanese) title** | `0x120` | 48 | 大写、空格补白；**唯一允许非 ASCII（Shift-JIS）的字段** | 高 |
| **Overseas title** | `0x150` | 48 | 大写、空格补白 | 高 |
| **Serial number** | `0x180` | 14 | `"XX YYYYYYYY-ZZ"` | **极高** |
| ├ Software type | `0x180` | 2 | `GM`=Game · `AI`=Aid · `OS`=Boot ROM(TMSS) · **`BR`=Boot ROM(Sega CD)** | 类型判别 |
| └ Serial + revision | `0x182` | 12 | 序列号 + `-` + 2 位修订号（`00`=初版） | **极高** |
| **ROM checksum** | `0x18E` | 2（BE） | 见下 | **高** |
| Device support | `0x190` | 16 | 每设备一字母，空格补白 | 外设提示 |
| ROM address range | `0x1A0` | 8 | 两个 u32：起始（恒 0）与结束 | 合理性检查 |
| RAM address range | `0x1A8` | 8 | MD 上恒为 `0xFF0000`/`0xFFFFFF` | 合理性检查 |
| Extra memory (SRAM/EEPROM) | `0x1B0` | 12 | `"RA"` + 类型字节 + `$20`(SRAM)/`$40`(EEPROM) + 起止地址 | 存档类型 |
| Modem support | `0x1BC` | 12 | `"MOxxxxyy,zww"` | 低 |
| **Region support** | `0x1F0` | 3 | 两种互不兼容格式，见下 | 中（**需异常表**） |

**已知 `0x100` 字符串**：`"SEGA MEGA DRIVE"` / `"SEGA GENESIS"` = MD · **`"SEGA 32X"` = MD + 32X** · `"SEGA PICO"` = Pico · `"SEGA EVERDRIVE"` / `"SEGA SSF"` / `"SEGA MEGAWIFI"` = 闪存卡扩展 · `"SEGA TERA68K"` / `"SEGA TERA286"` = Tera Drive。

**Device-support 字母**：`J`=3键手柄 · `6`=6键手柄 · `0`=SMS手柄 · `A`=模拟摇杆 · `4`=多分插 · `G`=光枪 · `L`=Activator · `M`=鼠标 · `B`=轨迹球 · `T`=数位板 · `V`=paddle · `K`=键盘 · `R`=RS-232 · `P`=打印机 · **`C`=CD-ROM (Sega CD)** · `F`=软驱。

**Checksum 算法**：

| 属性 | 值 |
|---|---|
| 位置 | `0x18E`，2 字节，**big-endian** |
| 范围 | **`0x000200` 到 ROM 末尾** |
| 算法 | 大端 16-bit 字求和，16 位累加器自然回绕 |
| 是否强制 | **主机不校验**；部分游戏自检 |

> Genesis Plus GX 用「头部 checksum + 实算 checksum」这一对值来消歧错误头部的游戏，例如 `checksum == 0x0000 && realchecksum == 0x1f7f` ⇒ Radica Sensible Soccer Plus。

**⚠ Region 字段 `0x1F0` 有两种不兼容格式**：
- **旧式**：每区域一字母，顺序任意，可能夹空格 —— `J`=Japan · `U`=Americas · `E`=Europe。plutiedev：「**Always check all three bytes.**」
- **新式**：单个 ASCII 十六进制数字，4 位掩码 —— bit0 `+1` 日本 60Hz · bit1 `+2` 国内 50Hz · bit2 `+4` 海外 60Hz · bit3 `+8` 海外 50Hz
- **`"E"` 二义**：旧式 = 仅欧洲；新式 = `0b1110` = 除日本外全部

> Genesis Plus GX 对此维护了**硬编码异常表**（Alisia Dragon EU、Back to the Future III EU、Brian Lara Cricket、Williams Arcade's Greatest Hits、Wiz'n'Liz、Muhammad Ali Boxing 均强制 PAL）。**结论：区域字节单用不可靠，生产级实现都带异常列表。**

**Sega CD / Mega CD 磁盘识别**：

| 项 | 值 |
|---|---|
| Magic | **`"SEGADISCSYSTEM"`（14 字节）** |
| 位置 | 2048 字节扇区（cooked）镜像的 `0x000`；2352 字节扇区（raw）镜像的 **`0x010`**（12 字节同步 + 4 字节扇区头之后） |
| 头字段 | 卷 ID 之后，磁盘首数据扇区在 `0x100`–`0x1FF` 带有**与 MD 完全相同的头布局** |
| 区域 | 从**磁盘偏移 `0x20B` 的安全码**读取：`0x64`⇒Europe，`0xA1`⇒Japan NTSC，其他⇒USA（**不是** `0x1F0`） |

来源（两个独立实现互证）：[Genesis Plus GX `core/cd_hw/cdd.c`](https://github.com/ekeeke/Genesis-Plus-GX/blob/master/core/cd_hw/cdd.c)、[PicoDrive `pico/media.c`](https://github.com/irixxxx/picodrive/blob/master/pico/media.c)

**`.smd` copier 头（interleaved）**：PicoDrive 判据 `romsize >= 0x4200 && (romsize & 0x3fff) == 0x200`，且载荷还需**去交错**。

> **【推断】** 32X 的头部判据（`0x100 == "SEGA 32X"`）虽有文档，但未见有模拟器据此识别 —— PicoDrive 用扩展名列表 `{"gen","smd","md","32x"}`。作为识别信号只能算辅助。

---

### A.9 Master System / Game Gear

来源：[SMS Power! – ROM Header](https://www.smspower.org/Development/ROMHeader)

#### A.9.1 位置与「可选性」—— 这是本平台最重要的事实

smspower 原文：
> "All Game Gear and export Master System, **and some Japanese Master System**, games include a header… The BIOS in non-Japanese Master Systems requires a valid header…"
> "The header can be at offset **`$1ff0`, `$3ff0` or `$7ff0`** in the ROM, although only the last of these seems to be used in known software."
> "…there are **many instances where the values found in the header contain mistakes**."

**结论：SMS 头部是可选的。** 日版 SMS 大量游戏没有头，SG-1000/SC-3000 完全没有。Genesis Plus GX 与 PicoDrive 都会扫描全部三个偏移。

#### A.9.2 16 字节布局（以 `$7FF0` 为例）

| 字段名 | 绝对偏移 | 相对 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|---|
| **`"TMR SEGA"` magic** | `$7FF0` | +0x00 | 8 | 出口版 SMS 与 GG BIOS 要求存在 | **极高（存在时）/ 大量日版缺失** |
| Reserved | `$7FF8` | +0x08 | 2 | 常见 `$00 $00`、`$FF $FF` 或 `$20 $20` | 无 |
| **Checksum** | `$7FFA` | +0x0A | 2（**LE**） | 「Game Gear and Japanese releases tend not to have a correct checksum there」 | 出口版 SMS 好；GG/JP **不可靠** |
| **Product code 低 4 位** | `$7FFC` | +0x0C | 2 | **BCD** —— 数据 `26 70` ⇒ 产品码 `7026` | **高** |
| **Product code 高位** | `$7FFE` | +0x0E | 高半字节 | 十六进制 —— `26 70 2` ⇒ `27026` | 高 |
| **Version** | `$7FFE` | +0x0E | 低半字节 | `0` = 初版 | **高** |
| **Region code** | `$7FFF` | +0x0F | 高半字节 | 见下 | 高 |
| ROM size / checksum 范围 | `$7FFF` | +0x0F | 低半字节 | 见下 | 中 |

**Region code 表**：`$3` SMS Japan · `$4` SMS Export · `$5` GG Japan · `$6` GG Export · `$7` GG International。（Genesis Plus GX 只实现这五个，其余打印 `Unknown` —— 独立佐证该表完整。）

**ROM size 低半字节**：`$a` 8KB(未用) · `$b` 16KB(未用) · `$c` 32KB · `$d` 48KB(**未用、有 bug**) · `$e` 64KB(罕用) · `$f` 128KB · `$0` 256KB · `$1` 512KB(罕用) · `$2` 1MB(**未用、有 bug**)。

> smspower 原文：「It is also common for it to **indicate a ROM size smaller than the actual ROM size**, perhaps to speed up the boot process.」
> **⚠ 不要把这个 nibble 当作真实 ROM 大小** —— 它是「校验和范围选择器」，被有意写小是常态。

**Checksum 算法**（来源：[Maxim's sega8bitheaderreader README](https://github.com/maxim-zhao/sega8bitheaderreader)）：字节求和进 16 位累加器，溢出回绕；跳过头部自身（范围止于 `$7FF0`）；小端存于 `$7FFA`；**出口版 SMS BIOS 校验，GG BIOS 不校验**。

| ROM size nibble | 范围 1 | 范围 2 |
|---|---|---|
| `$c` (32KB) | `$0000–$7FEF` | — |
| `$e` (64KB) | `$0000–$7FEF` | `$8000–$FFFF` |
| `$f` (128KB) | `$0000–$7FEF` | `$8000–$1FFFF` |
| `$0` (256KB) | `$0000–$7FEF` | `$8000–$3FFFF` |

**参考实现如何识别 SMS/GG —— 不靠头部**：MEKA（smspower 站长所写）计算 **CRC32 + 自定义 "MekaCRC"** 并查内置数据库；且先把大小裁到 8KB 的整数倍再算。
来源：[MEKA `meka/srcs/checksum.cpp`](https://github.com/ocornut/meka/blob/master/meka/srcs/checksum.cpp)

---

### A.10 PC Engine / TurboGrafx-16

#### A.10.1 有可用的内部头吗？—— **没有**

| 结论 | 证据 |
|---|---|
| **HuCard ROM 无标准头** | [PCEdev Wiki（pce.nesdev.org）](https://pce.nesdev.org/) 全站**没有** ROM header / cartridge header / `.pce` 格式页面；HuCard 部分只覆盖 mapper 与扩展硬件 |
| 模拟器实际做法 | Mednafen/Beetle-PCE 用 **ROM 的 CRC32** 匹配硬编码表（如 SuperGrafx 判定表：Darius Plus `0xbebfe042`、Aldynes `0x4c2126b0`、1941 `0x8c4588e2`、Madouou Granzort `0x1f041166`、Daimakaimura `0xb486a8ed`、Battle Ace `0x3b13af61`） |
| 唯一存在的「magic」 | 是**逐游戏/逐 mapper 的签名**而非头：`memcmp(buf + 0x1FD0, "MCGENJIN", 8)`、`memcmp(HuCROM + 0x1F26, "POPULOUS", 8)` |

来源：[Beetle-PCE `mednafen/pce/huc.cpp`](https://github.com/libretro/beetle-pce-libretro/blob/master/mednafen/pce/huc.cpp)、[`mednafen/pce/pce.cpp`](https://github.com/libretro/beetle-pce-libretro/blob/master/mednafen/pce/pce.cpp)

#### A.10.2 512 字节 copier 头 —— 判据不是 `% 8192 == 512`

两个独立模拟器使用**同一个、且与常见说法不同**的判据：

| 模拟器 | 代码 |
|---|---|
| Mednafen / Beetle-PCE (`HuC_Load`) | `if(len & 512) { len &= ~512; data += 512; size -= 512; }` |
| [Geargrafx `src/media.cpp`](https://github.com/drhelius/Geargrafx/blob/main/src/media.cpp) | `if(size & 512) { size &= ~512; buffer += 512; }` |

| 项 | 结论 |
|---|---|
| 判据 | **`(filesize & 0x200) != 0`**（bit 9 置位） |
| 与 `% 8192 == 512` 的关系 | 底层 ROM 是 8KB 整数倍时等价；`& 512` 严格更宽松。**建议采用 `size & 512`**，这是实际发行版模拟器的做法 |
| 剥离后 | 两者都会把 ROM 补齐到 8KB 边界再映射。**求哈希时应使用剥离后、未补齐的数据** |

#### A.10.3 CD-ROM² / Super CD

**【推断】** PC Engine CD 无 ROM 文件可识别，只能靠**磁盘 TOC + 数据轨哈希**。与 Sega CD 不同，本次调研在 PCEdev wiki 上**未找到**任何等价于 `"SEGADISCSYSTEM"` 的卷头 magic 的一手文档。另外「需要哪张 System Card BIOS」也是 CD 标题身份的一部分，且**无法从磁盘头推导** —— Geargrafx 用 BIOS 镜像的 CRC32 查库。

---

### A.11 PlayStation 1

#### A.11.1 SYSTEM.CNF

来源：[psx-spx（nocash PSX 规范）](https://psx-spx.consoledev.net/cdromfileformats/)

规范原文示例：
```
  BOOT = cdrom:\abcd_123.45;1 arg ;boot exe (drive:\path\name.ext;version)
  TCB = 4                         ;HEX (=4 decimal)   ;max number of threads
  EVENT = 10                      ;HEX (=16 decimal)  ;max number of events
  STACK = 801FFF00                ;HEX (=memtop-256)
```

| 字段名 | 位置 | 长度 | 内容 | 可靠性 |
|---|---|---|---|---|
| `BOOT` | `SYSTEM.CNF`（ISO 根目录）中的 ASCII 行 | 变长 | `cdrom:` + `\` + `ABCD_123.45` + `;1` + 可选参数串 | **极高** |
| `TCB` / `EVENT` / `STACK` | 同上 | 变长 | 十六进制值 | 无 |
| 行终止符 | — | 2 | `0Dh,0Ah` | — |
| 缺省引导文件 | — | — | **`SYSTEM.CNF` 不存在时为 `PSX.EXE`** | — |

**序列号编码规则**（psx-spx 原文）：
> "the filename/extension is taken from the game code (the `"ABCD-12345"` text that is printed on the CD cover), but, with **the minus replaced by an underscore**, and due to the 8-letter filename limit, **the last two characters are stored in the extension region**."
> "Wild Arms does unconventionally have the file in a separate folder, `"EXE\SCUS_946.06"`."

| 形式 | 示例 | 出现位置 |
|---|---|---|
| 盘内文件名 | `SLUS_007.55` | ISO 根目录 |
| 印刷 / 数据库形式 | `SLUS-00755` | 盘面、Redump `[T:ISN]` 字段 |
| BOOT 值 | `cdrom:\SLUS_007.55;1` | `SYSTEM.CNF` |

Redump 归一化为连字符形式（[Sony PlayStation Guide](https://wiki.redump.info/index.php?title=Sony_PlayStation_Guide) 原文）：
> "Internal Serials for all Sony systems are a standardized format in redump: `[T:ISN] XXXX-#####` (example: `[T:ISN] SLUS-01234`)"

> ⚠ **注意**：`wiki.redump.org` 已失效，官方 wiki 迁移至 **`wiki.redump.info`**。

#### A.11.2 DuckStation 的序列号抽取逻辑（源码级，高价值）

来源：[DuckStation `src/core/system.cpp`](https://github.com/stenzek/duckstation)

**Step A —— `GetExecutableNameForImage()`**：
1. 读 `SYSTEM.CNF`，失败则回退 `"PSX.EXE"`
2. 自定义 key/value 解析：`\r`/`\n` 结束一条记录；空格与 `0x09`–`0x0D` **全部跳过**；首个 `=` 切入 value 模式
3. **大小写不敏感**地找 key == `"boot"`
4. 路径清理（识别用的模式）：要求前缀 `"cdrom:"`，去掉这 6 字节，再去掉所有前导 `/` 与 `\`
5. 去版本：`pos = code.rfind(';'); if found → code.erase(pos);`

**Step B —— 归一化（源码原文）**：
```cpp
// SCES_123.45 -> SCES-12345
for (std::string::size_type pos = 0; pos < id.size();)
{
  if (id[pos] == '.')      { id.erase(pos, 1); continue; }
  if (id[pos] == '_')        id[pos] = '-';
  else                       id[pos] = static_cast<char>(std::toupper(id[pos]));
  pos++;
}
```

| 规则 | 效果 |
|---|---|
| 删除所有 `.` | `SCES_123.45` → `SCES_12345` |
| `_` → `-` | → `SCES-12345` |
| 其余字符大写 | `scus_946.06` → `SCUS-94606` |
| **不做格式校验** | 任何 BOOT 文件名都会变成「序列号」（与 PCSX2 不同，见 A.12） |

**Step C/D —— 无序列号时的哈希 ID**：
```cpp
std::string System::GetGameHashId(GameHash hash) { return fmt::format("HASH-{:X}", hash); }
```
哈希本体：
```cpp
XXH64_reset(state, 0x4242D00C);
XXH64_update(state, exe_name.data(), exe_name.size());
XXH64_update(state, exe_buffer.data(), exe_buffer.size());
XXH64_update(state, &iso_pvd, sizeof(IsoReader::ISOPrimaryVolumeDescriptor));  // 2048 字节
XXH64_update(state, &track_1_length, sizeof(track_1_length));
```
> 即 **XXH64(seed=0x4242D00C) 覆盖：引导 EXE 路径串 + EXE 全部内容 + 整个 2048 字节 ISO PVD + 轨道 1 的扇区数**。

**Step F —— 由序列号推区域**：

| 前缀（不分大小写） | 区域 |
|---|---|
| `sces` `sced` `sles` `sled` | PAL |
| `scps` `slps` `slpm` `sczs` `papx` | NTSC-J |
| `scus` `slus` | NTSC-U |
| 其他 | Other |

#### A.11.3 「Licensed by Sony Computer Entertainment」系统区字符串

psx-spx（*CDROM ISO Volume Descriptors → System Area*）原文：
```
  Sector 0..3   - Zerofilled (Mode2/Form1, 4x800h bytes, plus ECC/EDC)
  Sector 4      - Licence String
  Sector 5..11  - Playstation Logo (3278h bytes)
  Sector 12..15 - Zerofilled
```

**扇区 4** 内（偏移相对该扇区 2048 字节用户数据）：

| 偏移 | 长度 | 内容 |
|---|---|---|
| `0x000` | 32 | `"          Licensed  by          "` |
| `0x020` | 32+6 | EU：`"Sony Computer Entertainment Euro"` + `" pe   "` |
| `0x020` | 32+1 | JP：`"Sony Computer Entertainment Inc."` + `0Ah` |
| `0x020` | 32+6 | US：`"Sony Computer Entertainment Amer"` + `"  ica "` |

DuckStation 的 `GetRegionFromSystemArea()` 实现了完全一致的逻辑（`cdi->Seek(1, 4)`），且若轨道 1 是音频轨则提前放弃。

> **另一个弱信号**：PS-EXE 头自身在 `0x04C` 有 ASCII 标记 `"Sony Computer Entertainment Inc. for Japan area"` 等，但 psx-spx 原文说「**the BIOS doesn't verify this string, and boots fine without it**」。

#### A.11.4 Redump 的 PS1 识别

| 方面 | 结论 |
|---|---|
| 序列号 | `[T:ISN] XXXX-#####`，连字符形式 |
| 序列号前缀分配表 | [Sony PlayStation Serials](https://wiki.redump.info/index.php?title=Sony_PlayStation_Serials)，完整表在 https://sony.redump.info/ |
| **DAT 的识别单位** | Logiqx XML：**每条轨道的 `.bin` 各一个 `<rom>`，外加 `.cue` 一个 `<rom>`**，各带 `size` / `crc` / `md5` / `sha1` |
| EDC/ECC | dump log 含 `.img_EccEdc.txt`；`"[NO ERROR] User data vs. ecc/edc match all"` 表示 0 错误 |
| NoEDC 盘 | 「NoEDC discs with audio tracks **always have at least a single error** in the last data sector before the audio track」—— 属预期，不得移除 |
| LibCrypt | 基于子通道的保护 |

真实 DAT 记录形态（结构与 PS1 相同）：
```xml
<rom name="09 Chairs (Japan).cue" size="1289" crc="bf7e28be" md5="db7c…" sha1="03f3…"/>
<rom name="09 Chairs (Japan) (Track 01).bin" size="34285104" crc="8951f196" md5="0d9f…" sha1="c129…"/>
```

**DuckStation 消费的正是这些哈希**：`game_database.cpp` 的 `LoadTrackHashes()` 以**每条轨道的 MD5** 建 multimap；`cd_image_hasher.cpp` 按 **2352 字节原始扇区**计算，数据轨只取 index 1，音频轨取 index 0+1。

#### A.11.5 容器格式

| 容器 | Magic / 结构 | 识别路径 |
|---|---|---|
| `.cue` + `.bin` | 纯文本 cue + 每轨一个 `.bin` | 把轨道 1 当 ISO9660 挂载 → `SYSTEM.CNF` → BOOT；每个 `.bin` 各自求哈希对 DAT |
| **`.chd`（MAME CHD v5）** | 偏移 0 处 `char tag[8] = "MComprHD"` | libchdr 解成 CD 镜像，之后同 cue/bin |
| **`.pbp`** | 偏移 0 处 `u8 magic[4] = "\0PBP"` | 读内嵌 `PARAM.SFO` 的 `DISC_ID`/`TITLE`；或解 `DATA.PSAR` |

**CHD v5 头**（[libchdr `chd.h`](https://github.com/rtissera/libchdr)）：

| 偏移 | 类型 | 字段 |
|---|---|---|
| 0 | `char[8]` | `'MComprHD'` |
| 8 | u32 | header 长度 |
| 12 | u32 | version |
| 16 | u32×4 | `compressors[4]` |
| 32 | u64 | `logicalbytes` |
| 40 | u64 | `mapoffset` |
| 48 | u64 | `metaoffset` |
| 56 | u32 | `hunkbytes`（最大 512K） |
| 60 | u32 | `unitbytes` |
| **64** | u8[20] | **`rawsha1`** |
| **84** | u8[20] | **`sha1`（raw + meta）** |
| 104 | u8[20] | `parentsha1` |
| 124 | — | （V5 头长度） |

规则原文：*"If `parentsha1 != 0`, we have a parent… If `compressors[0] == 0`, we are uncompressed (including maps)."* **轨道布局在 CHD metadata tag 里，不在头里。**

**PBP 头**（DuckStation `cd_image_pbp.cpp`，`static_assert(sizeof(PBPHeader) == 0x28)`）：

| 偏移 | 字段 |
|---|---|
| `0x00` | `u8 magic[4]` = `"\0PBP"` |
| `0x04` | `u32 version` |
| `0x08` | `u32 param_sfo_offset`（通常 `0x28`） |
| `0x0C`–`0x1C` | icon0 / icon1 / pic0 / pic1 / snd0 偏移 |
| `0x20` | `u32 data_psp_offset` |
| `0x24` | `u32 data_psar_offset` |

`DATA.PSAR` 内（相对每张盘起点）：`+0x000` `"PSISOIMG0000"`（多盘时 PSAR 起点为 `"PSTITLEIMG000000"`）；`+0x400` 若为 `"\0PGD"` 表示已加密不支持；`+0x800` TOC（102 × 10 字节，BCD 时间码）；`+0xBFC` 压缩 ISO 偏移；`+0x4000` 块表。

---

### A.12 PlayStation 2

来源：[PCSX2 `pcsx2/CDVD/CDVD.cpp`](https://github.com/PCSX2/pcsx2)、`pcsx2/Elfheader.cpp`、`pcsx2/GameDatabase.cpp`

#### A.12.1 SYSTEM.CNF

| Key | 含义 | PCSX2 行为 |
|---|---|---|
| `BOOT2` | `cdrom0:\SLUS_202.02;1` | 设 ELF 路径，判为 `PS2Disc` |
| `BOOT` | PS1 路径 | 设 ELF 路径，判为 `PS1Disc` |
| `VER` | 软件版本 | 作为盘版本记录 |
| `VMODE` | NTSC/PAL | 仅记日志 |
| 两个 BOOT key 都没有 | — | `"Disc image is *not* a PlayStation or PS2 game"` |

路径解析（`cdvdUncheckedLoadDiscElf`）：
```cpp
size_t start_pos = (elfpath[5] == '0') ? 7 : 6;   // "cdrom0:" vs "cdrom:"
while (start_pos < elfpath.size() && (elfpath[start_pos]=='\\' || elfpath[start_pos]=='/')) start_pos++;
// 在【第一个】 ';' 处截断
```
源码注释：*"Some games use ;2 (MLB2k6), others put multiple versions in (Syphon Filter Omega Strain). **The PS2 BIOS appears to ignore the suffix entirely, so we'll do the same**"*。

#### A.12.2 序列号归一化（`ExecutablePathToSerial`）

| 步骤 | 规则 |
|---|---|
| 1 | 取最后一个 `\` 之后；无则取最后一个 `:` 之后；否则整串 |
| 2 | 从最后一个 `;` 处截断 |
| **3** | **格式校验**：必须匹配 `????_???.??*` 或 `????-???.??*`，**否则序列号被清空** |
| 4 | 删除所有 `.`；`_` → `-`；其余大写 |

> ⚠ **与 DuckStation 的关键差异**：**PCSX2 有第 3 步格式校验，DuckStation 没有。**

#### A.12.3 PCSX2 的 GameDB「CRC」到底算的是什么

`pcsx2/Elfheader.cpp` 原文：
```cpp
u32 ElfObject::GetCRC() const
{
	u32 CRC = 0;
	const u32* srcdata = reinterpret_cast<const u32*>(data.data());
	for (u32 i = static_cast<u32>(data.size()) / 4; i; --i, ++srcdata)
		CRC ^= *srcdata;
	return CRC;
}
```

| 属性 | 值 |
|---|---|
| **算法** | **32 位小端字的 XOR 折叠 —— 不是 CRC-32**，无多项式、无查表 |
| 覆盖数据 | **整个引导 ELF 文件**（ISO 内该文件的全部字节，含文件头与文件内填充） |
| 尾部处理 | 只处理 `size/4` 个字，**最多 3 个尾字节被忽略** |
| 是哪个 ELF | `BOOT2` 指定的（PS1 盘则是 `BOOT` 指定的） |
| 不覆盖 | ISO 元数据、PVD、其他文件、盘大小 |

**GameDB 查找**：以**小写序列号**为键（源码注释：*"Serials and CRCs must be inserted as lower-case, as that is how they are retrieved"*）；每条目的 `patches` 以**十六进制解析的 CRC** 为键，字面量 `default` 映射到 CRC `0`。

---

### A.13 PSP

来源：[psdevwiki PSP PARAM.SFO](https://www.psdevwiki.com/psp/PARAM.SFO)（活站返回 403，经 Wayback 读取）、[pspsdk `tools/mksfo.c`](https://github.com/pspdev/pspsdk/blob/master/tools/mksfo.c)、PPSSPP `Core/ELF/ParamSFO.cpp`

#### A.13.1 PARAM.SFO 二进制布局

**Header —— 偏移 `0x00`，长度 `0x14`（20 字节）**
psdevwiki 原文：*"magic and version are **Big Endian**. key_table_start, data_table_start, and tables_entries are **Little Endian**."*（PPSSPP 把五个字段都声明为 `u32_le`）

| 偏移 | 长度 | 字段 | 值 |
|---|---|---|---|
| `0x00` | 4 | `magic` | 字节 `00 50 53 46` = `"\0PSF"`；PPSSPP 按 LE u32 测 `0x46535000` |
| `0x04` | 4 | `version` | `01 01 00 00` = 1.01 |
| `0x08` | 4 | `key_table_start` | **绝对**偏移 |
| `0x0C` | 4 | `data_table_start` | **绝对**偏移 |
| `0x10` | 4 | `tables_entries` | 三张表的条目数 |

**index_table —— 偏移 `0x14`，`tables_entries` × `0x10` 字节，little endian**

| 相对偏移 | 长度 | 字段 | 含义 |
|---|---|---|---|
| `+0x00` | 2 | `key_offset` | 相对 `key_table_start` |
| `+0x02` | 2 | `data_fmt` | 数据类型 |
| `+0x04` | 4 | `data_len` | **已用**字节（utf8 含结尾 NUL） |
| `+0x08` | 4 | `data_max_len` | **预留**字节 |
| `+0x0C` | 4 | `data_offset` | 相对 `data_table_start` |

**data_fmt 取值**：

| 盘上字节 | u16 LE | 类型 |
|---|---|---|
| `04 00` | `0x0004` | utf8-S（Special Mode，**不以 NUL 结尾**） |
| `04 02` | `0x0204` | utf8（NUL 结尾，NUL 计入 `data_len`） |
| `04 04` | `0x0404` | int32（`len` 与 `max_len` 恒为 4） |

**key_table**：NUL 结尾的大写 UTF-8 键，**按字母序 A→Z 存储**，且*"This alphabetically order defines the order of the associated entries in the other two tables."*

psdevwiki 的最小完整示例（原文 hex）：
```
Offset(h) 00 01 02 03 04 05 06 07 08 09 0A 0B 0C 0D 0E 0F
 0x00     00 50 53 46 01 01 00 00 24 00 00 00 30 00 00 00   .PSF....$...0...
 0x10     01 00 00 00 00 00 04 02 0A 00 00 00 0F 00 00 00   ................
 0x20     00 00 00 00 54 49 54 4C 45 5F 49 44 00 00 00 00   ....TITLE_ID....
 0x30     42 4C 55 53 31 32 33 34 35 00 00 00 00 00 00 00   BLUS12345.......
```

#### A.13.2 PSP 的键集与字段尺寸（来自官方 SDK）

从 pspsdk `mksfo.c` 内嵌的规范 SFO 解码得出：`key_table_start = 0x94`，`data_table_start = 0xE8`，`tables_entries = 8`，`#define TITLE_POS 0x118`、`#define TITLE_SIZE 0x7F`。

| Key | `data_fmt` | `data_max_len` | 示例值 |
|---|---|---|---|
| `BOOTABLE` | int32 | 4 | `1` |
| `CATEGORY` | utf8 | 4 | `"MG"`（PSP GAME 为 `UG`） |
| **`DISC_ID`** | utf8 | **16 (0x10)** | `"UCJS10041"` |
| `DISC_VERSION` | utf8 | 8 | `"1.00"` |
| `PARENTAL_LEVEL` | int32 | 4 | `1` |
| `PSP_SYSTEM_VER` | utf8 | 8 | `"1.00"` |
| `REGION` | int32 | 4 | `0x00008000` |
| **`TITLE`** | utf8 | **128 (0x80)** | 写在 `0x118`，最多 `0x7F` 字符 |

> `DISC_ID` 是无分隔符的 9 字符序列号（如 `ULUS10041`）；Redump 归一化为 `[T:ISN] ULUS-10041`。
> **Redump 用 `DISC_VERSION` 作版本字段**（[PSP Dumping Guide](https://wiki.redump.info/index.php?title=Sony_PlayStation_Portable_Dumping_Guide) 原文）：
> *"For new disc submissions, you need to use the '**DISC_VERSION**' value for the version number, **NOT** 'PSP_SYSTEM_VER', 'APP_VER', or 'SFO version'!"*

#### A.13.3 PARAM.SFO 的位置

| 容器 | 路径 / 偏移 |
|---|---|
| UMD ISO | **`/PSP_GAME/PARAM.SFO`**（PPSSPP：`std::string sfoPath("disc0:/PSP_GAME/PARAM.SFO");`） |
| UMD ISO（附加） | 根目录 `/UMD_DATA.BIN` |
| UMD ISO（免解析） | **PVD 扇区 16，ISO9660「Application Used」区，起始 `0x373`**，形如 `ULUS-10339|19…` |
| 解包目录 | 有 `PSP_GAME/` = 游戏目录；只有裸 `PARAM.SFO` = 存档目录 |
| CSO / CISO | 不单独存储，需先解压 |
| PBP / EBOOT | 子文件索引 `PBP_PARAM_SFO`，即 PBP 头 `+0x08` 指向的块 |

**CSO/CISO 头**（PPSSPP `BlockDevices.cpp`）：`+0x00` magic `'C','I','S','O'`；`+0x04` header_size(0x18)；`+0x08` total_bytes(u64)；`+0x10` block_size；`+0x14` ver；`+0x15` align；`+0x18` 索引数组。

**Redump PSP 的 DAT 单位**：单个 `.iso`，提交模板要求 ISO 的 MD5 / SHA1 / CRC32 与精确字节大小。

---

### A.14 Sega Saturn

> ⭐ **最佳来源是 Sega 自己的 IP 头汇编源码** `SYS_ID.SRC`，见 [Antime 的 Sega 文档存档](https://antime.kapsi.fi/sega/docs.html) → `IPGNUSRC.zip` → `IPGNUSRC/SYS/SYS_ID.SRC`。
> 文件头注释：*"sys_id.src -- System ID for 3rd Party (Ver.1994-11-11) … Please refer to the Saturn Boot ROM System Users Manual Ver 1.0 ST-220"*。
> 交叉验证：[Yabause `src/cs2.c` 的 `Cs2GetIP()`](https://github.com/Yabause/yabause/blob/master/yabause/src/cs2.c)（读 FAD 150 = LBA 0，要求 `memcmp(buf, "SEGA SEGASATURN", 15) == 0`）。

#### A.14.1 IP.BIN / 磁盘头（数据轨 LBA 0）

| 偏移 | 长度 | 字段名 | 规则（Sega 原注释） | 可靠性 |
|---|---|---|---|---|
| `0x00` | 16 | Hardware ID | 固定 `"SEGA SEGASATURN "` —— *"This string should never be changed."* | **极高（magic）** |
| `0x10` | 16 | Maker / Manufacturer ID | Sega 自有：`"SEGA ENTERPRISES"`；第三方：`"SEGA TP T-001   "` 形式，左对齐空格补白 | 中 |
| **`0x20`** | **10** | **Product number** | 字母数字，右侧空格补白。如 `"T-00101H  "`、`"GS-9109   "` | **极高** |
| `0x2A` | 6 | Version | 必须为 `"V"` + 1 位数字 + `"."` + 3 位数字，如 `V1.003` | 高 |
| `0x30` | 8 | Release date | `YYYYMMDD`，纯数字，母盘制作日期 | 高 |
| `0x38` | 8 | Device information | `"CD-1/1  "`、`"CD-2/3  "` —— 第 N 张共 M 张 | 中 |
| `0x40` | 16 | **Area symbols** | 前 10 位有效 + 6 位强制空格；字母左对齐 | 高 |
| `0x50` | 16 | Peripherals | 字母左对齐，其余空格 | 中 |
| `0x60` | 112 | **Game title** | 字母数字 + 空格；多标题条目允许 `/ , - :`；空格补白 | 高 |
| `0xD0` | 16 | reserved | *"Do not change"* —— 四个零长字 | — |
| `0xE0` | 4 | IP size (bytes) | 如 `0x00001800` | — |
| `0xE8` | 4 | Stack-M | 主 SH2 SP；0 表示默认 `6001000h–6001FFFh` | — |
| `0xEC` | 4 | Stack-S | 从 SH2 SP；0 表示默认 `6000D00h–6000FFFh` | — |
| `0xF0` | 4 | 1st Read Address | 加载地址，Sega 默认 `0x06010000` | — |
| `0xF4` | 4 | 1st Read size | *"Normally ignored when CD loading is performed"* | — |

#### A.14.2 Area symbols（`0x40`）—— Sega 源码原文

| 符号 | 地区 |
|---|---|
| `J` | Japan |
| `T` | Asia NTSC（Taiwan, The Philippines） |
| `U` | North America（U.S. and Canada） |
| `B` | South America NTSC（Brazil） |
| `K` | Korea |
| `A` | East Asia PAL（China and Middle East） |
| `E` | Europe PAL |
| `L` | South America PAL |

全区字符串：`"JTUBKAEL        "`。Sega 原文：*"It is imperative that these codes are set correctly… the hardware area symbol on the Saturn, and the soft area symbol encoded into your game on CD must allign, or else your product will not work."*

#### A.14.3 Peripherals（`0x50`）

| 字符 | 外设 |
|---|---|
| `J` | Control pad |
| `A` | Analogue controller |
| `M` | Mouse |
| `K` | Keyboard |
| `S` | Steering controller |
| `T` | Multitap |

> Sega 1996 年的原注：光枪（Virtua Cop gun / STUNNER）当时**尚未分配代码**，*"The code will probably be 'G'."* —— 因此实际盘上的 `G` 是 ST-220 之后才出现的、未被该文档收录的扩展。

#### A.14.4 Redump 对 Saturn 头的使用

[Sega Saturn Guide](https://wiki.redump.info/index.php?title=Sega_Saturn_Guide) 原文：
> "**Build date** should be the format YYYY-MM-DD and is extrapolated directly from Row 0030 (Line 4) of the Header. For example, if Line 4 of the Header says `"19960307CD-1/1  "`, the Build date is 1996-03-07"
> "**Version** … from Row 0020 (Line 3). For example, if Line 3 says `"T-6802G   V1.003"`, the Version is 1.003. The first portion of Line 3 is the **Internal Serial** and may be placed in the comments with the `[T:ISN]` tag"

> 这段话精确印证了打包方式：`0x20` 的 product number（10 字节）紧接 `0x2A` 的 version（6 字节）；`0x30` 的日期（8 字节）紧接 `0x38` 的 device info（8 字节）。

---

### A.15 Sega Dreamcast

来源：[Marcus Comstedt, *Dreamcast Programming — IP0000.BIN*](http://mc.pp.se/dc/ip0000.bin.html)（该页本身即参考实现级文档）

Comstedt 原文：
> "The IP0000.BIN file is present on every Dreamcast disc… **The structure described below is repeated in the 16 first sectors** of the first Mode-1 track on the disc… **All the fields in the IP0000.BIN are plain ASCII, padded with spaces to their full length.**"

16 扇区 × 2048 = **`0x8000` 字节**；识别用的头是前 `0x100` 字节。

| 偏移 | 长度 | 字段名 | 内容 | 可靠性 |
|---|---|---|---|---|
| `0x000` | 16 | Hardware ID | 恒为 `"SEGA SEGAKATANA "` | **极高（magic）** |
| `0x010` | 16 | Maker ID | 恒为 `"SEGA ENTERPRISES"` | 低 |
| `0x020` | 16 | Device Information | `"8B40 GD-ROM2/3  "` —— 见下 | 中 |
| `0x030` | 8 | Area Symbols | 8 字符，空格或区域字母 | 高 |
| `0x038` | 8 | Peripherals | 7 位十六进制位域 | 中 |
| **`0x040`** | **10** | **Product number** | `"HDR-nnnn"`、`"T-9503N"` 等 | **极高** |
| `0x04A` | 6 | Product version | | 高 |
| `0x050` | 16 | Release date | `YYYYMMDD`（+ 补白） | 高 |
| `0x060` | 16 | Boot filename | 通常 `"1ST_READ.BIN"` | 低 |
| `0x070` | 16 | Software maker | 制盘公司 | 中 |
| `0x080` | 128 | **Game title** | 软件名 | 高 |

#### A.15.1 Device Information 里的 CRC —— Comstedt 原文

> "The Device Information field begins with a four digit hexadecimal number, which is a **CRC on the Product number and Product version fields (16 bytes)**. Then comes the string `"·GD-ROM"`, and finally an indication of how many discs this software uses…"

即：CRC 覆盖**字节 `0x40`–`0x4F`（16 字节）**，以 4 位大写十六进制写在 `0x20`。算法原文：
```c
int calcCRC(const unsigned char *buf, int size)
{
  int i, c, n = 0xffff;
  for (i = 0; i < size; i++)
  {
    n ^= (buf[i]<<8);
    for (c = 0; c < 8; c++)
      if (n & 0x8000) n = (n << 1) ^ 4129;
      else            n = (n << 1);
  }
  return n & 0xffff;
}
```
> 即 **CRC-16/CCITT**（多项式 `0x1021` = 4129），MSB 优先，**初值 `0xFFFF`**，无最终异或、无反射。
> Comstedt：*"exactly the same CRC algorithm as for the VMS file headers, except that the initial remainder is FFFF instead of 0."*

#### A.15.2 Area symbols（`0x030`）

> "eight characters, which are either space or a specific letter… So far, only the first three are assigned. These are **Japan** (and the rest of East Asia), **USA + Canada**, and **Europe**… **If the character for a particular region is a space, the disc will not be playable in that region**… The region characters for the first three regions are **J**, **U**, and **E**."

仅欧洲 ⇒ `"  E     "`。

> ⚠ **该页自身的一处笔误【推断】**：介绍位域的行文写作 *"The Device Information field is a 28 bit long bitfield…"*，但这与同页的偏移表（`0x20` = Device Information、`0x38` = Peripherals）及前文对 Device Information 的描述矛盾。**位域无疑指的是 `0x038` 的 Peripherals 字段。**

#### A.15.3 Redump 的 Dreamcast 约定

[Sega Dreamcast Guide](https://wiki.redump.info/index.php?title=Sega_Dreamcast_Guide) 原文：
> "Serial field is for box serial. **Internal serial (found in header) is listed in comments as `[T:ISN]`**"
> "We should **not** add Rev A, Rev B, Rev C to Dreamcast discs **based on mastering code information only**… Example: Virtua Striker 2 Ver.2000.1 is not Rev C even though mastering code is HDR-0045-0226C"

MIL-CD 为多段（multisession），有明确的段范围公式。DAT 形态同 PS1：`.cue` + 每轨 `.bin`。

---

### A.16 GameCube / Wii

#### A.16.1 GameCube 磁盘头（`boot.bin`）

来源：[YAGCD chapter 13](https://www.gc-forever.com/yagcd/chap13.html)

磁盘布局：总容量 `1,459,978,240` 字节 = `712880` × 2048 扇区。

| 偏移 | 长度 | 字段名 | 可靠性 |
|---|---|---|---|
| `0x0000` | 4 | **Game Code** = 1 Console ID + 2 Gamecode + 1 Country Code | **极高** |
| `0x0004` | 2 | **Maker Code** | 高 |
| `0x0006` | 1 | Disk ID（多盘序号） | 中 |
| `0x0007` | 1 | Version | 高 |
| `0x0008` | 1 | Audio Streaming | 低 |
| `0x0009` | 1 | Stream Buffer Size | 低 |
| `0x000A` | `0x12` | unused（零） | — |
| **`0x001C`** | 4 | **DVD Magic Word `0xC2339F3D`** | **极高（magic）** |
| `0x0020` | `0x03E0` | Game Name | 高 |
| `0x0420` | 4 | 主可执行 DOL 的偏移 | — |
| `0x0424` / `0x0428` / `0x042C` | 4 ×3 | FST 偏移 / 大小 / 最大大小 | — |

`bi2.bin` 位于 `0x440`，其 `0x18` 处是 **Countrycode**（u32）。

#### A.16.2 Wii 磁盘头

来源：[WiiBrew – Wii Disc](https://wiibrew.org/wiki/Wii_Disc)。原文：*"The first 0x400 bytes are like the GameCube disc header format."*

| 偏移 | 长度 | 字段名 | 备注 |
|---|---|---|---|
| `0x000` | 1 | **Disc ID**（主机/类型字母） | |
| `0x001` | 2 | **Game code** | |
| `0x003` | 1 | **Region code** | 见下表 |
| `0x004` | 2 | **Maker code** | |
| `0x006` | 1 | Disc number | 多盘 |
| `0x007` | 1 | Disc version | |
| `0x008` | 1 | Audio streaming | 「No Wii game uses streaming」 |
| **`0x018`** | 4 | **Wii Magicword `0x5D1C9EA3`** | 「Present on Wii discs, **zero on Gamecube discs**」 |
| **`0x01C`** | 4 | **GameCube Magicword `0xC2339F3D`** | 「Present on Gamecube discs, **zero on Wii discs**」 |
| `0x020` | 64 | Game title | 「though most docs claim it to be 0x400 **the Wii only reads 0x44**」 |
| `0x060` | 1 | disable hash verification | 零售机上会让所有读盘失败 |
| `0x061` | 1 | disable encryption / h3 loading | 同上 |

Wii「System Area」：头在 `0x00000`（1024 字节）、分区信息在 `0x40000`、区域设置在 `0x4E000`（32 字节）、magic `0xC3F81A8E` 在 `0x4FFFC`。

> **YAGCD 与 WiiBrew 的差异**：YAGCD 把 `0x00`–`0x03` 当作一个 4 字节 "Game Code"（console ID + 2 + country）；WiiBrew 拆成 Disc ID(`0x00`) / Game code(`0x01`–`0x02`) / Region code(`0x03`)。**描述的是同一批字节**，且都与 Redump 的 "Game ID = XYYZ" 一致。

[Dolphin `Source/Core/DiscIO/DiscUtils.h`](https://github.com/dolphin-emu/dolphin) 的常量印证了全貌：
```
MINI_DVD_SIZE = 1459978240;   // GameCube
SL_DVD_SIZE   = 4699979776;   // Wii 单层零售
DL_DVD_SIZE   = 8511160320;   // Wii 双层零售
GAMECUBE_DISC_MAGIC = 0xC2339F3D;  WII_DISC_MAGIC = 0x5D1C9EA3;
DISCHEADER_ADDRESS = 0; DISCHEADER_SIZE = 0x440;
BI2_ADDRESS = 0x440;    BI2_SIZE = 0x2000;
WII_REGION_DATA_ADDRESS = 0x4E000; WII_REGION_DATA_SIZE = 0x20;
```

#### A.16.3 区域字节（`0x03`）

| 字节 | Wii | GameCube |
|---|---|---|
| `D` | Germany | Germany |
| `E` | USA / NTSC-U | USA / NTSC-U |
| `F` | France | France |
| `H` | Netherlands | Netherlands |
| `I` | Italy | Italy |
| `J` | Japan / NTSC-J | Japan / NTSC-J |
| `K` | Korea | — |
| `M` | — | Sweden |
| `P` | Europe / PAL | Europe / PAL |
| `R` | Russia | — |
| `S` | Spain | Spain |
| `U` | Australia | Australia |
| `V` | Scandinavia | — |
| `W` | Hong Kong / Taiwan / Scandinavia | Korea |
| `X`/`Y` | 其他 | Europe（其他语言版） |

Dolphin（`Source/Core/DiscIO/Enums.cpp` 的 `CountryCodeToCountry`）解决了这些歧义：`'A'` = World；`'X'/'Y'/'Z'` = 「Additional language versions, store-exclusive versions, other special versions」→ NTSC-U 则 USA，否则 Europe；`'W'` = GameCube 上是 Korea、PAL Wii 上是 Europe、其他为 Taiwan；`'E'` 在 GameCube 上当 `revision >= 0x30` 时是 Korea。

**第一字节（`0x00`，主机/类型字母）**（Redump）：

| GameCube | Wii |
|---|---|
| `D` Demo · `E` Demo（D 溢出） · `G` Game · `P` Promo · `U` Game Boy Player | `D` Demo · `R` Game（早期） · `S` Game（后期） |

> Redump 备注：*"If unique, (nearly) guaranteed to be unique dump."*

#### A.16.4 Maker code（`0x04`–`0x05`）

Dolphin `GetCompanyFromID` 的表头：`{"01","Nintendo"}, {"02","Nintendo"}, {"08","Capcom"}, {"0A","Jaleco"}, {"13","Electronic Arts Japan"}, {"41","Ubisoft"}, {"51","Acclaim"}, {"52","Activision"}, {"54","Take-Two/Rockstar"}, …`
> **`01` = Nintendo 正确**（`02` 也是 Nintendo）。

Redump 的 GC/Wii「Internal Serial」= `XXXXYY` = 4 字符 game ID + 2 字符 publisher ID —— *"This is the 'Filename' output from Cleanrip or the 'Game ID' displayed in dolphin."*

#### A.16.5 容器格式与 Redump 的关系

| 容器 | Magic / 结构 | 识别路径 |
|---|---|---|
| `.iso` | 原始盘镜像，精确为 `1459978240`（GC）或 `4699979776`/`8511160320`（Wii） | 直接读偏移 0 的头 |
| `.wbfs` | 偏移 0 的 WBFS 头 magic；含 `hd_sector_shift`、`wbfs_sector_shift` 与块映射 | Dolphin 由块映射重建盘偏移后读头 |
| **`.wia`** | `char magic[4] = "WIA\x1"`（Dolphin：`WIA_MAGIC = 0x01414957`） | **`wia_disc_t.dhead[0x80]` 逐字保存了盘镜像的前 `0x80` 字节** —— 无需解压即可读出 ID |
| **`.rvz`** | `"RVZ\x1"`（`RVZ_MAGIC = 0x015A5652`） | 同上 |
| `.nkit` | —— | **未找到权威规范**；与 Redump SHA-1 的 1:1 关系仅为推断 |

**WIA/RVZ 结构**（[Dolphin `docs/WiaAndRvz.md`](https://github.com/dolphin-emu/dolphin/blob/master/docs/WiaAndRvz.md)）：
`wia_file_head_t` 在 `0x0`，`0x48` 字节，*"its format will never be changed"*：`magic[4]`、`version`、`version_compatible`、`disc_size`、`disc_hash`(sha1)、**`iso_file_size`(u64)**、`wia_file_size`、`file_head_hash`。**所有整数为大端。**
`wia_disc_t` 在 `0x48`：`disc_type`（0 未知、**1 GameCube、2 Wii**）、`compression`（0 NONE、1 PURGE、2 BZIP2、3 LZMA、4 LZMA2；**RVZ 增加 5 = Zstandard 并移除 PURGE**）、`compr_level`、`chunk_size`、**`u8 dhead[0x80]`**、`n_part`…

**Redump ↔ 这些格式**：Redump 的 DAT 是**完整未压缩 ISO**，一个游戏一个 `<rom>`：
```xml
<game name="007 - Agent im Kreuzfeuer (Germany)" id="4803">
  <rom name="007 - Agent im Kreuzfeuer (Germany).iso" size="1459978240"
       crc="43c1d6a0" md5="a91e2b43028eee80f6e953a542dee9c9"
       sha1="34d610e08042b896fc5c077e6e7060fc00b77aa4"/>
</game>
```
> **【推断】** 因为 WIA/RVZ/WBFS 都保存 `iso_file_size` 并能逐字节还原 ISO，它们可以还原出 Redump 的 SHA-1；**NKit 则不一定**。

---

### A.17 Arcade / MAME

街机 ROM 的识别模型与所有卡带/光盘平台都不同：**MAME 不识别「一个 ROM 文件」，而是识别「一台机器所需的全套 ROM 芯片」。**

#### A.17.1 匹配机制（源码级）

来源：[MAME `src/frontend/mame/audit.cpp`](https://github.com/mamedev/mame/blob/master/src/frontend/mame/audit.cpp)、`src/lib/util/hash.h`

```c
// audit_one_rom：先取期望 CRC，用「zip 内文件名 + CRC」定位
uint32_t crc = 0;
bool const has_crc = record.expected_hashes().crc(crc);
if (has_crc)  filerr = file.open(record.name(), crc);
else          filerr = file.open(record.name());
if (!filerr)  record.set_actual(file.hashes(m_validation), file.size());
```
```c
// compute_status：三者全等才算 GOOD
if (record.expected_length() != record.actual_length())          FOUND_WRONG_LENGTH
else if (expected_hashes().flag(FLAG_NO_DUMP))                   FOUND_NODUMP
else if (record.expected_hashes() != record.actual_hashes())     FOUND_BAD_CHECKSUM
else if (expected_hashes().flag(FLAG_BAD_DUMP))                  GOOD_NEEDS_REDUMP
else                                                             GOOD
```

`hash_collection`（`src/lib/util/hash.h`）**只定义两种哈希**：
```c
static constexpr char HASH_CRC  = 'R';
static constexpr char HASH_SHA1 = 'S';
```

| 要素 | 说明 |
|---|---|
| **set 名** | short name（如 `puckman`），**等于 zip 文件名** |
| **定位** | zip 内**每个成员文件**的 `name` + `CRC32` |
| **判定** | `size` + `CRC32` + `SHA1` 三者全等 |
| **无 MD5** | MAME 全程不使用 MD5 |
| **父子关系** | `cloneof`（游戏系谱）与 `romof`（ROM 共享 / BIOS 继承）**分别表达，可以不同** —— 一个 clone 可能 `romof` 指向 BIOS set 而 `cloneof` 指向父游戏 |

**ROM set 三种组织方式**（[官方文档](https://docs.mamedev.org/usingmame/aboutromsets.html) 原文）：
> **Non-merged**: "absolutely everything necessary for a given game to run in one ZIP file."
> **Split**: "the parent set contains all of the normal data it should, and the clone sets contain **only what has changed**."
> **Merged**: 父子合并进一个 zip。
> CHD 文件 "**should not** be stored in PKZIP archives"。

#### A.17.2 `-listxml` DTD

来源：`src/frontend/mame/infoxml.cpp`（注意文件已从 `info.cpp` 更名）
```c
#define XML_ROOT    "mame"
#define XML_TOP     "machine"
```
```dtd
<!DOCTYPE mame [
<!ELEMENT mame (machine+)>
	<!ATTLIST mame build CDATA #IMPLIED>
	<!ATTLIST mame mameconfig CDATA #REQUIRED>
	<!ELEMENT machine (description, year?, manufacturer?, biosset*, rom*, disk*, device_ref*, sample*, chip*, display*, sound?, input?, dipswitch*, …, softwarelist*)>
		<!ATTLIST machine name CDATA #REQUIRED>
		<!ATTLIST machine sourcefile CDATA #IMPLIED>
		<!ATTLIST machine isbios (yes|no) "no">
		<!ATTLIST machine isdevice (yes|no) "no">
		<!ATTLIST machine cloneof CDATA #IMPLIED>
		<!ATTLIST machine romof CDATA #IMPLIED>
		<!ATTLIST machine sampleof CDATA #IMPLIED>
		<!ELEMENT rom EMPTY>
			<!ATTLIST rom name CDATA #REQUIRED>
			<!ATTLIST rom bios CDATA #IMPLIED>
			<!ATTLIST rom size CDATA #REQUIRED>
			<!ATTLIST rom crc CDATA #IMPLIED>
			<!ATTLIST rom sha1 CDATA #IMPLIED>
			<!ATTLIST rom merge CDATA #IMPLIED>
			<!ATTLIST rom region CDATA #IMPLIED>
			<!ATTLIST rom offset CDATA #IMPLIED>
			<!ATTLIST rom status (baddump|nodump|good) "good">
			<!ATTLIST rom optional (yes|no) "no">
		<!ELEMENT disk EMPTY>
			<!ATTLIST disk name CDATA #REQUIRED>
			<!ATTLIST disk sha1 CDATA #IMPLIED>
			<!ATTLIST disk index CDATA #IMPLIED>
			<!ATTLIST disk status (baddump|nodump|good) "good">
]>
```
> **MAME 的 `<rom>` 只有 crc + sha1，没有 md5**（Logiqx DTD 里有 md5，MAME 自己的没有）。**`<disk>` 只有 sha1**，且该 sha1 是 **CHD 的 combined raw+meta SHA1**（见 B.2.4）。

#### A.17.3 Software lists（`hash/*.xml`）—— 卡带机的芯片级 DAT

官方 DTD：[`hash/softwarelist.dtd`](https://raw.githubusercontent.com/mamedev/mame/master/hash/softwarelist.dtd)
```dtd
<!ELEMENT softwarelist (notes?, software+)>
	<!ATTLIST softwarelist name CDATA #REQUIRED>
	<!ELEMENT software (description, year, publisher, notes?, info*, sharedfeat*, part*)>
		<!ATTLIST software name CDATA #REQUIRED>
		<!ATTLIST software cloneof CDATA #IMPLIED>
		<!ATTLIST software supported (yes|partial|no) "yes">
		<!ELEMENT info EMPTY>
			<!ATTLIST info name CDATA #REQUIRED>
			<!ATTLIST info value CDATA #IMPLIED>
		<!ELEMENT part (feature*, dataarea*, diskarea*, dipswitch*)>
			<!ATTLIST part name CDATA #REQUIRED>
			<!ATTLIST part interface CDATA #REQUIRED>
			<!ELEMENT feature EMPTY>          <!-- pcb-type、mapper type 等 -->
			<!ELEMENT dataarea (rom*)>
				<!ATTLIST dataarea name CDATA #REQUIRED>
				<!ATTLIST dataarea size CDATA #REQUIRED>
				<!ELEMENT rom EMPTY>
					<!ATTLIST rom name CDATA #IMPLIED>
					<!ATTLIST rom size CDATA #IMPLIED>
					<!ATTLIST rom crc CDATA #IMPLIED>
					<!ATTLIST rom sha1 CDATA #IMPLIED>
					<!ATTLIST rom offset CDATA #IMPLIED>
					<!ATTLIST rom loadflag (load16_byte|…|reload|fill|continue|reload_plain|ignore) #IMPLIED>
			<!ELEMENT diskarea (disk*)>
```
> ⚠ **DTD 中没有 `detector` / `header` / `skipper` 元素 —— MAME 不使用 clrmamepro 的 skipper 机制。**

**真实 `hash/nes.xml` 摘录**：
```xml
<softwarelist name="nes" description="Nintendo Entertainment System cartridges">
	<software name="89denku">
		<description>'89 Dennou Kyuusei Uranai by Jingūkan (Japan)</description>
		<year>1988</year>
		<publisher>Induction Produce</publisher>
		<info name="serial" value="IPC-J1-01"/>
		<info name="release" value="19881210"/>
		<info name="alt_title" value="神宮館'89電脳九星占い"/>
		<part name="cart" interface="nes_cart">
			<feature name="slot" value="sxrom" />
			<feature name="pcb"  value="HVC-SGROM" />
			<feature name="mmc1_type" value="MMC1A" />
			<dataarea name="prg" size="262144">
				<rom name="ipc-j1-0 prg" size="262144" crc="ba58ed29" sha1="56fe858d1035dce4b68520f457a0858bae7bb16d" offset="00000" />
			</dataarea>
			<!-- 8k VRAM on cartridge -->
			<dataarea name="vram" size="8192" />
		</part>
	</software>
```

> ⭐ **`<info name="alt_title">` 里存的是原生日文/中文标题** —— 对中文显示名有直接价值。
> `<info name="serial">` 存的是卡带丝印序列号（如 `IPC-J1-01`、`NES-TY-USA`）。

**⭐ MAME software list 的 ROM 是无头的（已验证）**：NES 条目被拆成独立的 `prg` / `chr` / `vram` **dataarea**，每块对应实体卡带上的一颗芯片（文件名用真实芯片标签，如 `nes-ty-0 prg.u1`），**没有 iNES 头**。`nes.xml` 头部注释原文：
> "whenever a chip was labeled, the label writings have been used as filenames, so that in principle one could **burn back the content on the right chip**."
> "for dumps which hasn't been documented from a cart (e.g. the ones obtained by **splitting PRG and CHR banks from a iNES or UNIF files**)…"

#### A.17.4 ⭐ NES 的完整证据链（跨库交叉验证，本调研最有价值的产出之一）

**案例 A：`'89 Dennou Kyuusei Uranai (Japan)`** —— 纯 PRG 卡带（无 CHR ROM，用 8K VRAM）

| 来源 | 文件 | size | CRC32 | SHA1 |
|---|---|---|---|---|
| No-Intro **Headered** | `.nes` | 262160 | `3577ab04` | `b4cbebec…` |
| No-Intro **Headerless** | `.unh` | 262144 | **`ba58ed29`** | **`56fe858d1035dce4b68520f457a0858bae7bb16d`** |
| **MAME** `89denku` 的 prg dataarea | `ipc-j1-0 prg` | 262144 | **`ba58ed29`** | **`56fe858d1035dce4b68520f457a0858bae7bb16d`** |
| **TOSEC**（带头） | `.nes` | 262160 | `679db8f7` | `3918c52a…` |

结论：
1. ✅ **No-Intro headerless 的 CRC/SHA1 与 MAME 的 PRG dataarea 完全一致** —— 因为该卡只有一颗 PRG 芯片，无头 ROM ≡ PRG 芯片内容。
2. ⚠ **TOSEC 与 No-Intro 的「带头」文件大小相同（262160）但 CRC/MD5/SHA1 全不同** —— 两个带头文件不是同一字节序列。**【推断】** 差异位于那 16 字节的 iNES 头（mapper/mirroring/flag 字节不同），因为无头部分已被两个独立来源印证为同一内容。
   > ⭐ **这直观说明：带头 CRC 不可跨库移植，headerless 才是稳定标识符。**
3. No-Intro 甚至把头字节直接写进 DAT：`header="4E 45 53 1A 10 00 10 08 00 00 00 07 00 00 00 01"`
   （`4E 45 53 1A` = `NES\x1A`；`10` = 16×16KB PRG = 256KB；`00` = 0×8KB CHR，即用 VRAM —— **与 MAME 的 `<dataarea name="vram" size="8192"/>` 完全吻合**）

**案例 B：`10-Yard Fight (USA, Europe)`** —— PRG + CHR 卡带

| 来源 | 内容 | size | CRC32 |
|---|---|---|---|
| No-Intro Headered | `.nes` 单文件 | 40976 | `c986cda2`（`header="4E 45 53 1A 02 01 00 08 …"` → 2×16K PRG，1×8K CHR） |
| No-Intro Headerless | `.unh` 单文件 | 40960 | `3d564757` |
| MAME `10yard` | `prg` dataarea | 32768 | `df58fc5a` |
| MAME `10yard` | `chr` dataarea | 8192 | `2b8336ee` |

40960 = 32768 + 8192。**No-Intro 的单一 CRC 覆盖 PRG‖CHR 的拼接；MAME 给出两个独立的按芯片 CRC。二者无法直接比较**，必须先按 iNES 头声明的 PRG/CHR 尺寸切分再逐块比对。

> **【推断】对 NES 识别的实践建议**：
> 1. 读前 4 字节，若为 `4E 45 53 1A` 则剥离 16 字节（若 `Flags6 bit2` 置位再剥 512 字节 trainer）
> 2. 用剥离后的数据算 CRC32/SHA1 → 匹配 **No-Intro Headerless** DAT
> 3. 用**整文件**算 CRC32/SHA1 → 匹配 **No-Intro Headered** DAT 与 **TOSEC**（TOSEC 按带头哈希）
> 4. 若要匹配 MAME software list，还需按 iNES 头 byte 4（PRG×16KB）/ byte 5（CHR×8KB）切成 prg/chr 两段再分别算 CRC32/SHA1 —— **这同时给了你一个天然的「分段指纹」**（见 D.4.2）

---

### A.18 平台横向对比

#### A.18.1 各平台「自带识别能力」

| 平台 | 强 magic | 内建校验和 | 内建唯一序列号 | 外挂文件头 | 识别难度 |
|---|---|---|---|---|---|
| NES | `NES\x1A`（**仅有头文件才有**） | **无** | **无** | **有**（16B + 可选 512B trainer） | **最难** |
| SNES | 无（靠打分启发式） | 有（16-bit checksum + complement） | 部分有（4 字符 game code，需 `$FFDA==0x33`） | **有**（512B copier） | 中–难 |
| GB/GBC | 有（48B logo @ `0x0104`） | 有（8-bit header + 16-bit global） | 无 | 无 | 易 |
| GBA | 有（156B logo + `0xB2==0x96`） | 有（8-bit complement） | **有**（4 字符 `AGB-UTTD`） | 无 | **最易** |
| NDS | 有（logo CRC 常量 `CF56h`） | 有（header CRC16） | **有**（4 字符 gamecode） | 无 | 易 |
| 3DS | 有（`NCSD` / `NCCH` @ `0x100`） | 有（RSA 签名 + SHA-256） | **有**（Title ID + Product Code） | 无 | 易 |
| N64 | 弱（首字节，n64brew 警告勿依赖） | **有（8 字节 check code）** | **有**（4 字符 Game Code） | 无（但**有 3 种字节序**） | 中 |
| Mega Drive | 有（`0x100` 的 `"SEGA …"`） | 有（16-bit，非强制） | **有**（14 字节 serial） | 有（`.smd` 交错） | 易 |
| SMS / GG | 有（`"TMR SEGA"`），**但可选** | 有（LE 16-bit，GG/JP 常错） | 有（product code），**但头可能不存在** | 有（512B） | 中–难 |
| PC Engine | **无** | **无** | **无** | 有（`size & 512`） | **最难（仅能靠哈希）** |
| PS1 | 有（系统区 license 串） | — | **有**（`SYSTEM.CNF` BOOT） | — | 易 |
| PS2 | 有（`"PLAYSTATION"` @ `0x8008`/`0x9320`） | — | **有**（`BOOT2`） | — | 易 |
| PSP | 有（`"PSP GAME"` @ `0x8008`） | — | **有**（`DISC_ID`） | — | 易 |
| Saturn | 有（`"SEGA SEGASATURN"`） | — | **有**（product number @ `0x20`） | — | 易 |
| Dreamcast | 有（`"SEGA SEGAKATANA"`） | 有（CRC-16 覆盖 `0x40`–`0x4F`） | **有**（product number @ `0x40`） | — | 易 |
| GameCube | 有（`0xC2339F3D` @ `0x1C`） | — | **有**（Game Code + Maker Code） | — | 易 |
| Wii | 有（`0x5D1C9EA3` @ `0x18`） | — | **有**（同上） | — | 易 |

#### A.18.2 推荐的识别主键设计【推断】

| 平台 | 主键 | 副键 / 交叉校验 |
|---|---|---|
| NES | **去头（16B + 可选 512B trainer）后到 EOF 的 SHA-1/CRC32** | 整文件 CRC32；NesCartDB 逐芯片 PRG/CHR CRC32 |
| SNES | **去 512B copier 头后整文件的 SHA-1/CRC32** | 内部 title + game code + version + checksum |
| GB/GBC | 整文件 SHA-1/CRC32 | title + global checksum(`0x014E`, BE) + mask ROM version |
| GBA | 整文件 SHA-1/CRC32 | **game code(`0x0AC`)** + maker code + software version |
| NDS | 整文件 SHA-1 | gamecode + makercode + ROM version（用 `0x15E` 校验） |
| 3DS | Title ID | NCCH Product Code + Title version |
| N64 | **归一化到 z64 后**的 SHA-1 | **`0x10` 的 8 字节 check code** + Game Code + version |
| Mega Drive | 整文件 SHA-1 | serial(`0x180`) + 重算 checksum |
| SMS / GG | **CRC32 数据库（必需）** | product code + version + region（若头存在） |
| PC Engine | **剥离 copier 头后的 CRC32（唯一手段）** | `.sgx` 扩展名 |
| PS1 | Redump 逐轨 MD5/SHA-1 | `SYSTEM.CNF` 序列号；DuckStation XXH64 |
| PS2 | 序列号 | 引导 ELF 的 XOR 折叠「CRC」 |
| PSP | `DISC_ID` | ISO 的 SHA-1 |
| Saturn / DC | product number | IP.BIN 其余字段；Redump 逐轨哈希 |
| GC / Wii | Game Code + Maker Code | 完整 ISO 的 SHA-1（Redump） |

#### A.18.3 最容易踩的坑汇总【推断】

1. **NES 头部字节几乎全不可信** —— ripper 署名污染 byte 7–15（`"DiskDude!"` 会让 `byte7 & 0x0C == 0x04`，误判为 archaic iNES）
2. **NES trainer**：`Flags6 bit2` 置位时 PRG 后移 512 字节
3. **SNES copier 头判据不统一**：bsnes `size & 0x7fff == 512` vs 文档 `filesize % 1024 == 512`
4. **SNES 头定位必须四点打分**，不能靠扩展名或单点探测
5. **SNES region 字段不可信** —— bsnes 源码注释明确说 homebrew/hack 常改主 region 却忘改扩展头 region
6. **GB title 长度是变长语义**（16 / 15 / 11 字节，取决于卡带年代）
7. **GB global checksum 是 big-endian**，且排除自身两字节
8. **GBA logo 不是 100% 固定**：`0x09C`（bit2/bit7）与 `0x09E`（bit0/bit1）允许变化
9. **NDS `0x15C` 不是 secure-area CRC**，而是 logo CRC（固定 `CF56h`）；secure area CRC 在 `0x06C`
10. **NDS CRC16 请按 ndstool 实现**，GBATEK 的伪代码逐字实现无法复现标准校验值
11. **N64 libultra 版本在 `0x0C` 不是 `0x10`**；`0x10` 是 8 字节 check code
12. **N64 必须先归一化字节序**再做任何哈希或头解析
13. **Mega Drive region 字段有两种不兼容格式**，`"E"` 二义；生产级实现都带异常表
14. **SMS ROM size nibble 是「校验和范围选择器」**，被有意写小是常态，不是真实大小
15. **PC Engine copier 判据是 `size & 512`**，不是 `% 8192 == 512`
16. **RetroArch 读的是 2352 字节原始扇区**，所有 Sega 磁盘偏移要 +0x10
17. **PCSX2 的「CRC」不是 CRC-32**，是 32 位字的 XOR 折叠
18. **Redump wiki 已迁移**到 `wiki.redump.info`（`wiki.redump.org` 失效）

---

## 第 B 章 · 基于哈希与 DAT 数据库的识别

### B.1 No-Intro

#### B.1.1 站点结构与 DAT 集合分类

`https://no-intro.org` 只是门户；实际数据库是 **DAT-o-MATIC（DoM）** `https://datomatic.no-intro.org`。

下载页顶部 7 个页签（原文）：
```
Select | Standard DAT | P/C List | P/C XML | Scene | Daily | DB | Dumplog
```

DAT 列表分为 **5 个顶层分组**：
```
No-Intro  →  Source Code  →  Unofficial  →  Non-Redump  →  Non-Game
```

> ⭐ **没有 "ROM Hacks" 分组。** 对下载页 HTML 全文做大小写不敏感的 `hack` 搜索，正文零命中；wiki 的 `Special:AllPages`（319 页）中也没有 ROM Hacks 页面。**No-Intro 不收录 ROM hack。**

**「Aftermarket」不是集合，而是文件名标签**（[Aftermarket Guide](https://wiki.no-intro.org/index.php?title=Aftermarket_Guide) 原文）：
> **(Aftermarket)** = Any unlicensed game that was first distributed **after the last-known original licensed game released for that platform**. All aftermarket games must be unlicensed…

| 情况 | 标签 |
|---|---|
| 授权游戏，生命周期内发行 | 无标签 |
| 授权游戏，官方复刻 | `(Limited Run Games)` `(Retro-Bit)` `(Virtual Console)` `(Switch Online)` `(Evercade)` `(Steam)` |
| 未授权 + 原创素材 + 生命周期内 | `(Unl)` |
| 未授权 + 盗用素材 + 生命周期内 | `(Pirate)` |
| 未授权 + 原创素材 + 生命周期后 | `(Aftermarket) (Unl)` |
| 未授权 + 盗用素材 + 生命周期后 | `(Aftermarket) (Pirate)` |

该页还给出各平台的 **Aftermarket 起始年**（NES=1995、SNES=2000、GB=2001、GBA=2008、N64=2002、Mega Drive=2002、NDS=2016…）。

#### B.1.2 ⚠ 下载方式、robots.txt 与封禁（含实测警告）

**robots.txt**（`https://datomatic.no-intro.org/robots.txt`，HTTP 200，原文）：
```
User-agent: *
Disallow: 
Crawl-delay: 5 # wait 5 seconds
Request-rate: 1/5 # 1 page every 5 seconds
Sitemap: http://datomatic.no-intro.org/sitemap.xml
```
即**不禁止任何路径**，但要求 5 秒间隔 / 每 5 秒 1 页。

> ⚠⚠ **【实测警告】** 本次调研中，对 `index.php?page=select`（一个不存在的 `page` 值）发起**一次**请求后，**整个 `index.php` 立即被 IP 封禁**，此后所有请求返回：
> ```
> Something went wrong with your client or another client on your network:
> Wrong 'page' param in URL.
> Error id: 2861.
> The ban won't be lifted until you contact me. To remove the ban: shippa6@hotmail.com
> ```
> **结论：DoM 对畸形 URL 参数会即时封 IP，解封需人工邮件申请。**
> 注意 **`/stuff/` 下的静态文件**（`terms.txt`、`*.xsd`、`header_*.zip`）**不受此封禁影响**，仍可正常下载。

**实际下载机制**：真实自动化实现（[hugo19941994/auto-datfile-generator `no-intro.py`](https://github.com/hugo19941994/auto-datfile-generator/blob/master/no-intro.py)）必须用 **headless Firefox + Selenium** 点击：
```python
driver.get("https://datomatic.no-intro.org")
# select "DOWNLOAD" → select "daily"
if key == "standard":     driver.find_element(value="//input[@name='dat_type' and @value='dat']").click()
if key == "parent-clone": driver.find_element(value="//input[@name='dat_type' and @value='pc']").click()
# select "Request" → sleep(5) → select "Download"
```
即 **两阶段 Request →（等待）→ Download 表单流程**。

同生态另一佐证（[oxyromon README](https://github.com/alucryd/oxyromon)）：
> "Redump offers direct downloads, but no summary, whereas **No-Intro offers a summary but no direct downloads**."

> **【推断】** 这个两步流 + 5 秒等待等价于软验证码。`datoso` 的文档则警告有时会出现真验证码且失败会被封。
> **站点条款**（`https://datomatic.no-intro.org/stuff/terms.txt`，可直接取）只涉及版权免责，**不含任何关于下载自动化的条款**。

> ✅ **可行替代**：使用 [libretro-database](https://github.com/libretro/libretro-database) 的 git 镜像，或第三方每日重打包（如 `auto-datfile-generator` 的 Release）。

#### B.1.3 ⭐ 现代 No-Intro DAT 不是 Logiqx DTD，而是自有 XSD

**这是一条重要更正。** 2026 年真实 No-Intro DAT 的根元素是：
```xml
<?xml version="1.0"?>
<datafile xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"
          xsi:schemaLocation="https://datomatic.no-intro.org/stuff https://datomatic.no-intro.org/stuff/schema_nointro_datfile_v4.xsd">
```
**没有 Logiqx 的 `<!DOCTYPE ... datafile.dtd>`。** 实测 326 个 P/C dat 中：v3 = 249 个、v4 = 77 个。v4 相对 v3 只多了 `<trademarks>` 与 `<piracy>` 两个 header 元素。

**官方 XSD v4 要点**（`https://datomatic.no-intro.org/stuff/schema_nointro_datfile_v4.xsd`）：
```xml
<xs:element name="header">
  <xs:sequence>
    <xs:element name="id"          type="xs:int" />         <!-- 必填 -->
    <xs:element name="name"        type="xs:string" />
    <xs:element name="description" type="xs:string" />
    <xs:element name="version"     type="xs:string" />
    <xs:element name="date"        type="xs:string" minOccurs="0" />
    <xs:element name="author"      type="xs:string" />
    <xs:element name="homepage"    type="xs:string" minOccurs="0" />
    <xs:element name="url"         type="xs:string" minOccurs="0" />
    <xs:element name="trademarks"  type="xs:string" minOccurs="0" />   <!-- v4 新增 -->
    <xs:element name="piracy"      type="xs:string" minOccurs="0" />   <!-- v4 新增 -->
    <xs:element name="subset"      type="xs:string" minOccurs="0" />
    <xs:element name="clrmamepro"  minOccurs="0">
        <xs:attribute name="forcenodump" default="obsolete"/>   <!-- obsolete|required|ignore -->
        <xs:attribute name="header" type="xs:string" use="optional" />
    </xs:element>
    <xs:element name="romcenter" minOccurs="0">
        <xs:attribute name="plugin" type="xs:string" use="optional" />
    </xs:element>
  </xs:sequence>
</xs:element>

<xs:element name="game">
  <xs:sequence>
    <xs:element name="category"    minOccurs="0" maxOccurs="unbounded" />
    <xs:element name="description" type="xs:string" />
    <xs:element name="rom">
        <xs:attribute name="name"   type="xs:string"      use="required" />
        <xs:attribute name="size"   type="xs:unsignedInt" use="required" />
        <xs:attribute name="crc"    type="xs:string"      use="required" />
        <xs:attribute name="md5"    type="xs:string"      use="required" />
        <xs:attribute name="sha1"   type="xs:string"      use="required" />
        <xs:attribute name="sha256" type="xs:string"      use="optional" />
        <xs:attribute name="status" type="xs:string"      use="optional" />
        <xs:attribute name="serial" type="xs:string"      use="optional" />
        <xs:attribute name="header" type="xs:string"      use="optional" />
    </xs:element>
    <xs:element name="release" minOccurs="0" maxOccurs="unbounded">
        <xs:attribute name="name"   use="required" />
        <xs:attribute name="region" use="required" />
    </xs:element>
  </xs:sequence>
  <xs:attribute name="name"      use="required" />
  <xs:attribute name="id"        use="optional" />
  <xs:attribute name="cloneof"   use="optional" />
  <xs:attribute name="cloneofid" use="optional" />
</xs:element>
```

**与 Logiqx DTD 的关键差异**：
- 无 `romof` / `sampleof` / `isbios` / `sourcefile` / `board` 等 MAME 遗留属性
- 无 `<disk>` / `<sample>` / `<archive>` / `<biosset>`
- **新增 `sha256`、`serial`、`header`（rom 级）、`cloneofid`、`id`、`subset`**

> ⚠ **XSD 与现实不符的两处（实测）**：① XSD 声明 `<rom>` 恰好出现 1 次，但 131 个 P/C dat 中存在多 `<rom>` 的 game（cue+bin 光盘集）；② XSD 声明 `md5` 为 required，但实测 cue 条目常常没有 md5。
> **不要用该 XSD 做严格校验。**

真实 Standard DAT 摘录：
```xml
<game name="007 - Everything or Nothing (USA, Europe) (En,Fr,De)" id="1256">
    <description>007 - Everything or Nothing (USA, Europe) (En,Fr,De)</description>
    <rom name="007 - Everything or Nothing (USA, Europe) (En,Fr,De).gba" size="8388608"
         crc="9d4f1e18" md5="b63b2244edc2385ae1eab9c8ee448c6f"
         sha1="fc6163f99b71b05c10686a0d29010b31274e1dc4"
         sha256="45d0003d5612d4f0a4b57adcf7ddbe6bb10c1e7f7c01f02472967fc690cf6676"
         status="verified" serial="BJBE"/>
</game>
```

#### B.1.4 哈希覆盖率：SHA256 **确实存在于现代 DAT**

对完整每日包的全量扫描实测：

| 包 | dat 数 | 含 `sha256=` | 含 `rom/@serial` | 含 `rom/@status` | 含 `rom/@header` | `game/@id` | clone 表示法 |
|---|---|---|---|---|---|---|---|
| **Standard** | 334 | **257** | 79 | 134 | 8 | 330 | `id` + `cloneofid`（数字 ID） |
| **Parent-Clone** | 326 | **0** | 0 | 122 | 0 | 0 | `cloneof="<父档名>"`（按名字） |

- **CRC32 / MD5 / SHA1 = 必有**
- **SHA256 = 现代 Standard DAT 中普遍存在**（334 中 257 个）
- `status="verified"` 表示该条目有 ≥2 个可信 dump（wiki：*"Verified = ROM has two or more Trusted Dumps"*）

wiki [File_Convention](https://wiki.no-intro.org/index.php?title=File_Convention) 明确列出数据库字段：
```
===CRC32===    Custom XML name: crc
===MD5===      Custom XML name: md5
===SHA-1===    Custom XML name: sha1
===SHA-256===  Custom XML name: sha256
```

**Standard vs Parent-Clone 的实测差异**（同一系统 NES Headerless，同版本 20260704-141639）：

| | Standard | Parent-Clone |
|---|---|---|
| game 数 | 4507 | 7291 |
| 亲子关系 | `cloneofid="0003"` | `cloneof="10-Yard Fight (USA, Europe)"` |
| sha256 | 有 | 无 |
| `<category>` | 有（Games/Applications/Preproduction/…） | 无 |

> **【推断】** 这些差异部分来自 DoM 下载表单的可选项，而非「Standard vs P/C」的内在定义。
> **可确定的核心区别**：Standard DAT 每个 `<game>` 是独立条目（父子用 `cloneofid` 数字引用，ROM 管理器可忽略）；Parent-Clone 用 Logiqx 风格的 `cloneof="父档名"`，让 clrmamepro/RomVault 能做 **1G1R** 合并与筛选。

#### B.1.5 Headered / Headerless 双 DAT（实测对照）

No-Intro 对 NES 同时发布两个 DAT：
```
Nintendo - Nintendo Entertainment System (Headered)   (20260704-141639).dat
Nintendo - Nintendo Entertainment System (Headerless) (20260704-141639).dat
```
- **Headerless** DAT：header 含 `<clrmamepro header="No-Intro_NES.xml"/>` 与 `<romcenter plugin="nes.dll"/>`；ROM 扩展名 `.unh`
- **Headered** DAT：**不含** clrmamepro header 属性；扩展名 `.nes`；并在 `<rom>` 上用 `header=` 属性**逐字记录那 16 个头字节**

同一游戏（`'89 Dennou Kyuusei Uranai (Japan)`，id=0001）的真实对照：
```xml
<!-- Headered DAT -->
<rom name="'89 Dennou Kyuusei Uranai (Japan).nes" size="262160"
     crc="3577ab04" md5="44091221ff27af8f274f210dec670bb1"
     sha1="b4cbebec2a49f8bf5454a39424dff567c50d901c"
     sha256="e528c956eab225ad74b83d0eae2f280de8e4849e0341cf1d081b2b1ba5d787b3"
     status="verified"
     header="4E 45 53 1A 10 00 10 08 00 00 00 07 00 00 00 01"/>

<!-- Headerless DAT -->
<rom name="'89 Dennou Kyuusei Uranai (Japan).unh" size="262144"
     crc="ba58ed29" md5="4187a797e33bc96a96993220da6f09f7"
     sha1="56fe858d1035dce4b68520f457a0858bae7bb16d"
     status="verified"/>
```
262160 − 262144 = 16。`rom/@header` 属性只出现在 8 个 DAT 中（NES Headered、C64、Mega Drive、Game Boy Color、FM-7、Sega PICO 等）。

**使用 skipper 的 DAT 恰好 7 个，覆盖 4 个 skipper**：
```
Nintendo - Nintendo Entertainment System (Headerless)  → No-Intro_NES.xml
Nintendo - Family Computer Disk System (FDS)           → No-Intro_FDS.xml
Atari - Atari 7800 (A78) / (BIN)                       → No-Intro_A7800.xml
Atari - Atari Lynx (LNX) / (LYX) / (BLL)               → No-Intro_LNX.xml
```
与 oxyromon README 一致：*"This currently affects Nintendo Entertainment System (Headerless), Famicom Disc System, Atari 7800, and Atari Lynx."*

#### B.1.6 命名规范

来源：[Naming Convention](https://wiki.no-intro.org/index.php?title=Naming_Convention)（基于 2007-10-30 的官方约定）

**总语法**（原文）：
```
[BIOS flag] Title (Region) (Languages) (Version) (Devstatus) (Additional) (Special) (License) [Status]
```
> "The only mandatory elements are **Title and Region**."

| 字段 | 规则 |
|---|---|
| **字符集** | 仅 7-bit ASCII 的 `a-z A-Z 0-9 SPACE $ ! # % ' ( ) + , - . ; = @ [ ] ^ _ { } ~`；禁止 `\ / : * ? " < > \| \``。UTF-8 原名放在单独的 `namealt` 字段 |
| **排序** | 冠词后置：`The Legend of Zelda` → `Legend of Zelda, The`；副标题用 ` - ` 分隔，**副标题首个冠词不后置** → `Legend of Zelda, The - A Link to the Past` |
| **Region（单区）** | `(Australia) (Brazil) (Canada) (China) (France) (Germany) (Hong Kong) (Italy) (Japan) (Korea) (Netherlands) (Spain) (Sweden) (USA)` |
| **Region（多区）** | `(World)`（三大区全发）、`(Europe)`（含 Australia）、`(Asia)`、`(Japan, USA)`、`(Japan, Europe)`、`(USA, Europe)`。`(USA)` 含 Canada |
| **Languages** | ISO 639-1，首字母大写次字母小写，逗号无空格；**仅当多于一种语言时才加**。规定顺序：`En Ja Fr De Es It Nl Pt Sv No Da Fi Zh Ko Pl`。例：`Super Metroid (Japan, USA) (En,Ja)` |
| **Version** | `(v1.05)` 或 `(Rev A)`/`(Rev 1)`，仅当高于首发版本时添加 |
| **Devstatus** | `(Beta)` / `(Beta 1)` / `(Proto)`（未发行）/ `(Sample)` |
| **Additional** | 仅用于区分多版本，如 `(Rumble Version)`、`(Doritos Promo)` |
| **Special** | `(ST)` `(MB)` `(NP)` 等 |
| **License** | 未授权加 `(Unl)` |
| **Status** | `[b]` = bad and/or hacked，**由 source/file 条目自动生成** |
| **BIOS** | 前缀 `[BIOS]`，如 `[BIOS] Game Genie (USA, Europe) (Unl)` |

**日文罗马化**：ASCII 兼容的 Hepburn；助词 を→`o`、へ→`e`；外来词还原原语言拼写（`Pocket Monsters` 而非 `Poketto Monsutaa`）。

> ⚠ **注意**：`(Zh)` 在语言表里存在，但它表示**官方中文版**（如神游、iQue），**不表示民间汉化**。No-Intro 没有任何表示民间汉化的标记。

---

### B.2 Redump

#### B.2.1 下载与站点状态

`http://redump.org/downloads/`（⚠ **只有 HTTP 可用**，HTTPS 在本次调研环境握手失败）。

> ✅ **无验证码、无 JS、直接 GET 返回 zip** —— 实测 `curl http://redump.org/datfile/ngcd/` 直接得到 `application/zip`。**与 No-Intro 形成鲜明对比。**

下载页按系统给出 5 类：
```
Systems | Cuesheets | Datfiles | Subchannels | Disc Keys | BIOS Datfiles
```
链接形态：`/cues/<sys>/`、`/datfile/<sys>/`、`/sbi/<sys>/`、`/gdi/<sys>/`（Dreamcast/Naomi/Chihiro/Triforce）、`/keys/<sys>/`（Xbox 360/PS3）。系统覆盖约 60 个。

> ⚠ **`wiki.redump.org` 当前整站 404**，官方 wiki 已迁移至 **`https://wiki.redump.info/`**（归档的 Main Page 上有迁移横幅）。历史内容需走 Wayback（用 `id_` 原始修饰符 + `curl --compressed`）。

#### B.2.2 DAT 格式与哈希粒度（实测）

Redump 使用**标准 Logiqx XML**（有 DOCTYPE），header 简洁，**没有 `<clrmamepro>` 元素**：
```xml
<?xml version="1.0"?>
<!DOCTYPE datafile PUBLIC "-//Logiqx//DTD ROM Management Datafile//EN" "http://www.logiqx.com/Dats/datafile.dtd">
<datafile>
	<header>
		<name>SNK - Neo Geo CD</name>
		<description>SNK - Neo Geo CD - Discs (111) (2026-05-06 12-21-03)</description>
		<version>2026-05-06 12-21-03</version>
		<date>2026-05-06 12-21-03</date>
		<author>redump.org</author>
		<homepage>redump.org</homepage>
		<url>http://redump.org/</url>
	</header>
```
> **哈希：CRC32 + MD5 + SHA1 三者齐全，无 SHA256。**（对 Neo Geo CD 111 条与 GameCube 2019 条全量检查确认。）

**CD 系统：每轨一个哈希 + cue 也被哈希**
```xml
<game name="Top Hunter - Roddy &amp; Cathy (World) (En,Ja)">
	<category>Games</category>
	<description>Top Hunter - Roddy &amp; Cathy (World) (En,Ja)</description>
	<rom name="Top Hunter … (World) (En,Ja).cue" size="4801" crc="bd21ee23" md5="e032d028…" sha1="c3fca41b…"/>
	<rom name="Top Hunter … (World) (En,Ja) (Track 01).bin" size="21934752" crc="c21bc460" md5="3c167960…" sha1="aeafea12…"/>
	<rom name="Top Hunter … (World) (En,Ja) (Track 02).bin" size="5371968"  crc="f30169b3" md5="8200e30e…" sha1="d53fd473…"/>
	…（共 37 轨）
</game>
```

要点：
- **每条轨道一个独立 `.bin`，各自有 CRC32/MD5/SHA1**
- ⭐ **`.cue` 文件本身是 dump 的一部分并被哈希** —— 这是 Redump 与其他库最大的结构性差异。cue 承载轨道布局、pregap、INDEX、轨道类型（`MODE1/2352`、`AUDIO`），**没有它无法从裸 bin 还原光盘**
- 轨道命名固定为 `(Track NN)`，两位零填充
- `<category>` 放在 `<game>` 内 —— **这不符合 Logiqx DTD v1.5**（该 DTD 只允许 `category` 出现在 `<header>`），是 Redump 的方言扩展

**DVD 系统：整镜像一个哈希，无 cue**
```xml
<game name="Resident Evil 4 (USA) (Disc 1)">
	<category>Games</category>
	<rom name="Resident Evil 4 (USA) (Disc 1).iso" size="1459978240"
	     crc="f5c51b40" md5="ca749757e3b9d119f3feb1f9f0f81bd7" sha1="9de89c7f6d8ffb2e27423900a39d4aec1439f4e3"/>
</game>
```
> 对 GameCube DAT 全部 2019 条扫描：**没有任何一条含多个 `<rom>`**。下载页也印证：GameCube / Wii / PSP 等只有 "Datfile"，**没有 "Cuesheets" 列**。

#### B.2.3 Dumping 标准

Dumping guide 原文：
> "**IMPORTANT: Compatible Disc Drive Requirement** — … requires that a drive from the **Compatible disc drive** list is used… **Submissions made using drives not on the list are likely to be discarded as ineligible.**"

工具链：
```bash
DiscImageCreator.exe cd d MyGameTitle.bin 8 /c2 /nl      # CD-ROM
DiscImageCreator.exe dvd d MyGameTitle_Dump1.iso 8       # DVD-ROM，需 dump 两次比对
edccchk "MyGameTitle (Track 01).bin"                     # 检测 EDC
psxt001z "MyGameTitle (Track 01).bin"                    # PS1：取 EXE Date
psxt001z --libcrypt "MyGameTitle.sub" > libcrypt.txt     # PS1 PAL：LibCrypt 检测
```
另有 **redumper** 作为新一代 CLI dumper。

**光盘页记录的完整识别字段**（真实例，`http://redump.org/disc/36628/`）：
```
System              Sony PlayStation
Media               CD
Category            Demos
Serial              SLPM 80238
EXE date            1998-04-16
Edition             Taikenban
EDC                 No
Anti-modchip        No
LibCrypt            No
Errors count        0
Number of tracks    1
Write offset        +2
Barcode             4 990951 977024
Comments            Internal Serial: SLPM-80238

Track(s) | Cuesheet
#  Type          Pregap      Length     Sectors   Size        CRC-32    MD5        SHA-1
1  Data/Mode 2   00:00:00    41:55:37   188662    443733024   6af29073  0aa29df8…  6bdd288b…

Rings
#  Mastering Code   Mastering SID Code  Mould SID Code  Write offset
1  SLPM-80238   1   IFPI L274           IFPI 455C       +2

Primary Volume Descriptor (PVD)
Creation  31 39 39 38 30 34 31 36 …   1998-04-16  03:08:30.00  +09:00
```

**Ring Code 四类**（Ring Code Guide 原文）：
> **Mastering Code (laser branded/etched)**、**Mastering SID Code**（"generally start with 'IFPI', then followed by four or five more characters"）、**Toolstamp or Mastering Code (engraved/stamped)**、**Mould SID Code**（"indicate what plant the disc was manufactured at"）

**Write offset**：驱动器读取偏移补偿值（如 `+2`），按盘记录，保证不同驱动器 dump 出的音轨字节对齐一致。

#### B.2.4 ⭐ CHD 与 Redump 的哈希关系（最易搞错的点）

**CHD v5 头中的两个 SHA1**（[MAME `src/lib/util/chd.h`](https://github.com/mamedev/mame/blob/master/src/lib/util/chd.h)）：
```
[ 64] uint8_t  rawsha1[20];    // raw data SHA1
[ 84] uint8_t  sha1[20];       // combined raw+meta SHA1
[104] uint8_t  parentsha1[20];
```
版本演进：V3 只有 raw（在 `[80]`）；V4 两者都有（`[48]` combined、`[88]` raw）；V5 顺序调换。

**combined SHA1 的算法**（`chd.cpp` `compute_overall_sha1`）：
```c
if (m_version < 4) return rawsha1;      // V3：二者相同
// 遍历所有带 CHD_MDFLAGS_CHECKSUM 的 metadata blob：
//   hashentry.tag = 4 字节 metatag（大端）；hashentry.sha1 = 该 blob 内容的 SHA1
// 按 memcmp 排序 hasharray
overall_sha1.append(&rawsha1, sizeof(rawsha1));
overall_sha1.append(hasharray.data(), hasharray.size() * sizeof(hasharray[0]));
```
> 即 **combined SHA1 = SHA1( rawsha1 ‖ 排序后的 [metatag + SHA1(blob)] 数组 )**

**三个 SHA1 的区别**：

| 名称 | 位置 | 含义 | **是否等于解出的 `.bin` 的 SHA1** |
|---|---|---|---|
| `chdman info` 的 `SHA1:` | V5 头 offset 84 | combined raw+meta | **否** |
| `chdman info` 的 `Data SHA1:` | V5 头 offset 64 | 压缩前「逻辑原始数据流」的 SHA1 | **否** |
| **Redump DAT 里的 `sha1`** | — | **单条 `.bin` 轨道文件**（及 `.cue`）的 SHA1 | 是（定义如此） |

> ⚠ **即便是 `Data SHA1`（rawsha1）也不等于任何一个 Redump 轨道的 SHA1。** 原因：CHD 的 raw stream 是**整张盘所有轨道按 CD 帧连续拼接、并按 hunk 边界补齐**的单一字节流，而 Redump 是**逐轨分开**的多个 `.bin`；此外 chdman 对 CD 采用 2352+96（含 subcode）的内部扇区布局，与 Redump 的纯 2352 用户数据也不同。

**如何用 CHD 校验 Redump**：

**方法 1（权威）—— 解包比对**：
```bash
chdman extractcd -i game.chd -o game.cue -ob game.bin   # 默认输出单一大 bin
chdman extractcd -i game.chd -o game.cue --splitbin     # -sb：每轨一个 .bin，与 Redump 一致
```
> ⚠ **【推断】** `.cue` 的哈希通常对不上 —— Redump DAT 把 cue 纳入哈希，而 chdman 是**自行重建** cue（注释、大小写、REM 行、路径写法都可能不同）。**这是实践中最常见的「误报」来源。**

**方法 2（快速，只读头）—— 读 CHD 的轨道 metadata**：
```bash
chdman info --verbose -i game.chd
```
metadata 格式字符串（`chd.cpp`）：
```c
CDROM_TRACK_METADATA2_FORMAT = "TRACK:%d TYPE:%s SUBTYPE:%s FRAMES:%d PREGAP:%d PGTYPE:%s PGSUB:%s POSTGAP:%d";
GDROM_TRACK_METADATA_FORMAT  = "TRACK:%d TYPE:%s SUBTYPE:%s FRAMES:%d PAD:%d PREGAP:%d PGTYPE:%s PGSUB:%s POSTGAP:%d";
```
metadata tag：`CHTR`（旧 CD 轨道）、**`CHT2`（现代 CD CHD）**、`CHSE`（session）、`CHGD`（GD-ROM）、`DVD `、`GDDD`（硬盘）。

由 `CHT2` 的 `TRACK`/`TYPE`/`FRAMES`/`PREGAP`/`POSTGAP` 可重建轨道数、每轨扇区数与类型，算出每轨 `.bin` 的**预期字节数**（`FRAMES × 每扇区字节数`），与 Redump DAT 的 `<rom size=...>` 比对。**这能快速确认「轨道结构是否匹配」，但不能证明字节内容相同。**

> ⭐ **`chdman info --verbose` 的唯一实质增益是不截断 metadata 字符串**（非 verbose 模式截断到 60 字符），而这正是读取完整轨道表所必需的。

**方法 3 —— MAME 自身**：`audit.cpp` 的 `audit_one_disk` 用的是 `chd_file::sha1()`，即 **combined SHA1**。
> 因此 **MAME `-listxml` 里 `<disk sha1="...">` 是 CHD 的 combined SHA1，改动任何 metadata 都会让它变化，而它与 Redump 的任何哈希都毫无关系。这是 MAME CHD 集与 Redump 集不能互相校验的根本原因。**

---

### B.3 TOSEC

#### B.3.1 命名规范（TNC 2015-03-23）

来源：[TOSEC Naming Convention](https://www.tosecdev.org/tosec-naming-convention)

**完整文法**（原文）：
```
Title version (demo) (date)(publisher)(system)(video)(country)(language)(copyright status)(development status)(media type)(media label)[dump info flags][more info]
```

**dump flags 顺序固定**（原文）：
```
[cr][f][h][m][p][t][tr][o][u][v][b][a][!]
```
> "With dump info flags relative to image modifications being ordered alphabetically first (**cr**acked, **f**ixed, **h**acked, **m**odified, **p**irated, **t**rained, **tr**anslated) followed by the ones related with information about the dump process: **o**ver dump, **u**nder dump, **v**irus, **b**ad dump, **a**lternate, verified dump [**!**]."
> "**Note:** `Title (date)(publisher)` is the bare **minimum** required for a renamed image."

| 字段 | 规则 |
|---|---|
| **Title**（必填） | `The`/`A` 后置加逗号；非英语冠词同理（`De`/`Die`/`Le/La/Les`） |
| **Version** | **无括号**，紧跟标题 —— `Legend of TOSEC, The v1.03b` / `Rev 1` / `v20000101` |
| **(demo)** | 唯一允许 `)` 与 `(` 之间有空格的字段。取值 `demo`、`demo-kiosk`、`demo-playable`、`demo-rolling`、`demo-slideshow` |
| **Date**（必填） | `(1986)`、`(199x)`、`(2000-01-0x)` |
| **Country** | ISO 3166-1 alpha-2，**大写**（`US` `JP` `DE` `CN`） |
| **Language** | ISO 639-1，**小写**（`en` `ja` `de` **`zh`**）。英语是默认，无 flag 即英语或语言中立；已有国家 flag 时默认为该国官方语言 |
| **Copyright** | `CW CW-R FW GW GW-R LW PD SW SW-R` |
| **Devstatus** | `alpha beta preview pre-release proto` |
| **Media Type** | `Disc Disk File Part Side Tape`，如 `(Disk 06 of 13)`、`(Side A)` |
| **禁止字符** | `/ \ ? : * " < > \|` |

#### B.3.2 ⭐ `[tr]` 翻译标志 —— 汉化识别的关键

规范原文：
> **Translated - [tr]** — "The original software has been **deliberately hacked/altered to translate into a different language** than originally published/released. If it is a **partial translation**, not fully complete, '**-partial**' should be appended to the language code."

| 语法 | 含义 |
|---|---|
| `[tr]` | Translation |
| **`[tr language]`** | **Translated to Language** |
| `[tr language-partial]` | 部分翻译 |
| **`[tr language Translator]`** | **由某译者/汉化组翻译成某语言** |
| `[tr language1-language2]` | 翻译成两种语言 |
| `[tr language1-partial-language2-partial Translator]` | 组合 |

> "Translator name is not allowed if language isn't identified too（`[tr Translator]` **not** allowed）."
> **语言码为 ISO 639-1 小写两字母 ⇒ 中文 = `zh`。**

**真实 DAT 验证**（`Nintendo Famicom & Entertainment System - Games - [NES] (TOSEC-v2025-01-15_CM).dat`，4.5 MB）：
```xml
<game name="1942 (1985-12-11)(Capcom)(JP-US)[tr es Emu4ever][98%]">
<game name="1942 (1985-12-11)(Capcom)(JP-US)[tr fr Generation IX][v1.00-20040810]">
<game name="1942 (1985-12-11)(Capcom)(JP-US)[tr zh MS emumax][v.20060217]">
<game name="1943 - The Battle of Midway (1988-10)(Capcom)(US)[tr pt BR Games]">
<game name="1944 (199x)(-)(AS)[p][tr zh MS emumax][v.20051205]">
```

该单个 DAT 的 flag 计数：`[tr ` = **5637**，`[h` = **13404**，`[b]` = **4578**，`[a]` = **981**，`[p` = **3945**，`[t ` = 12，`[cr` = 0，`[!]` = **0**。
（`[!]` 为 0 与 TNC 一致：*"This is currently only used in the **TOSEC-ISO** branch."*）

#### B.3.3 TOSEC 的 DAT 格式（实测）

**标准 Logiqx XML**，文件名后缀 `_CM`（clrmamepro 兼容）：
```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE datafile PUBLIC "-//Logiqx//DTD ROM Management Datafile//EN" "http://www.logiqx.com/Dats/datafile.dtd">
<datafile>
	<header>
		<name>Nintendo Famicom &amp; Entertainment System - Games - [NES]</name>
		<description>… (TOSEC-v2025-01-15)</description>
		<category>TOSEC</category>
		<version>2025-01-15</version>
		<author>etabeta - Cassiel - Newpain - mictlantecuhtle</author>
		<email>contact@tosecdev.org</email>
		<url>http://www.tosecdev.org/</url>
	</header>
	<game name="!Mario (2000-11-19)(Dokokalaki)(JP)[h][Super Mario Bros.]">
		<description>…</description>
		<rom name="….nes" size="40976" crc="2e2bf112" md5="b1a0e41c…" sha1="7849ccb4…"/>
	</game>
```

| 属性 | 值 |
|---|---|
| `<category>` 位置 | 在 `<header>` 内（**符合 Logiqx DTD**），取值 `TOSEC` / `TOSEC-ISO` / `TOSEC-PIX` |
| 哈希 | **CRC32 + MD5 + SHA1；无 SHA256** |
| `<clrmamepro>` 元素 | **无** |
| ⭐ **头处理** | **不使用 header skipper** —— NES 条目 size = 40976 / 262160 即**含 iNES 头**，TOSEC 按**带头文件原样**哈希 |

2025-03-13 datpack 结构：`TOSEC/`（4443 dat）+ `TOSEC-ISO/`（300 dat）+ `TOSEC-PIX/` + `CUEs/` + `Scripts/`，共 **4743 个 dat / 约 100 MB**，**直接下载完整 zip，无验证码**。

> ⭐⭐ **对汉化识别至关重要的推论**：因为 **TOSEC 按带头文件哈希，而 No-Intro headerless DAT 按去头哈希**，所以要同时匹配两者，**必须同时计算「含头」与「去头」两套哈希**（见 D.1）。若只算一套，就会丢掉 TOSEC 里那 960 条中文汉化条目。

#### B.3.4 TOSEC 覆盖 No-Intro 不覆盖的内容

| 内容 | TOSEC | No-Intro |
|---|---|---|
| **翻译版 `[tr xx]`** | ✅ 单个 NES DAT 就有 **5637 条**（其中 `[tr zh]` 722 条） | ❌ |
| Hack / 加 intro / 改图 `[h]` | ✅ 13404 条 | ❌ |
| Crack / Trainer / Fixed `[cr][t][f]` | ✅ | ❌ |
| Alternate dump `[a]` | ✅ 981 条 | ❌（只保留「最佳可得副本」） |
| Bad dump `[b]` 作为独立条目 | ✅ 4578 条 | 仅作 `[b]` 标记 |
| Over/Under dump、Virus | ✅ | ❌ |
| PD / Freeware / Shareware 分类 | ✅ | ❌ |
| 单一权威副本 + 严格双人验证 | ❌ | ✅ `status="verified"` |
| SHA256 | ❌ | ✅ |
| Header skipper / 无头哈希 | ❌ | ✅ |

> **一句话**：**No-Intro 追求「每个 ROM 的唯一正确原始副本」；TOSEC 追求「某平台上曾经存在过的每一个文件变体」。**
> **对汉化库而言，TOSEC 才是主力数据源。**

---

### B.4 libretro-database 与 RetroArch 的实现

来源：[libretro-database](https://github.com/libretro/libretro-database)、[RetroArch](https://github.com/libretro/RetroArch)、[libretro-super `libretro-build-database.sh`](https://github.com/libretro/libretro-super/blob/master/libretro-build-database.sh)

#### B.4.1 仓库结构与「主键」选择

libretro-database README 明确定义了主键规则（原文）：
> "The key field for matching varies by console typical file size (i.e. original media type).
> - **CRC checksum** for systems with smaller file sizes…
> - **Serial Number** for larger files like disc-based games, to avoid computing checksums on large files. Found within the ROM file. **The serial is not metadata but encoded within the game's binary data**, which is scanned (in applicable cases) as a byte array by RetroArch.
> CRC and serial also serve as RetroArch's primary index."

**【实测】** 统计 `libretro-build-database.sh` 中的键分配：

```bash
curl -sL https://raw.githubusercontent.com/libretro/libretro-super/master/libretro-build-database.sh \
  | grep -oE '"rom\.[a-z]+"' | sort | uniq -c
#  132 "rom.crc"
#    1 "rom.name"      (Lutro)
#   12 "rom.serial"
```

**使用 serial 作主键的 12 个系统（全量）**：PlayStation、PlayStation 2、PlayStation 3、PlayStation Portable、PlayStation Vita、GameCube、Wii、Dreamcast、Saturn、Mega-CD/Sega CD、Naomi、Naomi 2。

> ⚠ 注意：**CD-i、Neo Geo CD、PC Engine CD、3DO、Xbox 虽然是光盘平台，却仍用 `rom.crc`。**

#### B.4.2 目录职责

| 目录 | 内容 |
|---|---|
| `metadat/no-intro/` | No-Intro 上游批量导入（非光盘系统） |
| `metadat/redump/` | Redump 上游批量导入（光盘系统） |
| `metadat/tosec/` | TOSEC 导入（**优先级低于前两者**，作为补充） |
| `metadat/mame/`、`mame-split/`、`mame-nonmerged/`、`mame-member/`、`fbneo-*` | 街机 |
| **`metadat/hacks/`** | **「Data for modified (or 'hacked') versions of commercially released games」**（详见 C 章） |
| **`metadat/libretro-dats/`** | **「Currently includes fan translations of SNES games」**（详见 C 章） |
| `metadat/headered/` | Lynx / A7800 的 headered 变体 |
| `metadat/serial/` | **CRC → serial 的元数据叠加层**（卡带系统） |
| `metadat/homebrew/`、`lost-level-archive/` | 自制/其他 |
| 纯元数据叠加 | `developer/` `publisher/` `genre/` `franchise/` `origin/` `releaseyear/` `releasemonth/` `maxusers/` `esrb/` `bbfc/` `elspa/` `analog/` `rumble/` `enhancement_hw/` `magazine/` `barcode/` `tgdb/` |
| `rdb/` | 编译产物 |
| `cursors/` | 保存的查询 |

**合并模型**：`c_converter` 以「匹配键」把多个 DAT 合并成一份 RDB，**后指定的文件覆盖先前的同键记录**。这就是 `metadat/serial/` 这类「只有 crc + serial」的叠加 DAT 能给主记录附加序列号的原理。

**【实测】** `metadat/serial/Nintendo - Game Boy Advance.dat` 的实际形态：
```
game (
	comment "007 - Everything or Nothing (Japan)"
	serial "AGB-BJBJ-JPN"
	rom ( crc CAF2E99F )
)
```
> **【推断】** 注意这里 serial **只是挂在 CRC 上的元数据**，而不是可用于匹配的键 —— 因为 GBA 的主键是 `rom.crc`。所以 **RetroArch 无法用 ROM 内部的 GBA game code 去匹配一个 CRC 对不上的（如汉化版）GBA ROM**，尽管那个 game code 就明明白白写在文件 `0xAC` 处。这是本调研发现的一个重要能力缺口。

#### B.4.3 `.rdb` 二进制格式

规范：[`RetroArch/libretro-db/README.md`](https://github.com/libretro/RetroArch/blob/master/libretro-db/README.md)；实现：`libretro-db/libretrodb.c`

| 部分 | 内容 |
|---|---|
| Header（16 字节） | `char magic[8] = "RARCHDB\0"` + `uint64 metadata_offset`（**大端**） |
| Body | 连续的 MessagePack map，无分隔符，以一个 `0xC0`（msgpack NIL）哨兵结束 |
| Trailer | 位于 `metadata_offset` 的单键 map `{"count": N}` |
| Index（可选） | `{name, key_size, next, count}` + 排序的 `key‖uint64 offset` 对 |

**字段类型关键点**：`crc` / `md5` / `sha1` / `serial` 在 RDB 中是 **msgpack `bin`（原始字节）**，不是十六进制文本。`c_converter.c` 的 `rdb_mappings[]` 把 DAT 的 `rom.crc`/`rom.md5`/`rom.sha1` 以 HEX 解码存入，`serial`/`rom.serial` 以 BINARY（原始 ASCII）存入。

> ⭐ **【实测 / 重要】** 官方发布的 `.rdb` **不含索引**。`libretrodb_create_index` 只被 CLI 工具与单元测试调用，构建脚本从不调用它。
> 验证方式：`metadata_offset` 与文件大小之差恒为 10 字节（正好是 `{"count":N}` 的长度），没有索引区。
> **推论**：RetroArch 的每次扫描查询都是**线性游标遍历**，不是二分查找。
> **【推断】** `serial` 即便想建索引也不行 —— `libretrodb_create_index` 要求键长度统一，而序列号是变长 ASCII（`"SCUS-94179"` 10 字节 vs `"SCUS-94163-1"` 12 字节）。

#### B.4.4 RetroArch 扫描流程

入口：`tasks/task_database.c` 的 `task_database_iterate_playlist`。**CRC 还是 serial 由文件扩展名决定**（`extension_to_file_type`）：

| 扩展名 | 处理 |
|---|---|
| `7z` `zip` `apk` `zst` | CRC 查询（压缩包自身 CRC，从 ZIP 中央目录读取，**不解压**） |
| `cue` | `task_database_cue_get_serial` → SERIAL；**失败则回退 CRC** |
| `gdi` | GDI serial；回退 CRC |
| `wbfs` `rvz` `wia` | `intfstream_file_get_serial` → SERIAL（**无回退**） |
| `iso` | SERIAL_LOOKUP_SIZEHINT |
| `chd` | CHD serial；回退 CRC |
| `pbp` | PBP serial；回退 CRC |
| `lutro` | ITERATE_LUTRO |
| **其他一切** | **CRC 查询** |

查询语句（源码原样）：
```c
/* CRC：同时尝试内层文件 CRC 与压缩包自身 CRC */
snprintf(query, sizeof(query), "{crc:or(b\"%08lX\",b\"%08lX\")}",
         db_state->crc, db_state->archive_crc);

/* Serial：先 bin_to_hex_alloc 再拼 */
snprintf(query, query_len, "{'serial': b'%s'}", serial_buf);
```

扫描前会先跑 `{size:min(0)}` / `{size:max(0)}` 取得尺寸区间，跳过尺寸范围不可能包含该文件的数据库。

**扫描的严格 / 宽松模式**（[libretro 官方文档](https://docs.libretro.com/guides/roms-playlists-thumbnails/)原文）：
> "The default fully automatic scan is **strict**, the content CRC checksum or disc serial must match existing databases from the libretro-database."
> "Scan conditions could be relaxed ('**loose scan**'). If there is no database match, the content is still playable… but may lack thumbnails and do not appear in the Explore menu."

#### B.4.5 ⭐ RetroArch 扫描器**不做任何头部剥离**

这是本调研最反直觉的发现之一，且已交叉验证：

| 事实 | 证据 |
|---|---|
| RetroArch 扫描器中**不存在** iNES / SMC 头剥离逻辑 | 全树 grep `SKIP_HEADER`、`iNES`、`NES\x1a`、`4e45531a` 在扫描路径中零命中 |
| 仓库中唯一的头剥离代码在 `deps/rcheevos/src/rhash/hash_rom.c` | 那是 **RetroAchievements** 的哈希器，与播放列表扫描器完全无关 |
| libretro 的解法是**在数据库里存两份记录** | 见下 |
| libretro 的 DAT **完全不带** clrmamepro `header="…"` 属性 | 【实测】确认 |

**【实测】No-Intro NES DAT 的双记录**（libretro 镜像，version 2026.08.01）：

```bash
# .nes（headered） vs .unh（headerless） 条目数
7066 .nes"
7065 .unh"
```
两条记录 `name` 相同，因此无论用户手上是有头还是无头版本，播放列表标签一致。`metadat/headered/` 为 Lynx（262208 = 262144+64）与 A7800（131200 = 131072+128）提供同样的配对。

**⚠ 能力缺口 —— 【实测】确认**：

```bash
# SNES No-Intro DAT 的扩展名分布
4257 .sfc"
  11 .bin"     # 无任何 .smc 记录
```
**SNES 没有 headered 变体记录。带 512 字节 copier 头的 `.smc` 文件在 RetroArch 严格扫描下无法匹配。** FDS 同理（No-Intro FDS 尺寸 65500/131000，均为无头）。

#### B.4.6 各主机的 serial 抽取实现（`tasks/task_database_cue.c`）

先用 magic 表 `detect_system` 判定平台（源码原样）：

```c
static struct magic_entry MAGIC_NUMBERS[] = {
   { "Nintendo - GameCube",         "\xc2\x33\x9f\x3d", 0x00001c},
   { "Nintendo - GameCube",         "\xc2\x33\x9f\x3d", 0x000074}, /* RVZ, WIA */
   { "Nintendo - Wii",              "\x5d\x1c\x9e\xa3", 0x000018},
   { "Nintendo - Wii",              "\x5d\x1c\x9e\xa3", 0x000218}, /* WBFS */
   { "Nintendo - Wii",              "\x5d\x1c\x9e\xa3", 0x000070}, /* RVZ, WIA */
   { "Sega - Dreamcast",            "SEGA SEGAKATANA",  0x000010},
   { "Sega - Mega-CD - Sega CD",    "SEGADISCSYSTEM",   0x000010},
   { "Sega - Saturn",               "SEGA SEGASATURN",  0x000010},
   { "Sony - PlayStation",          "Sony Computer ",   0x0024f8},
   { "Sony - PlayStation 2",        "PLAYSTATION",      0x009320},
   { "Sony - PlayStation 2",        "PLAYSTATION",      0x008008}, /* PS2 DVD */
   { "Sony - PlayStation Portable", "PSP GAME",         0x008008},
   { "Philips - CD-i", "\xff\xff\xff\xff\xff\xff\xff\xff\xff\xff\x00", 0x000001},
   { NULL, NULL, 0}
};
```

| 函数 | 方法 |
|---|---|
| `detect_ps1_game` | **无固定偏移**。读前 60000 字节，逐字节暴力扫描 13 个前缀：`SCUS_ SLUS_ SLES_ SCED_ SLPS_ SLPM_ SCPS_ SLED_ SIPS_ ESPM_ SCES_ SLKA_ SCAJ_`。`_`→`-`，去掉 `SLUS_004.02` 式的 `.`，截断到 10 字符。失败时输出字面量 `"XXXXXXXXXX"` |
| `detect_ps2_game` | 同法，读前 600000 字节，约 45 个前缀。含损坏修复：`if (raw_game_id[8] == '\x12') raw_game_id[8] = '3';` |
| `detect_psp_game` | 读前 300000 字节，扫 `ULES- ULUS- ULJS- ULJM- UCES- UCUS- UCJS- NPEH- NPUH- NPJH-` 等 |
| `detect_pbp_game` | **真正的 PARAM.SFO 解析器**：验 `'\0PBP'`（`0x50425000`）→ 读 `0x08` 的 SFO 偏移 → 验 `'\0PSF'`（`0x46535000`）→ 读 `+0x08` key_table_start、`+0x0C` data_table_start、`+0x10` num_entries → 遍历 `0x14 + i*0x10` 的 16 字节索引项 → 找 `DISC_ID` 再找 `TITLE_ID` → `"SCUS94900"` 转成 `"SCUS-94900"` |
| `detect_gc_game` | 读偏移 `0` 的 4 字节；若是 `"RVZ"`/`"WIA"` 则改读 `0x0058`。拼成 `"DL-DOL-" + serial`，再按第 11 字节追加区域后缀（`E→-USA`，`J→-JPN`，`P`/`X→-EUR`，`D→-NOE`…） |
| `detect_wii_game` | 读偏移 `0` 的 6 字节；`"WBFS"` → 改读 `0x0200`；`"RVZ"`/`"WIA"` → 改读 `0x0058` |
| `detect_scd_game` | 固定偏移：serial `0x0193`（11 字节）、region `0x0200`。再按 `T-`/`G-`/`MK-` 前缀做 Redump 归一化 |
| `detect_sat_game` | serial `0x0030`（9 字节）、region `0x0050` |
| `detect_dc_game` | serial `0x0050`（10 字节）、region `0x0042` |

> ⭐ **【推断，证据充分】所有 Sega 偏移都是 IP.BIN 偏移 + 0x10**：

| 字段 | IP.BIN 偏移 | RetroArch 偏移 |
|---|---|---|
| Saturn magic `SEGA SEGASATURN` | `0x00` | `0x10` |
| Saturn serial | `0x20` | `0x30` |
| Saturn area codes | `0x40` | `0x50` |
| Dreamcast serial | `0x40` | `0x50` |
| Dreamcast area symbols | `0x32` | `0x42` |
| Mega Drive product code | `0x180`(+3) | `0x193` |
| Mega Drive region | `0x1F0` | `0x200` |

原因：`libretro-common/streams/chd_stream.c` 对 `MODE1_RAW`/`MODE2_RAW` 设 `frame_size = 2352`，而 `frame_offset` 恒为 0 —— 即 RetroArch 读的是**含 16 字节同步+扇区头的原始扇区**，检测偏移把这 16 字节吸收进去了。

#### B.4.7 「枚举一切变体」是 libretro 的通用策略

Redump PSX DAT 同时收录带盘号后缀与不带后缀的两条 serial 记录：
```
serial "SCUS-94163"    rom ( name "Final Fantasy VII (USA) (Disc 1).bin" ... serial "SCUS-94163" )
serial "SCUS-94163-0"  rom ( name "Final Fantasy VII (USA) (Disc 1).bin" ... serial "SCUS-94163-0" )
```
对应 `cue_append_multi_disc_suffix`，当文件名含 `(Disc N)`/`(Disk N)` 时追加 `-<N-1>`。

> **总结【推断】**：libretro 的哲学是「**把每一种变体都枚举成一条独立 DAT 记录**」，而不是在扫描时做归一化。这对扫描器很省事（无检测逻辑、无二次哈希），但**上游 DAT 缺哪种变体，就在哪种变体上失效**。

---

### B.5 ROM 管理器的 header skipper 机制

#### B.5.1 clrmamepro —— 数据驱动的 XML 检测器（已逐字核实）

规范：[XML driven header support](https://mamedev.emulab.it/clrmamepro/docs/xmlheaders.txt)（HTTP 200，14086 字节，3.90 起）。文件放在首次运行时自动创建的 `headers/` 子目录。

设计动机（原文）：
> "While thinking about a way to support headers, I quickly dropped the idea of a plugin system (**not to mention the security risks which generally arise with plugins**), Mike (Logiqx) came up with the idea of defining header detection rules in XML."

—— 与 RomCenter 的插件式架构形成直接对照。

求值语义（原文）：
> "A rule is 'fulfilled' if **all (logical AND)** tests of that rule succeed. Single rules are connected with a **logical OR**… **As soon as a rule is fulfilled, no more rules are tested.** If no rule can be applied successfully to the current file, the default values (start = 0, end = EOF) are used."

整体结构：
```xml
<?xml version="1.0"?>
<detector>
  <name>...</name>
  <author>...</author>
  <version>...</version>
  <rule ...>
    <test ...>
    ...
  </rule>
  <rule ...></rule>
</detector>
```

**`<rule>` 属性**：

| 属性 | 说明 |
|---|---|
| `start_offset` | 十六进制（最大 64 位），默认 `0` —— **这就是「跳过多少字节」** |
| `end_offset` | 十六进制，默认 `"EOF"`；`-` 前缀表示相对文件末尾（`end_offset="-800"` = 距末尾 0x800 = 2048 字节） |
| `operation` | `none` \| `bitswap` \| `byteswap` \| `wordswap` \| `wordbyteswap`，默认 `none` |

变换语义（原文）：
- `byteswap`：`01\|02` → `02\|01`，**要求文件大小为偶数**
- `wordswap`：`01\|02\|03\|04` → `04\|03\|02\|01`，**要求 filesize mod 4 = 0**
- `wordbyteswap`：`01\|02\|03\|04` → `03\|04\|01\|02`
- `bitswap`：「swaps higher with lower bits: 7 -> 0, 6 -> 1 etc.」

> ⭐ **无测试的规则恒为真** —— 规范原文：「`<rule start_offset="80" end_offset="EOF"/>` Always skips the first 0x80 bytes… **A rule without any tests is always 'fulfilled'.**」

**三类测试**（规范原文示例）：

| 类别 | 示例 | 属性 |
|---|---|---|
| **data** | `<data offset="1" value="415441524937383030" result="true"/>` | `offset`（默认 0，可为负或 `EOF`）、`value`（必填）、`result`（默认 true，可反转） |
| **boolean** | `<or\|xor\|and offset="10" mask="1f54" value="4154" result="true"/>` | mask 逐字节作用于读入数据后再比较；**mask 与 value 的字节长度必须相同** |
| **file** | `<file size="1000" result="true" operator="less"/>`、`<file size="PO2" result="false"/>` | `size` 必填（十六进制或 `"PO2"` 测 2 的幂）、`operator` ∈ `equal\|less\|greater`（默认 equal） |

> 注意：规范自身给出的 data 测试示例 `value="415441524937383030"` 正是 ASCII `"ATARI7800"` —— 即 Atari 7800 检测器的形态在官方规范里就是示例。

规范的完整工作示例（原文 Example 1）：
```xml
<rule start_offset="80" end_offset="EOF">
  <data offset="64" value="41435455414C2043" result="true"/>
  <file size="PO2" result="false"/>
</rule>
<rule start_offset="0" end_offset="EOF">
  <file size="PO2" result="true"/>
</rule>
```
> 语义：`(偏移 0x64 处是该字节串 AND 文件大小不是 2 的幂) OR (文件大小是 2 的幂)`，据此把起始偏移设为 `0x80` 或 `0x0`。

Example 3（**变换而非跳过**）：
```xml
<rule start_offset="0" end_offset="EOF" operation="wordswap">
  <data offset="0" value="504b0304" result="true"/>
</rule>
```

**DAT 中的绑定**：cmpro 语法写 `header nes.xml`；Logiqx XML 写 `<clrmamepro header="No-Intro_NES.xml"/>`。

规范还有一条易被忽视但很重要的约定（原文）：
> "a datfile which uses files with headers should list **either no size information at all (size "-") or should use the size value of that data block (without the header)**."

**一个真实的、可核实的检测器** —— 来自 ckmame（nih-at），展示 N64 检测器是**变换**而非跳过：
```xml
<?xml version="1.0"?>
<detector>
  <name>Nintendo 64 de-interleave</name>
  <author>Dieter Baron</author>
  <version>20070416</version>
  <rule operation="byteswap">
    <and offset="0" value="37804012" mask="FFFFFEFF" />
  </rule>
</detector>
```

#### B.5.1b ⭐ No-Intro 官方 skipper 原文（本次调研直接下载核实）

官方 skipper 可从 `https://datomatic.no-intro.org/stuff/` 直接下载（**这些静态文件不受 DAT-o-MATIC 的封禁机制影响**）：

| URL | 内含文件 | 实测 HTTP |
|---|---|---|
| `https://datomatic.no-intro.org/stuff/header_nes.zip` | `No-Intro_NES.xml` | 200 |
| `https://datomatic.no-intro.org/stuff/header_fds.zip` | `No-Intro_FDS.xml` | 200 |
| `https://datomatic.no-intro.org/stuff/header_lynx.zip` | `No-Intro_LNX.xml` | 200 |
| `https://datomatic.no-intro.org/stuff/header_a7800.zip` | `No-Intro_A7800.xml` | 200 |

**`No-Intro_NES.xml` 全文（实测解压所得）**：
```xml
<?xml version="1.0"?>

<detector>

  <name>No-Intro NES Dat iNES Header Skipper</name>
  <author>Yakushi~Kabuto</author>
  <version>20070321</version>

  <rule start_offset="10">
    <data offset="0" value="4E4553"/>
  </rule>

</detector>
```

**`No-Intro_FDS.xml`**：
```xml
<detector>
  <name>No-Intro FDS Dat FWnes Header Skipper</name>
  <author>Yakushi~Kabuto</author><version>20070421</version>
  <rule start_offset="10"><data offset="0" value="464453"/></rule>   <!-- "FDS" -->
</detector>
```

**`No-Intro_LNX.xml`**：
```xml
<detector>
  <name>No-Intro Lynx Dat LNX Header Skipper</name>
  <author>Yakushi~Kabuto</author><version>20070408</version>
  <rule start_offset="40"><data offset="0" value="4C594E58"/></rule> <!-- "LYNX" -->
</detector>
```

**`No-Intro_A7800.xml`**：
```xml
<detector>
  <name>No-Intro Atari 7800 Dat Header Skipper</name>
  <author>Connie</author><version>20130123</version>
    <rule start_offset="80" end_offset="EOF" operation="none">
      <data offset="1"  value="415441524937383030" result="true"/>   <!-- "ATARI7800" -->
      <data offset="60" value="0000000041435455414C20434152542044415441205354415254532048455245" result="true"/>
    </rule>                                                          <!-- "ACTUAL CART DATA STARTS HERE" -->
</detector>
```

> ⭐ **两个容易写错的关键点**：
> 1. **`start_offset` 是十六进制**。`start_offset="10"` = **16 字节**（不是 10）；`"40"` = **64 字节**（Lynx）；`"80"` = **128 字节**（A7800）。这是最常见的实现 bug。
> 2. **No-Intro 的 NES 检测器只比对 3 个字节 `4E 45 53`（"NES"），不是 4 个** —— 它**不检查** iNES magic 的第 4 字节 `0x1A`。这比 igir / RomVault 的 `4E45531A` 更宽松。

**NES iNES 16 字节头被跳过的完整机制**：
1. DAT header 中出现 `<clrmamepro header="No-Intro_NES.xml"/>`
2. ROM 管理器在 `headers/` 中加载该 XML
3. 对文件求值唯一一条 rule：读偏移 0 起 3 字节，比较是否为 `4E 45 53`
4. 命中 → rule fulfilled → 采用 `start_offset = 0x10`（= 16）、`end_offset` 默认 `EOF`
5. CRC32/MD5/SHA1 只对 `[0x10, EOF)` 计算；报告的 size 也是 `filesize − 16`
6. 未命中（本就是无头 ROM）→ 无 rule 满足 → 回退默认 `start=0, end=EOF`，整文件计算

> **因此同一个 skipper 同时兼容有头与无头文件。**

**MAME 不消费这套机制** —— `hash/softwarelist.dtd` 中**没有** `detector` / `header` / `skipper` 元素，`<rom>` 的属性只有 `name size crc sha1 offset value status loadflag`。MAME 用**逐芯片 `<dataarea>` 哈希**绕开了这个问题。真实条目（`hash/nes.xml`）：
```xml
<software name="smb">
  <description>Super Mario Bros. (Europe, rev. A)</description>
  ...
  <part name="cart" interface="nes_cart">
    <dataarea name="prg" size="32768">
```
> **MAME 的模型里根本不存在 iNES 头 —— PRG 与 CHR 是分开的 dataarea。**

#### B.5.2 RomVault（RVWorld）—— 硬编码 + 双哈希

来源：[RomVault/RVWorld](https://github.com/RomVault/RVWorld)

**RomVault 完全不解析 clrmamepro XML。** 检测表硬编码在 `FileScanner/FileHeaders.cs`，DAT 里的 `header="…"` 字符串**只当查表的键**：
```csharp
new Detector(HeaderFileType.NES,   16,  16, "No-Intro_NES.xml",   new Data(0, new byte[]{0x4E,0x45,0x53,0x1A})),
new Detector(HeaderFileType.NES,   16,  16, "nes.xml",            new Data(0, new byte[]{0x4E,0x45,0x53,0x1A})),
new Detector(HeaderFileType.A7800,128, 128, "No-Intro_A7800.xml", new Data(1, /* "ATARI7800" */ ...)),
new Detector(HeaderFileType.Lynx,  64,  64, "No-Intro_LNX.xml",   new Data(0, new byte[]{0x4C,0x59,0x4E,0x58})),
new Detector(HeaderFileType.SNES, 512, 512, "snes.xml",           new Data(22, /* "SUPERUFO" */ ...)),
```
跳过长度：NES 16、FDS 16、A7800 128、Lynx 64、PCE 512、SNES 512、SPC 256、PSID 118/124。

⭐ **AltHeader 概念 —— RomVault 同时保存两套哈希**（`FileScanner/ScannedFile.cs`）：
```csharp
public ulong? Size;    public byte[] CRC;    public byte[] SHA1;    public byte[] MD5;
public ulong? AltSize; public byte[] AltCRC; public byte[] AltSHA1; public byte[] AltMD5;
```
`FileScanner/FileScan.cs` 顶部的文档表：
```
 * DeepScan | Has a Header | size | crc | sha1 | md5 | altsize | altcrc | altsha1 | altmd5
 *     0    |       1      |  H   |  H  |      |     |    V    |    V   |         |
 *     1    |       1      |  HV  |  HV |  V   |  V  |    V    |    V   |    V    |    V
```
（H = 含头的整文件，V = 去头后的数据）。两套哈希**一次读取内完成**：读 512 字节 → `FileHeaderReader.GetType(...)` 得出 `actualHeaderSize` → 只把头喂给主哈希器 → 剩余部分同时喂给两套。

匹配（`RomVaultCore/Scanner/Compare.cs`）：
```csharp
if (CompareHash(dbFile, testFile))    { altMatch = false; return true; }
if (CompareAltHash(dbFile, testFile)) { altMatch = true;  return true; }
```
即 DAT 里的 CRC 会去和扫描文件的 **`AltCRC`**（去头哈希）比对 —— 这正是 No-Intro 的场景。

#### B.5.3 RomCenter —— 编译型插件（已核实）

来源：[RomCenter plugin wiki](https://www.romcenter.com/wiki/doku.php?id=plugin)（原文，含原始拼写错误）：
> "Plugins are used by romcenter to **calulate** the file crc and size. Its main work is to **recognize the rom format, skip header datas and process roms datas so that two roms of a same game in different format return only one size and crc**… Using plugins, you only need to store **one crc and one size** in the datafile to recognize all formats."

绑定写在 **Logiqx XML** 的 DAT 头里：
```xml
<romcenter rommode="unmerged" biosmode="split" samplemode="merged"
           lockbiosmode="no" lockrommode="yes" locksamplemode="no" plugin="nes.dll"/>
```
DLL 放在 `romcenter/plugin`。未指定插件时 RomCenter 会尝试自动检测并**建议**一个。

官方插件源码：[ebolefeysot/RomcenterPlugins](https://github.com/ebolefeysot/RomcenterPlugins)。ABI（`Goodxxx/RCPlugins.h`，接口版本字符串 `"2.50"`）：
```c
RCPLUGINS_API char *GetSignature(char *name, char *zipcrc, char **format,
                                 __int64 *size, char **comment, char **errmsg);
```

**【实测】** 真实插件实现（`Goodxxx/fmt/`）：
```c
// nes.cpp
if (pDynMem[0]=='N' && pDynMem[1]=='E' && pDynMem[2]=='S') { pDataSize -= 0x10; pData = &pDynMem[0x10]; }

// lynx.cpp
if (iImageSize > 64) { pDataSize -= 64; pData = &pDynMem[64]; }

// pce.cpp（注释："SMS, PCE, GBX shared. SNES could use this but does not"）
if ((iImageSize > 0x200) && (iImageSize % 0x400 == 0x200)) { pDataSize -= 0x200; pData = &pDynMem[0x200]; }

// psid.cpp —— 头长度从文件自身读出
if (!memcmp(pDynMem,"PSID",4)) tmp = (pDynMem[6]<<8) | pDynMem[7];   // 0x76 v1, 0x7C v2
if (tmp < iImageSize) { pDataSize = iImageSize - tmp; pData = &pDynMem[tmp]; }
```
`snes.cpp` 先检测 FFE copier 签名 `pDynMem[8]==0xAA && pDynMem[9]==0xBB` 再跳 `0x200`。
`a7800.cpp` 最有意思 —— 它的生效分支**不跳过**，而是在匹配偏移 1 处的 `"ATARI7800"` 后**把 `pDynMem[11..42]` 清零**，即就地归一化易变的头部字段（跳 128 字节的变体被 `#if 0` 注释掉了）。

> ⭐ **这正是「代码优于 XML」的地方**：`psid.cpp` 从文件里**读出**跳过长度，而 clrmamepro 的固定 `start_offset` 表达不了这一点。

#### B.5.4 igir —— 现代实现（同时算两套哈希）

来源：[igir ROM headers 文档](https://igir.io/roms/headers/)、[`src/models/files/romHeader.ts`](https://github.com/emmercm/igir/blob/main/src/models/files/romHeader.ts)

igir 的检测表**直接以 No-Intro skipper 文件名为键**，并给出了 `(headerOffsetBytes, headerValueHex, dataOffset, headeredExt, headerlessExt)`：

| Key | 检测偏移 | 检测值（hex） | 跳过字节 | 扩展名 |
|---|---|---|---|---|
| `No-Intro_A7800.xml` | 1 | `415441524937383030`（"ATARI7800"） | 128 | `.a78` |
| `No-Intro_LNX.xml` | 0 | `4C594E58`（"LYNX"） | 64 | `.lnx` / `.lyx` |
| `No-Intro_NES.xml` | 0 | `4E45531A`（"NES\x1A"） | 16 | `.nes` |
| `No-Intro_FDS.xml` | 0 | `464453`（"FDS"） | 16 | `.fds` |
| `SMC` | 3 | `00` × 509 | 512 | `.smc` / `.sfc` |
| `SMC_GAME_DOCTOR_1` | 0 | `00014D4520444F43544F522053462033` | 512 | `.smc` / `.sfc` |
| `SMC_GAME_DOCTOR_2` | 0 | `47414D4520444F43544F522053462033`（"GAME DOCTOR SF 3"） | 512 | `.smc` / `.sfc` |

igir 文档原文：
> "will use this detected header information to compute both '**headered**' and '**headerless**' checksums of ROMs and use both of those to match against DAT files."
> "Many DAT groups expressly only include the size and checksum information for the **headerless** ROM."

#### B.5.5 四种架构对比

| 工具 | 规则存放位置 | 存 headered 哈希 | 存 headerless 哈希 | 支持字节变换 |
|---|---|---|---|---|
| clrmamepro | 数据驱动 XML（`headers/`） | 否 | 是 | bitswap / byteswap / wordswap / wordbyteswap |
| RomVault | **硬编码 C#**，以 XML 文件名为键 | **是** | **是**（`Alt*`） | 无 |
| RomCenter | **编译型插件 DLL** | 否 | 是（单一签名） | 任意代码 |
| igir | 硬编码 TS，以 XML 文件名为键 | **是** | **是** | 无 |
| **RetroArch / libretro** | **不存在 —— 完全没有 skipper** | **是** | **是** | 无 |

**共同算法**：读文件前缀 → 在某偏移比对 magic → 跳过 N 字节 → 对**头之后**的数据算 CRC32/MD5/SHA1 → 与 DAT 比对。No-Intro 的 NES/FDS/Lynx/A7800 DAT 记录的是**无头**的哈希与尺寸，所以整文件 CRC 永远匹配不上。

#### B.5.6 从 ZIP 中直接取 CRC（不解压）

[PKWARE APPNOTE 6.3.9](https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.9.TXT) 第 4.3.12 节，central file header 结构：
```
central file header signature   4 bytes  (0x02014b50)
version made by                 2 bytes
version needed to extract       2 bytes
general purpose bit flag        2 bytes
compression method              2 bytes
last mod file time              2 bytes
last mod file date              2 bytes
crc-32                          4 bytes      <-- 偏移 +16
compressed size                 4 bytes
uncompressed size               4 bytes      <-- 偏移 +24
```
> RetroArch 的 `file_archive_get_file_crc32_and_size` 正是从中央目录读取，**不解压**。
> **【推断】** 但要注意：这个 CRC32 是**未压缩数据整体**的 CRC —— 对 NES 而言仍然是「含 iNES 头」的 CRC。所以匹配 headerless DAT 时仍必须解压。

### B.6 Logiqx XML DAT 格式（datafile.dtd v1.5）—— 三库共用的历史基础

> ⚠ `http://www.logiqx.com` **整站已下线**（DNS 无响应）。`datafile.dtd` 尚有 [Wayback 归档](https://web.archive.org/web/2018id_/http://www.logiqx.com/Dats/datafile.dtd)与 [GitHub 镜像](https://github.com/Logiqx/logiqx-www/blob/master/Dats/datafile.dtd)；**`detector.dtd` 从未被归档，已彻底不可得** —— 唯一权威规范是 clrmamepro 作者的 `xmlheaders.txt`（见 B.5.1）。

DTD v1.5（`$Date: 2008/10/28 21:39:16 $`）核心结构：
```dtd
<!ELEMENT datafile (header?, game+)>
  <!ELEMENT header (name, description, category?, version, date?, author, email?, homepage?, url?, comment?, clrmamepro?, romcenter?)>
    <!ELEMENT clrmamepro EMPTY>
      <!ATTLIST clrmamepro header CDATA #IMPLIED>
      <!ATTLIST clrmamepro forcemerging (none|split|full) "split">
      <!ATTLIST clrmamepro forcenodump (obsolete|required|ignore) "obsolete">
      <!ATTLIST clrmamepro forcepacking (zip|unzip) "zip">
    <!ELEMENT romcenter EMPTY>
      <!ATTLIST romcenter plugin CDATA #IMPLIED>
      <!ATTLIST romcenter rommode (merged|split|unmerged) "split">
  <!ELEMENT game (comment*, description, year?, manufacturer?, release*, biosset*, rom*, disk*, sample*, archive*)>
    <!ATTLIST game name CDATA #REQUIRED>
    <!ATTLIST game cloneof CDATA #IMPLIED>
    <!ATTLIST game romof CDATA #IMPLIED>
    <!ATTLIST game sampleof CDATA #IMPLIED>
    <!ELEMENT rom EMPTY>
      <!ATTLIST rom name CDATA #REQUIRED>
      <!ATTLIST rom size CDATA #REQUIRED>
      <!ATTLIST rom crc CDATA #IMPLIED>
      <!ATTLIST rom sha1 CDATA #IMPLIED>
      <!ATTLIST rom md5 CDATA #IMPLIED>
      <!ATTLIST rom merge CDATA #IMPLIED>
      <!ATTLIST rom status (baddump|nodump|good|verified) "good">
      <!ATTLIST rom date CDATA #IMPLIED>
    <!ELEMENT disk EMPTY>
      <!ATTLIST disk name CDATA #REQUIRED>
      <!ATTLIST disk sha1 CDATA #IMPLIED>
      <!ATTLIST disk md5 CDATA #IMPLIED>
```

> ⚠ **Logiqx DTD v1.5 里根本没有 `sha256` 和 `serial`；`category` 只在 `<header>` 下合法。**
> 但 Redump 把 `<category>` 放进 `<game>`，TOSEC 放在 `<header>`（合规），No-Intro 干脆改用自有 XSD。
> **结论：「Logiqx DAT」在实践中是一族方言，不是一个严格标准。解析器必须宽容。**

---

## 第 C 章 · 汉化版 / 魔改版的识别

> 这是本调研最关键的一章。用户库中以汉化版为主，文件名不可靠、哈希与官方 DAT 不匹配。
> **核心结论先行**：调研过程中通过**实测**推翻了一个常见判断 ——「不存在带哈希的汉化 ROM 数据库」是**错的**。**TOSEC 用 ISO 639-1 语言码把汉化版正式收录为 `[tr zh]`，并给出完整的 CRC32/MD5/SHA1**；且这些数据可通过免费 API 直接查询。详见 [C.4](#c4-可用的汉化-rom-哈希数据源实测)。

### C.1 补丁格式：哪些格式自带原版 ROM 的可识别特征

这是「拿到补丁文件能否反推原版」的问题。答案差异极大。

#### C.1.1 IPS —— 完全不含任何源信息

来源：[Zerosoft IPS 规范](https://zerosoft.zophar.net/ips.php)、参考实现 [Flips `libips.cpp`](https://github.com/Alcaro/Flips/blob/master/libips.cpp)

| 偏移 | 长度 | 字段 |
|---|---|---|
| 0 | 5 | Magic `"PATCH"`（`50 41 54 43 48`） |
| 5 | … | 记录，重复 |
| … | 3 | 终止符 `"EOF"`（`45 4F 46`） |

普通记录（**多字节字段全为 big-endian**）：offset (3) + size (2) + data (size)。
RLE 记录（`size == 0` 时）：offset (3) + `0x0000` (2) + rle_size (2) + value (1)。

> ⭐ **IPS 不存储原始 ROM 的任何哈希或校验和 —— 一个字节都没有。** Zerosoft 规范无校验字段；Flips 的 `ips_apply_study()` 除边界检查外不对输入做任何验证。
> **这是本问题最重要的一条事实：仅凭一个 IPS 补丁文件，无法归因到任何一份基准 ROM。**

其他要点：
- **16 MB 上限**：24 位偏移使输出上限为 `0xFFFFFF`。Flips 显式强制：`if (targetlen > 16777216) return ips_16MB;`
- **偏移 `0x454F46`（4,542,278）不可表示** —— 落在该处的记录会被读成 `"EOF"` 终止符
- Flips 支持一个未文档化的扩展：`"EOF"` 之后可跟 3 字节的截断长度

#### C.1.2 UPS —— 存储源 ROM 的 CRC32 ✅

参考实现：[Flips `libups.cpp`](https://github.com/Alcaro/Flips/blob/master/libups.cpp)

```
"UPS1"                    4 字节 magic
varint  input-size
varint  output-size
repeat { varint skip ; XOR 字节，以 0x00 终止 }   直到 offset == len-12
uint32  input  CRC32      ┐ 12 字节 footer，
uint32  output CRC32      │ little-endian
uint32  patch  CRC32      ┘（patch CRC 覆盖 patch.len - 4）
```

源码原文：
```c
uint32_t crc_in_expected    = read32(patchat);
uint32_t crc_out_expected   = read32(patchat+4);
uint32_t crc_patch_expected = read32(patchat+8);
```

> ⭐ **一个 UPS 补丁本身就能告诉你基准 ROM 的 CRC32 与精确字节大小。**
> 注意 UPS 是**双向**的 —— Flips 允许反向应用（交换两个 CRC），所以一个未知 UPS 给出的是**两个候选 CRC32**，不是一个。

#### C.1.3 BPS —— 存储源 ROM 的 CRC32 ✅

规范：[byuu `bps_spec.md`](https://github.com/Alcaro/Flips/blob/master/bps_spec.md)（public domain）

```
string "BPS1"
number source-size
number target-size
number metadata-size
string metadata[metadata-size]
repeat {
  number action | ((length - 1) << 2)
  action 0: SourceRead { }
  action 1: TargetRead { byte[] length }
  action 2: SourceCopy { number negative | (abs(offset) << 1) }
  action 3: TargetCopy { number negative | (abs(offset) << 1) }
}
uint32 source-checksum
uint32 target-checksum
uint32 patch-checksum
```

规范原文：
> "The source checksum verifies that the input file is correct… Note that **checksums are stored in CRC32 format**."
> 循环终止于 `offset() >= size() - 12`，"Where 12 is the number of bytes in the patch footer."

> **delta 与 linear 的区别不影响可识别性** —— 两者文件结构相同，都带 footer。

#### C.1.4 xdelta / VCDIFF —— 标准不含源哈希；新版有 BLAKE3

[RFC 3284](https://datatracker.ietf.org/doc/html/rfc3284)：magic `D6 C3 C4 00`，**未定义任何源或目标的校验和**。

xdelta3 的非标准扩展（[`xdelta3.c`](https://github.com/jmacd/xdelta/blob/master/xdelta3/xdelta3.c)）：
```c
#define VCD_ADLER32 (1U << 2) /* has adler32 checksum */
```
> ⚠ **重要细节：`VCD_ADLER32` 是解码后**目标**窗口的 adler32，不是源的。** 它只能在应用之后**间接**发现源不对。错误消息原文：*"target window checksum mismatch: the supplied source likely does not match the one used to create this patch"*。
> **无法从经典 xdelta3 补丁中恢复源哈希。**

**新进展**：当前 xdelta3 master 的 **"armor mode" 默认开启**（[docs](https://jmacd.github.io/xdelta/armor/)），把**源与目标各自的 BLAKE3-256 摘要**写进 VCDIFF application header，编码为 `name#<64 hex>`。文档原文：*"Armor is on by default… the source is verified up front, before applying the delta."*
> **【推断】** 现存的 ROM hack xdelta 补丁几乎都早于此特性，不会带 armor。把它当作**前瞻能力**，而非识别现有补丁的手段。

#### C.1.5 APS —— 两个同名不同物的格式，都带源身份信息

**APS (N64)**（[UniPatcher wiki](https://github.com/btimofeev/UniPatcher/wiki/APS-(N64))）：

| 偏移 | 长度 | 字段 |
|---|---|---|
| 0–4 | 5 | Magic `"APS10"` |
| 5 | 1 | Patch type：0=simple，1=N64 专用 |
| 6 | 1 | 编码方式 |
| 7–56 | 50 | 描述，空格补白 |
| 57 | 1 | 原镜像格式（0=Doctor V64，1=CD64/Z64/Wc/SP） |
| **58–59** | 2 | **原镜像的 Cart ID**（大端） |
| **60** | 1 | **原镜像的 Country code** |
| **61–68** | 8 | **原镜像的 CRC** |
| 69–73 | 5 | 填充 |
| 74–77 | 4 | 目标镜像大小 |

> 注意：58–68 是**从 N64 卡带头里抄出来的**（`0x10`–`0x17` 的 check code、cart ID、country 字节），**不是文件哈希**。所以 APS/N64 能定位**游戏与区域**及 ROM 内部 CRC，但不能定位到确切文件。
> **【推断】** 实践中这几乎同样好用，因为 No-Intro 的 N64 条目与这些内部 CRC 是 1:1 对应的。

**APS (GBA)** —— **这是一个真正的分块哈希格式**（[RomPatcher.js `RomPatcher.format.aps_gba.js`](https://github.com/marcrobledo/RomPatcher.js/blob/master/rom-patcher-js/modules/RomPatcher.format.aps_gba.js)）：
```js
const APS_GBA_MAGIC='APS1';
const APS_GBA_BLOCK_SIZE=0x010000; //64Kb
const APS_GBA_RECORD_SIZE=4 + 2 + 2 + APS_GBA_BLOCK_SIZE;
```
头部：`"APS1"`(4) + 原始大小(4) + 修改后大小(4) = 12 字节。
每条记录：offset(4) + **源 64 KB 块的 CRC16**(2) + 目标块 CRC16(2) + 64 KB 的 `source XOR target`。
验证器同时检查源文件大小**和每一个源块的 CRC16**。

> ⭐ **APS/GBA 给出源大小 + 基准 ROM 的 64 KB 粒度稀疏指纹。这是 ROM 补丁领域唯一在用的真正的分段哈希方案。**

#### C.1.6 PPF 3.0 —— 1024 字节原始数据，非哈希

来源：[Paradox `PPF3.txt`](https://github.com/meunierd/ppf/blob/master/ppfdev/PPF3.txt)

| 偏移 | 长度 | 字段 |
|---|---|---|
| 00–04 | 5 | Magic `"PPF30"` |
| 05 | 1 | 编码方式（0=PPF1.0，1=PPF2.0，2=PPF3.0） |
| 06–55 | 50 | 描述 |
| 56 | 1 | Imagetype：`0x00`=BIN，`0x01`=GI |
| 57 | 1 | Blockcheck（0=关，1=开） |
| 58 | 1 | 是否有 undo 数据 |
| 59 | 1 | Dummy |
| **60–1083** | **1024** | **从原镜像 `0x9320`（BIN）/ `0x80A0`（GI）抄来的原始字节** |
| 1084–… | | 补丁记录（**64 位 little-endian 偏移**） |

> **【推断】** `0x9320` 落在 PSX 光盘的 ISO9660 卷描述符/系统区，所以这段字节在不同游戏间有区分度。但这是**比对块**，只在你已有候选镜像时可用，**不能作为查询键**。

#### C.1.7 RUP / NINJA2 —— 最丰富，存源 MD5 ✅

来源：[RomPatcher.js `RomPatcher.format.rup.js`](https://github.com/marcrobledo/RomPatcher.js/blob/master/rom-patcher-js/modules/RomPatcher.format.rup.js)（其规范为 romhacking.net doc 288）

```js
const RUP_MAGIC='NINJA2';
const RUP_ROM_TYPES=['raw','nes','fds','snes','n64','gb','sms','mega','pce','lynx'];
```
每个文件记录携带：`fileName`、`romType`、`sourceFileSize`、`targetFileSize`、**`sourceMD5`**、**`targetMD5`**、溢出模式。补丁级元数据还有 author / version / title / genre / **language** / date / web / description。

> **【推断】** 这是唯一一种「补丁文件本身就给出基准 ROM MD5 + 补丁后 MD5 + 声明的语言」的常见格式。

#### C.1.8 补丁格式对比总表

| 格式 | Magic | 存源哈希？ | 存源大小？ | 不持有原版能否识别基准 ROM？ |
|---|---|---|---|---|
| **IPS** | `PATCH` | ❌ 无 | ❌ | **否** |
| **UPS** | `UPS1` | ✅ CRC32（LE，`len-12`） | ✅ varint | **是** —— CRC32 + 大小 |
| **BPS** | `BPS1` | ✅ CRC32（LE，`len-12`） | ✅ varint | **是** —— CRC32 + 大小 |
| **APS (N64)** | `APS10` | ⚠ cart ID + country + 8 字节**内部** ROM CRC | 仅目标大小 | **大致可以** —— 定位游戏+区域，非确切文件 |
| **APS (GBA)** | `APS1` | ⚠ 每 64 KB 一个 CRC16 | ✅ 4 字节 | **部分可以** —— 稀疏块指纹 |
| **PPF 3.0** | `PPF30` | ⚠ `0x9320` 起的 1024 原始字节 | ❌ | **仅可比对**，不可查询 |
| **RUP / NINJA2** | `NINJA2` | ✅ **MD5**（源与目标） | ✅ | **是** —— 最强 |
| **xdelta3 / VCDIFF** | `D6 C3 C4 00` | ❌（RFC 3284）；✅ BLAKE3-256（新版 armor 默认开） | 窗口头中有 | **仅当带 armor**（新特性） |

> ⚠ **对中文场景的现实意义**：IPS 在 FC/NES 汉化中占绝对主导，而 IPS 恰恰是**唯一完全不可归因**的格式。

---

### C.2 补丁会改动哪些区域？内部头会不会被改？

#### C.2.1 内部头会被改写，且校验和**必须**重算 —— 有工具作证

**Game Boy** —— [Pan Docs](https://gbdev.io/pandocs/The_Cartridge_Header.html)：
`0x014D` 的 header checksum 覆盖 `0x0134`–`0x014C`（含 Title），且
> "The boot ROM verifies this checksum. If the byte at `$014D` does not match… the boot ROM will lock up and the program in the cartridge **won't run**."

> **结论【事实推导】**：任何改写标题的 GB/GBC 汉化（很常见）**必须**修复 `0x014D`，否则游戏无法启动。**这是被文档强制的头部修改。**

**GBA** —— devkitPro 的 [`gbafix`](https://github.com/devkitPro/gba-tools/blob/master/src/gbafix.c) 是标准修复工具：
```c
char HeaderComplement() {
	int n; char c = 0;
	char *p = (char *)&header + 0xA0;
	for (n=0; n<0xBD-0xA0; n++) { c += *p++; }
	return -(0x19+c);
}
...
	header.complement = 0;
	header.checksum = 0;	// must be 0
	header.complement = HeaderComplement();
```
其命令行选项**正是汉化者要改的字段**：
```
-t[<title>]     Patch title. Stripped filename if none given.
-c<game_code>   Patch game code (four characters)
-m<maker_code>  Patch maker code (two characters)
-r<version>     Patch game version (number)
-p              Pad to next exact power of 2. No minimum size!
```

> ⭐ **`gbafix` 的存在、它是 devkitPro 标准工具、且同时提供 `-t`/`-c`/`-m` 与强制重算 complement —— 这是「GBA 的 title 与 game code 被改写是常规操作」的直接证据。**
> 且 `-p` 会**把文件补齐到下一个 2 的幂**，即**改变文件大小**。

**SNES** —— [SnesLab SNES ROM Header](https://sneslab.net/wiki/SNES_ROM_Header)：`$FFD7` 是 ROM Size、`$FFDE` 是 Check Sum。
> **结论【事实推导】**：任何 SNES ROM 扩容都会改变 `$FFD7` 并使 `$FFDE` 失效，因此**扩容型汉化必然改写内部头**。

#### C.2.2 ROM 扩容 —— 文件大小不是不变量

- [FuSoYa's Lunar Expand](https://fusoya.eludevisibility.org/le/index.html) 就是为把 SNES ROM 扩到 32 Mbit 以上而生，产出布局包括 **48 Mbit ExHiROM / 48 Mbit ExLoROM / 64 Mbit ExHiROM / 64 Mbit ExLoROM** 及 SA-1 变体。
- `gbafix -p` 是**直接验证过**的、会改变文件大小的标准工具选项。
- BPS/UPS 都记录 `target-size`，且**可以大于 `source-size`**。BPS 的 `TargetRead` 动作就是「往输出里塞全新字节」。

> ⭐ **实测佐证**：libretro `metadat/hacks/Nintendo - Nintendo Entertainment System.dat` 中
> `Metroid - Rogue Dawn` 的 size = **786448**（= 768KB + 16），而原版 `Metroid (USA)` 只有 128KB —— **扩容 6 倍**。
> `Battle City - 4 Players Hack` size = 40976，原版 Battle City 约 24592 字节。
>
> **推论**：**绝不能用文件大小给候选做分桶**。

#### C.2.3 哪些区域被改 —— 诚实的边界

有一手证据支持的只有两条：
1. **头部会变**（C.2.1，被 GB 的校验、SNES 的 ROM Size 字段强制）
2. **大量数据被追加**（BPS 的 `TargetRead` 与 `target-size > source-size`）

> ⚠ 关于「文本/字库/指针表/解压程序」的细分，权威语料是 romhacking.net 的 Documents 区，而该站已于 2024-08-01 转为只读（见 C.3.1）。**本调研未取得该细分的一手来源，故不作为事实断言。**
> **【推断】** 从工具与格式形态推测：扩容型汉化的改动集中在 **ROM 尾部**（追加的文本/字库 bank）+ **头部** + **指针表中零散的小改动**，这正是 BPS 的 `SourceCopy`/`TargetRead` 混合所优化的形态。

---

### C.3 现有做「结构化/模糊识别」的项目

#### C.3.1 romhacking.net（RHDN）—— 记录了基准 ROM 的哈希

**确认：RHDN 的翻译/hack 页面带有 `ROM / ISO Information:` 区块，记录基准 ROM 的 No-Intro 名 + CRC32 + SHA-1（常含 MD5/SHA-256）。**
但**该字段是自由文本，格式极不统一**。四个真实存档样本：

*完整结构化（No-Intro 匹配形式）—— [translation 6000](https://web.archive.org/web/20210315103630/https://www.romhacking.net/translations/6000/)：*
```
ROM / ISO Information:
Database match: Super Metroid (Japan, USA) (En,Ja).sfc
Database: No-Intro: Super Nintendo Entertainment System (v. 20210222-050638)
File/ROM SHA-1: DA957F0D63D14CB441D215462904C4FA8519C613
File/ROM CRC32: D63ED5F8
```

*文件名 + 三种哈希 —— [translation 2153](https://web.archive.org/web/20231127101710/https://www.romhacking.net/translations/2153/)：*
```
Legend of Zelda, The - A Link to the Past (USA).sfc
MD5: 608C22B8FF930C62DC2DE54BCD6EBA72
SHA-1: 6D4F10A8B10E10DBE624CB23CF03B88BB8252973
CRC32: 777AAC2F
```
（该页还有 `Patching Information: No-Header (SNES)`）

*GoodTools 形式 —— [translation 4500](https://web.archive.org/web/20230501065857/https://www.romhacking.net/translations/4500/)：*
```
Famicom Tantei Club Part II (J) (V1.0) (NP).smc - GOODSNES (HEADERED)
CRC32: 0F1D367F  MD5: ...  SHA-1: ...  SHA-256: ...
```

*只有 CRC32，但**包含补丁后的结果** —— [translation 1499](https://web.archive.org/web/20221028135915/https://www.romhacking.net/translations/1499/)：*
```
Original CRC32 - 2165121C
Patched CRC32 - 1C7C8310
```

> 每条记录还带 `Patching Information`（如 `No-Header (SNES)`），告诉你补丁期望的是**去掉 512 字节 copier 头**的版本 —— 这对复现哈希是必需的。

**站点状态**：RHDN 已于 **2024-08-01 转为只读**（[官方公告 news 3074](https://www.romhacking.net/news/3074/)）。
**全库已释出到 Internet Archive**：

| 项 | 值 |
|---|---|
| Item | https://archive.org/details/romhacking.net-20240801 |
| 描述 | "Export of SQL Database and Files for Abandoned, Fonts, Homebrew, Documents, Utilities, Hacks, and Translation sections" |
| `rhdn_20240801.zip` | 12,573,450,015 字节 |
| **`romhacking.sql.zip`** | **6,820,209 字节（约 6.8 MB）** |

> ⚠ **实测限制**：该 item 的元数据带 `"access-restricted-item": "true"`，collections 含 `loggedin`，匿名直接下载返回 **HTTP 401**。需登录 archive.org 账号。
> **【推断】** 那个 6.8 MB 的 SQL dump 是构建「基准 ROM ↔ hack」映射的**最高价值单一物件** —— 它包含每个 hack/translation 的结构化记录（含 ROM info 字段），而不需要下载 12 GB 的补丁。

#### C.3.2 SMDB（Hardware Target Game Database）

`smokemonster/SMDB` 已 404。现址：[frederic-mahe/Hardware-Target-Game-Database](https://github.com/frederic-mahe/Hardware-Target-Game-Database)（`SmokeMonsterPacks/EverDrive-Packs-Lists-Database` 重定向至此）。

README 原文（**列顺序确认**）：
> "One record per line, **six tab-separated columns** per record:
> 1. SHA256 value,  2. folder and file name (Unix/Linux format),  3. SHA1,  4. MD5,  5. CRC32,  6. file size (in bytes, new feature not yet available in all SMDBs)"

用法：`parse_pack.py` 从目录树生成 SMDB；`build_pack.py` 拿一堆散乱 ROM + SMDB 重命名整理到目标布局，并输出 `Missing.txt`。

> **定位**：SMDB 是「哈希 → 规范路径」的**清单**，不是策展的 hack 数据库 —— 只有当整理者把某个 hack 放进 pack 里，它才会出现。

#### C.3.3 No-Intro / Redump —— 修改版**不在收录范围**

- [No-Intro Database Navigation Guide](https://wiki.no-intro.org/index.php?title=Database_Navigation_Guide) 定义了 **Aftermarket**（「Any unlicensed game that was first distributed after the last-known original licensed game released for that platform」）与 **Unlicensed**，但**没有** Hacks / Translations 类别。
- [Naming Convention](https://wiki.no-intro.org/index.php?title=Naming_Convention) 的字段顺序为 `[BIOS flag] Title (Region) (Languages) (Version) (Devstatus) (Additional) (Special) (License) [Status]`，只有 Title 与 Region 是必填。

> **结论：不存在 No-Intro 的 "ROM Hacks" 集合。** 最接近的 Aftermarket / Unlicensed / Homebrew 覆盖的是**未授权的原创游戏**，不是修改过的 dump。Redump 同理，只收原盘。

#### C.3.4 ⭐ RetroAchievements 的 rc_hash —— 逐平台自定义哈希（最有价值的现有实践）

来源：[rcheevos `src/rhash/hash_rom.c`](https://github.com/RetroAchievements/rcheevos/blob/develop/src/rhash/hash_rom.c)、[`hash_disc.c`](https://github.com/RetroAchievements/rcheevos/blob/develop/src/rhash/hash_disc.c)。**全部为 MD5。**

**头部剥离规则**：

| 系统 | 规则 |
|---|---|
| NES / Famicom | 开头是 `"NES\x1a"` 则跳 16 字节；`"FDS\x1a"` 同样跳 16 |
| SNES | `calc = (size / 0x2000) * 0x2000;` 若 `size - calc == 512` 则跳 512 |
| Atari 7800 | `buffer[1..9] == "ATARI7800"` 则跳 128 |
| Atari Lynx | 开头 `"LYNX"` 则跳 64 |
| PC Engine | `size & 512` 则跳 512 |
| Super Cassette Vision | 开头 `"EmuSCV"` 则跳 32 |
| **GB/GBC/GBA、MD、SMS、GG、32X、SG-1000、WonderSwan、NGP、Virtual Boy、Jaguar、MSX…** | **整文件，不做修改** |

**N64**：先按首字节归一化字节序再 MD5 —— `0x80`=z64（原生）、`0x37`=v64（按 16 位交换）、`0x40`=n64（按 32 位交换）、`0xE8`/`0x22`=ndd。**`.z64`/`.v64`/`.n64` 三种容器得到同一个哈希。**

**NDS**：**不是整文件**。剥离 SuperCard 头后只哈希：头部 `0x000`–`0x15F`（352 字节）+ ARM9 代码 + ARM7 代码 + `0xA00` 字节的 icon/title 数据（不足则补零）。

**⭐ PlayStation（PSX）—— 只哈希启动可执行文件**：
```c
sector = rc_hash_find_playstation_executable(iterator, track_handle, "BOOT", "cdrom:", exe_name, ...);
if (!sector) { sector = rc_cd_find_file_sector(iterator, track_handle, "PSX.EXE", &size); ... }
...
if (memcmp(buffer, "PS-X EXE", 7) == 0)
    size = (buffer[31]<<24 | buffer[30]<<16 | buffer[29]<<8 | buffer[28]) + 2048;
...
md5_append(&md5, (md5_byte_t*)exe_name, (int)strlen(exe_name));   /* 文件名也参与哈希 */
result = rc_hash_cd_file(&md5, iterator, track_handle, sector, exe_name, size, "primary executable");
```
即 **PSX hash = MD5( 启动可执行文件名字符串 ‖ 该可执行文件的字节 )**，大小取自 `PS-X EXE` 头偏移 28 的 4 字节值 **+ 2048**。**光盘其余部分完全不参与哈希。**
源码注释解释为何把文件名也算进去：*"there's a few games that use a singular engine and only differ via their data files"*。

同系列：**PS2** 用 `BOOT2` / `cdrom0:`；**PSP** 哈希 `PSP_GAME\SYSDIR\EBOOT.BIN`；**PS3** 哈希 `PS3_GAME\USRDIR\EBOOT.BIN`。

**Arcade**：直接哈希**文件名**（去扩展名）——「the cores are pretty stringent about having the right ROM data」。

**这套方案容忍什么、不容忍什么**：

| 容忍 | 不容忍 |
|---|---|
| iNES/FDS/SMC/LYNX/A7800/PCE/EmuSCV/SuperCard 头的有无 | 哈希区域内的任何字节变化 |
| N64 字节序（z64/v64/n64） | 卡带系统上 = 整个 ROM |
| 光盘容器格式（`.cue`/`.m3u`/`.iso`/`.chd`） | —— |
| zip 压缩 | —— |
| **PSX 系：光盘上非可执行内容的任何改动**（数据文件、音轨、填充、ISO 布局） | PSX 系：可执行文件本身的改动 |

> ⭐ **对汉化识别的直接价值**：对 **PS1/PS2/PSP 光盘汉化**，若汉化只改数据文件而不改主 EXE，**RA 的哈希仍然匹配原版** —— 这是一条现成的、可直接复用的「结构不变量」技术路径。
> 对卡带系统，翻译补丁**必然**改变 RA 哈希。

**但这是设计使然 —— RA 为每个游戏关联多个哈希**：[Hash Labels 指南](https://docs.retroachievements.org/guidelines/content/hash-labels.html) 的标签词表既含保存组织（`nointro`、`redump`、`tosec`、`goodtools`、`fbneo`、`cleancpc`、`mamesl`、`nongood`），也含 **hack 来源**（`rhdn`、`rhdc`、`romhackplaza`、`smwcentral`、`metconst`、`pokecommunity`、`gamebanana`、`github`、`itchio`…）。
[ROM hacks 政策](https://docs.retroachievements.org/guidelines/content/achievements-for-rom-hacks.html)明确接纳翻译补丁。

**可通过 API 查询**：[`API_GetGameHashes`](https://api-docs.retroachievements.org/v1/get-game-hashes.html)
```
GET https://retroachievements.org/API/API_GetGameHashes.php?y=<key>&i=<gameID>
```
返回每个哈希的 `MD5` / `Name` / `Labels[]` / `PatchUrl`：
```json
{ "MD5": "1b1d9ac862c387367e904036114c4825",
  "Name": "Sonic The Hedgehog (USA, Europe) (Ru) (NewGame).md",
  "Labels": ["nointro", "rapatches"],
  "PatchUrl": "https://github.com/RetroAchievements/RAPatches/raw/main/MD/Translation/Russian/1-Sonic1-Russian.zip" }
```

配套的 [RAPatches](https://github.com/RetroAchievements/RAPatches) 按 `<System>/<Category>/<Language>/` 组织，含 `SNES/Translation/{Arabic, Chinese, English, ...}`。
> ⚠ **中文覆盖率实测**：`Translation/Chinese` 目录下 SNES **3** 个、NES **5** 个、GBA **3** 个、GBC **2** 个。**RA 严谨但对汉化的覆盖极小。**

#### C.3.5 libretro-database 的 hacks DAT —— 有，但无中文

**【实测】** `metadat/hacks/` 下有 24 个 `.dat`，`metadat/libretro-dats/` 有 SNES 同人翻译 DAT。格式为 clrmamepro，`rom(...)` 记录的是**打过补丁后的**尺寸/CRC/MD5/SHA1，`name` 是基准 ROM 的 No-Intro 文件名，并附 `patch` URL：

```
game (
    name "Metroid - Rogue Dawn [Hack by Grimlock, Optomon, and snarfblam]"
    description "Metroid - Rogue Dawn by Grimlock, Optomon, and snarfblam version (1.21)"
    rom ( name "Metroid (USA).nes" size 786448 crc BDCF38B2 md5 f8aeb04e76a1ccbcf8c1387b58aac8d5 sha1 e2abd7a4647eab55d1b5a9b0b71a0a79f6317da2 )
    patch "https://www.romhacking.net/hacks/3280/"
)
```

**语言分布实测**（NES + SNES + GBA + NDS + PSX + MD 六个 hacks DAT 合计）：
```
  85 [T-En
   1 [T-Fr
   0  中文
```
`metadat/libretro-dats/Nintendo - Super Nintendo Entertainment System.dat`（529 条同人翻译）：
```
1036 [T-En
   4 [T-Edition
   0  中文
```

> ⚠ **结论：libretro-database 对汉化 ROM 的覆盖率为零。**

#### C.3.6 模糊哈希（ssdeep / TLSH）—— 无 ROM 领域先例

- [ssdeep](https://ssdeep-project.github.io/ssdeep/)（CTPH，上下文触发分片哈希）与 [TLSH](https://github.com/trendmicro/tlsh) 是标准工具，但**在任何 ROM 项目中都找不到应用先例**。
- ROM 补丁领域唯一实际部署的分片哈希方案是 **APS (GBA) 的每 64 KB CRC16**（C.1.5）。

**ssdeep 的关键限制（从源码 `fuzzy.c` / `fuzzy.h` 读出）**：

| 常量 | 值 | 含义 |
|---|---|---|
| `SPAMSUM_LENGTH` | ≤ **64** | **签名最长 64 字符** |
| `MIN_BLOCKSIZE` | 3 | 块大小 = `3 << index` |
| `NUM_BLOCKHASHES` | 31 | — |
| `ROLLING_WINDOW` | 7 | 两串必须有 ≥7 字符的公共子串才可能匹配 |

块大小选择逻辑：`while (FUZZY_BS(bi) * SPAMSUM_LENGTH < total_fixed_length) ++bi;`
> **【推导】** 即块大小 ≈ 文件大小 / 64。**一个 4 MB 的 ROM 会用 98304 字节的块** —— 极其粗糙。

比较约束（`fuzzy_compare`）：只有两个签名的块大小**相等或相差 2 倍**时才会实际比较，否则直接返回 0。
> ⭐ **这对汉化场景是致命的**：汉化常把 ROM 扩容 2–6 倍（C.2.2），扩容超过 2 倍时 **ssdeep 直接给 0 分，无法比较**。

**TLSH 相对更适合**：[README](https://github.com/trendmicro/tlsh) 原文 —— 最小输入 50 字节，摘要 35 字节（`T1` + 70 个十六进制字符）；`diffxlen` 函数「removes the file length component of the tlsh header from the comparison」，**可显式忽略文件长度差异**。

> **【推断】** RetroAchievements 的方案（C.3.4）是**结构化**而非模糊的 —— 它通过哈希一个**规范子集**（启动可执行文件、ARM9/ARM7 blob、去头 ROM）取得稳定性，这比编辑距离型模糊哈希更契合本问题。**把它推广到汉化场景 = 只哈希不变的代码段、排除文本/字库 bank。但目前无人做过。**

---

### C.4 可用的汉化 ROM 哈希数据源（实测）

> ⚠ **本节推翻了一个常见判断。** 网络上（以及本次调研中一个并行子代理的初步结论）普遍认为「不存在带哈希的汉化 ROM 数据库」。**这是错的。**

#### C.4.1 ⭐ TOSEC —— 正式收录汉化版，用 `[tr zh]` 标记

**TOSEC 命名规范官方定义**（[TOSEC Naming Convention](https://www.tosecdev.org/tosec-naming-convention)）：

整体文法：
```
Title version (demo) (date)(publisher)(system)(video)(country)(language)(copyright status)(development status)(media type)(media label)[dump info flags][more info]
```

翻译标志的官方定义（原文）：
> "The original software has been deliberately hacked/altered to **translate into a different language** than originally published/released."

| 语法 | 含义 |
|---|---|
| `[tr]` | Translation |
| **`[tr language]`** | **Translated to Language** |
| `[tr language-partial]` | 部分翻译 |
| **`[tr language Translator]`** | **由某译者/汉化组翻译成某语言** |
| `[tr language1-language2]` | 翻译成两种语言 |

> 「Translator name is not allowed if language isn't identified too（`[tr Translator]` **not** allowed）」
> **语言码采用 ISO 639-1 两位小写字母** —— 因此**中文 = `zh`**。

**【实测】libretro-database 的 TOSEC 镜像中的中文翻译条目数**（数据版本 2026.08.01）：

| 平台 | 总条目 | **`[tr zh]`** | `[tr en]` |
|---|---|---|---|
| **NES / Famicom** | 10820 | **722** | 840 |
| Sega Mega Drive / Genesis | 6065 | **86** | 104 |
| Game Boy | 3767 | **74** | 194 |
| SNES / SFC | 3663 | **48** | 310 |
| Game Boy Advance | 295 | **26** | 0 |
| Game Boy Color | 2061 | **4** | 14 |
| Master System | 886 | 0 | 34 |
| **合计（采样平台）** | | **960** | |

NES 的 TOSEC 翻译语言分布（实测）：
```
 840 [tr en    722 [tr zh    410 [tr pt    388 [tr es    364 [tr fr
 254 [tr ru    124 [tr it    114 [tr sv    108 [tr de     86 [tr ko
```
> **中文是 TOSEC NES 集里第二大的翻译语言。**

真实条目样本：
```
game (
	name "1942 (Japan, USA)[tr zh MS emumax][v.20060217]"
	rom ( name "1942 (1985-12-11)(Capcom)(JP-US)[tr zh MS emumax][v.20060217].nes"
	      size 40976 crc A36F13D0 md5 0B3CEADB1A155807E7C7AA37FD9904F7
	      sha1 859A8B0459496FB4E998FFCF04414A69155C1EFF )
)
game (
	name "Chrono Trigger (Japan)[tr zh]"
	rom ( name "Chrono Trigger (1995)(Square)(JP)[tr zh].bin"
	      size 4194304 crc BC353C81 md5 BAE3C4D0B79F6AA667150A8F8C3A2CBC
	      sha1 845A31166475CA4E9C5A049F314AF907937ADCEF )
)
```
> 注意 TOSEC 的记录里**同时有基准游戏名（`1942 (Japan, USA)`）与汉化组信息（`MS emumax`）与版本（`v.20060217`）**，正是刮削需要的全部字段。

> ⭐⭐ **实现上的关键陷阱**：**TOSEC 不使用 header skipper，它按「带头文件」原样哈希。**
> 上面 `1942` 那条 size = **40976**（= 40960 + 16 字节 iNES 头），而 No-Intro headerless DAT 里的对应尺寸是 40960。
> **因此：要命中 TOSEC 的这 960 条中文条目，必须用「含头」的整文件哈希；只算「去头」哈希会全部落空。**
> 正确做法是**两套哈希都算**（见 [D.1](#d1-l0--容器归一化做对这一步l1-命中率翻倍)）。

⚠ **重要限制**：libretro 的 TOSEC 镜像只有 **33 个平台的 DAT**，缺 NDS / N64 / PS1 / Saturn 等。完整 TOSEC 需从 [tosecdev.org](https://www.tosecdev.org/downloads) 获取。

#### C.4.2 ⭐ Hasheous —— 免费哈希查询 API，实测能识别汉化 ROM

[Hasheous](https://github.com/gaseous-project/hasheous) 是一个公开免费的 API，把 TOSEC / No-Intro / Redump / MAME / RetroAchievements 等 DAT 摄入并提供哈希查询，同时代理 IGDB 元数据。

**【实测】API 端点**（从 `https://hasheous.org/swagger/v1/swagger.json` 直接读取）：

| 方法 | 路径 |
|---|---|
| POST | `/api/v1/Lookup/ByHash` |
| GET | `/api/v1/Lookup/ByHash/crc/{crc}` |
| GET | `/api/v1/Lookup/ByHash/md5/{md5}` |
| GET | `/api/v1/Lookup/ByHash/sha1/{sha1}` |
| GET | `/api/v1/Lookup/ByHash/sha256/{sha256}` |
| GET | `/api/v1/Lookup/Platforms` |

**【实测】用一个汉化 NES ROM 的 SHA1 查询**（`1942` 的 MS emumax 汉化版）：
```bash
curl -s "https://hasheous.org/api/v1/Lookup/ByHash/sha1/859a8b0459496fb4e998ffcf04414a69155c1eff"
```
返回（节选）：
```json
{ "id": 18199,
  "name": "1942",
  "platform": { "name": "Nintendo Entertainment System",
                "metadata": [ {"source":"IGDB","link":"https://www.igdb.com/platforms/nes"},
                              {"source":"TheGamesDb", ...}, {"source":"RetroAchievements", ...} ] },
  "publisher": { "name": "Capcom", "metadata":[{"source":"IGDB","link":"https://www.igdb.com/companies/capcom"}] },
  "signature": {
    "game": { "name":"1942", "year":"1985-12-11", "publisher":"Capcom",
              "system":"Nintendo Famicom & Entertainment System",
              "countries":{"JP":"Japan","US":"United States"} },
    "rom":  { "name":"1942 (1985-12-11)(Capcom)(JP-US)[tr zh MS emumax][v.20060217].nes",
              "size":40976, "crc":"a36f13d0",
              "md5":"0b3ceadb1a155807e7c7aa37fd9904f7",
              "sha1":"859a8b0459496fb4e998ffcf04414a69155c1eff" } } }
```

**【实测】SNES 汉化版同样命中**（`Aretha (1993)(Yanoman)(JP)[tr zh].bin`，SHA1 `7d4ac8be…`）→ 返回 `name: Aretha`、`platform: Nintendo Super Nintendo Entertainment System`。

> ⭐ **这是本调研对用户问题最直接的答案：一个免费的、无需 API key 的、单次 HTTP GET 就能把汉化 ROM 的 SHA1 映射到「游戏名 + 平台 + 发行商 + IGDB/TGDB 链接」的服务，且已实测可用。**

#### C.4.3 ⭐ BizHawk 的 GoodTools 数据库 —— 离线、SHA1 键、含中文

[BizHawk](https://github.com/TASEmulators/BizHawk) 在 `Assets/gamedb/` 下随发行版附带纯文本数据库，格式为 TSV：
```
;Hash	Status	Name	System ID	Notes	MetaData	Configurations	CoreForce
```
Status 码（文件内注释原文）：
```
;b - bad dump      ;v - bad dump (??)   ;t - translated rom   ;o - overdump (bad)
;i - bios          ;d - homebrew        ;h - hack             ;u - unknown
```

**【实测】`gamedb_goodnes.txt`（源自 GoodNES，22097 行）**：

| 指标 | 数量 |
|---|---|
| Status = `T`（translated rom）的条目 | **2945** |
| 含 `[T+Chi` / `[T-Chi` 的条目 | **646** |
| 含 `(Ch)`（中文原版/盗版）的条目 | 792 |

翻译语言分布（实测）：
```
 833 Rus   700 Eng   640 Chi   249 Bra   217 Spa   211 Fre
 100 Pol    70 Swe     69 Ger    66 Ita    53 Kor    14 ChS
```
样本：
```
859a8b0459496fb4e998ffcf04414a69155c1eff	T	1942 (JU) [T+Chi_MS emumax]	NES
6d419c58b56098b94d5f665139fd84a9e0f2f60d	T	4 Nin Uchi Mahjong (J) (PRG1) [T+Chi]	NES
```
> 注意第一行的 SHA1 与 C.4.1 / C.4.2 中 TOSEC 的 `1942 … [tr zh MS emumax]` **完全相同** —— 两个独立数据源互相印证。

**其他平台实测**：

| gamedb 文件 | `[T+/-]` 条目 | 中文条目 |
|---|---|---|
| `gamedb_goodnes.txt`（GoodNES） | 2945 | **646** |
| `gamedb_sega_md.txt`（GoodGen） | 691 | **172** |
| `gamedb_snes.txt` | 35 | 0 |
| `gamedb_pce_hucards.txt` | 22 | 0 |
| `gamedb_gb.txt` / `gbc` / `gba` / `n64` | 0 | 0 |

**GoodTools 命名码官方定义**（`GoodCodes.txt`，由 Cowering 制定，Psych0phobiA 编写，[archive.org 副本](https://archive.org/download/SegaGameGearCollectionByGhostware/GoodCodes.txt)）：
```
:   [a?] Alternate       [p?] Pirate            :
:   [b?] Bad Dump        [t?] Trained           :
:   [f?] Fixed           [T-] OldTranslation    :
:   [o?] Overdump        [T+] NewerTranslation  :
:   [h?] Hack            (-) Unknown Year       :
:   [!p] Pending Dump    [!] Verified Good Dump :
```
国家码含 `(C) China`、`(HK) Hong Kong`、`(K) Korea`、`(J) Japan`。

> ⚠ **诚实更正**：`GoodCodes.txt` **只**定义了 `[T-]` = OldTranslation 与 `[T+]` = NewerTranslation，**没有**定义语言子码/版本/组名后缀。`[T+Chi1.0_Group]` 这种形式是 GoodTools 实际输出的**事实惯例**，而非官方码表条目。（其实际使用可由 BizHawk、Zaparoo、puNES 等第三方解析器代码佐证。）

#### C.4.4 中文社区自建：壬天堂世界「中文ROM补完计划」

来源：[bbs.newwise.com 帖 934484](https://bbs.newwise.com/forum.php?action=printable&mod=viewthread&tid=934484)（[archiver 视图](https://bbs.newwise.com/archiver/tid-934484.html)）

| 项 | 内容 |
|---|---|
| **命名标准（原文）** | `编号 - 主标题 - 副标题 (版本号) (修正版) (简繁) [汉化组织]` |
| 提供的数据 | **"OfflineList Chinese Games DATs (Support ClrMamePro)"**，含 **CRC32** |
| 规模（不同时点的统计） | 736 条（20250927）/ 505 条（20251001）/ 1297 条（20240307） |
| 覆盖平台 | FC、SFC、GB、GBC、GBA、NDS（含神游/同人）、N64、PS1、SS、DC、MD |
| 介质约定 | PS1/SS 统一 bin+cue；DC 统一 cdi |
| 条目样本（原文） | `SLPS-02080本格派四人麻将俱乐部 (v1.0) (简) [施珂昱] 909A3C8C` |
| 主持人 | xiong_online |

> 该样本同时含 **PS1 序列号 + 中文名 + 版本 + 简繁 + 汉化组 + CRC32**，正是刮削所需的完整信息。
> ⚠ **限制**：内容在 Discuz 论坛楼层中，DAT 为论坛附件（需登录），**难以自动化获取**；且是唯一一处系统性记录了汉化组、简繁、修正版等中文特有维度的来源。

#### C.4.5 其他相关来源

| 来源 | 内容 | 中文覆盖 |
|---|---|---|
| [archive.org `En-ROMs`](https://archive.org/details/En-ROMs) | 49 个 `<System> [T-En] Collection (<date>).zip` clrmamepro DAT，命名 `<No-Intro 基准名> [T-En by <Group> v<version>]` | ❌ **仅英化**（实测搜索 `[T-Zh]`/`[T-Chi]` 为零）；且父 item `access-restricted`，匿名下载 **401** |
| [archive.org `hackset`](https://archive.org/details/hackset) | GB/GBC/GBA/NES/SNES/GENESIS/SMS/N64 的 hack+translation romset，带 JSON 元数据 | 未声明中文覆盖 |
| [archive.org `rom-hack-patch-archive`](https://archive.org/details/rom-hack-patch-archive) | romhacking.net hack 区 + 英译 + nsmbhd + metroidconstruction 的补丁存档；「ROM folders are usually labeled as shortened No-Intro names」 | 原文：「**I rarely focus on including Non-English translations**」 |
| [archive.org NDS 中文非官方汉化](https://archive.org/details/nintendodschineseunofficialtranslations) | 68 个 NDS 汉化 ROM（从一张烧录卡上抢救） | ✅ 但**无 DAT / 无哈希清单** |
| [yingw/rom-name-cn](https://github.com/yingw/rom-name-cn) | 以 No-Intro/Redump/MAME/libretro DAT 名为键的**中文译名对照表**（CSV/JSON/DAT，50000+ 条，30+ 平台） | ✅ 但这是**官方游戏名的中文翻译**，不是汉化 ROM 的哈希库。其 SFC 行版本注为 `Retail+ST+T-EN+T-CN` |
| [i30817/rhdndat](https://github.com/i30817/rhdndat) | RHDN 更新检查 + ROM 改名工具 | 见 C.5 |

---

### C.5 已被验证可行的工程手段：`.rxdelta` 反向补丁 + 扩展属性

[rhdndat](https://github.com/i30817/rhdndat) 提供了一个**已在实践中运行**的方案，直接针对「硬补丁（hardpatched）ROM 无法匹配 DAT」的问题：

| 机制 | 说明 |
|---|---|
| 识别键 | **SHA1** |
| 反向补丁 | 与 ROM 同名、扩展名 `.rxdelta` 的 **xdelta3** 补丁，可**把硬补丁 ROM 还原回原版** |
| 结果缓存 | 把还原后的原版 SHA1 写入**文件扩展属性** `user.rhdndat.rom_sha1` |
| 版本追踪 | `rhdndat.ver` 文件，成对的「版本号行 + romhacking.net URL 行」，用于检查补丁更新 |
| 搜索目标 | No-Intro / Redump 的 XML DAT |

> ⭐ **这是本调研找到的、唯一一个真正解决了「已硬补丁 ROM → 原版身份」的落地方案。**
> **【推断】** 其思路可以推广：**不要试图从被改的字节里反推原版，而是把「原版身份」作为一份旁路元数据持久化下来**（扩展属性、sidecar JSON、或本地 SQLite）。一旦某份汉化 ROM 被人工/半自动识别一次，就永久记住。

---

## 第 D 章 · 对自动化刮削工具的可执行建议

> 本章是全文的落地部分。按**识别置信度分层**组织，每层给出：判据、数据源、实现要点、以及该层能/不能解决什么。
> 本章的架构设计属 **【推断】**，但每条技术判据都指向前文的一手来源。

### D.0 分层总览

| 层级 | 名称 | 置信度 | 能识别汉化版？ | 典型命中率来源 |
|---|---|---|---|---|
| **L0** | 容器归一化（前置步骤，非识别层） | — | — | 决定 L1 能否工作 |
| **L1** | 精确哈希（CRC32 / MD5 / SHA-1） | **确定** | ✅ 若该汉化版被 TOSEC/GoodTools 收录 | No-Intro + Redump + **TOSEC** + GoodTools |
| **L2** | 头部 / 序列号 | **高** | ⚠ 取决于汉化是否改了头 | ROM 内部字段 + `metadat/serial/` |
| **L3** | 结构指纹（分段哈希 / 代码段哈希 / 规范子集哈希） | **中** | ⚠ 光盘类可行，卡带类需自建 | rcheevos 思路 + APS/GBA 思路 |
| **L4** | 文件名解析 | **低** | ⚠ 中文文件名可解析出汉化组/简繁 | TOSEC / GoodTools / No-Intro / 中文社区命名规范 |
| **L5** | 模型推断（LLM / 模糊匹配） | **不确定** | ⚠ 仅作候选生成，必须人工确认 | — |

**总原则【推断】**：**每一层都必须输出「置信度 + 证据来源」，而不是只输出一个结果。** 上层命中即停；下层结果必须可被用户覆盖，且覆盖结果要持久化（见 D.6）。

---

### D.1 L0 —— 容器归一化（做对这一步，L1 命中率翻倍）

这一层不产生识别结果，但**决定了 L1 能不能工作**。所有操作都有一手依据。

| 平台 | 归一化动作 | 依据 |
|---|---|---|
| **NES / FDS** | 若首 4 字节 `4E 45 53 1A`（或 `46 44 53 1A`）→ **跳 16 字节**；若 iNES `Flags6 bit2` 置位再**跳 512 字节 trainer** | [nesdev INES](https://www.nesdev.org/wiki/INES)；clrmamepro skipper；rcheevos `hash_rom.c` |
| **SNES** | `calc = (size / 0x2000) * 0x2000;` 若 `size - calc == 512` → **跳 512 字节** | rcheevos `rc_hash_snes`；fullsnes（`filesize % 1024 == 512`）；bsnes（`size & 0x7fff == 512`） |
| **Atari 7800** | `buffer[1..9] == "ATARI7800"` → 跳 **128** | rcheevos；RomVault；igir |
| **Atari Lynx** | 首 4 字节 `"LYNX"` → 跳 **64** | 同上 |
| **PC Engine** | `size & 512` → 跳 **512** | Mednafen `huc.cpp`；Geargrafx `media.cpp`；rcheevos |
| **N64** | 按首字节归一化到 z64：`0x80`=原生；`0x37`→按 16 位交换；`0x40`→按 32 位交换 | [n64brew](https://n64brew.dev/wiki/ROM_Header)；mupen64plus `rom.c`；rcheevos |
| **Mega Drive `.smd`** | `romsize >= 0x4200 && (romsize & 0x3fff) == 0x200` → 跳 512 **且需去交错** | PicoDrive `media.c` |
| **压缩包** | 从 ZIP 中央目录 `+16` 处直接读每个成员的 CRC32（**不解压**）做快速预筛；但要匹配 headerless DAT 仍必须解压 | [PKWARE APPNOTE 6.3.9 §4.3.12](https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.9.TXT) |
| **光盘（chd / cue+bin / iso）** | 见 A 章各平台；注意 RetroArch 读的是 **2352 字节原始扇区**，所有 Sega 偏移需 **+0x10** | RetroArch `chd_stream.c` / `task_database_cue.c` |

**关键实现建议【推断】**：
1. **同时计算两套哈希** —— 「含头整文件」与「去头后」。这是 RomVault（`Alt*` 字段）与 igir 都采用的做法，**一次读取即可完成**，成本几乎为零，却能同时匹配 headered DAT 与 headerless DAT。
2. **不要相信扩展名**。fullsnes 原文：`.SMC` "is often used for **ANY** type of SNES ROM-images"。检测必须基于内容与文件大小取模。
3. **不要用文件大小给候选分桶** —— 汉化扩容 2–6 倍是常态（C.2.2）。

---

### D.2 L1 —— 精确哈希（首选层，且对汉化版比想象中有效）

#### D.2.1 数据源优先级

| 顺序 | 数据源 | 覆盖 | 汉化覆盖 | 获取方式 |
|---|---|---|---|---|
| 1 | **No-Intro** | 卡带原版 | ❌ 政策上不收 | DAT-o-MATIC（见 D.2.3 的限制） |
| 2 | **Redump** | 光盘原版 | ❌ | redump.org |
| 3 | ⭐ **TOSEC** | 原版 + **翻译版（`[tr zh]`）** + hack | ✅ **实测 960 条中文条目**（采样平台）<br>⚠ **按「含头」文件哈希** | [tosecdev.org/downloads](https://www.tosecdev.org/downloads)；或 [libretro-database `metadat/tosec/`](https://github.com/libretro/libretro-database/tree/master/metadat/tosec) 镜像（33 平台子集） |
| 4 | ⭐ **GoodTools 派生库（BizHawk `Assets/gamedb/`）** | 含 `[T+Chi]` / `[T-Chi]` | ✅ **实测 NES 646 条、MD 172 条**，SHA-1 键 | [BizHawk 仓库](https://github.com/TASEmulators/BizHawk/tree/master/Assets/gamedb) 直接下载纯文本 |
| 5 | **libretro-database `metadat/hacks/` + `libretro-dats/`** | hack + 英译 | ❌ 实测 0 条中文 | git clone |
| 6 | **RetroAchievements `API_GetGameHashes`** | 策展的 hack/翻译 MD5 + 补丁 URL | ⚠ 实测仅 13 条中文 | [API 文档](https://api-docs.retroachievements.org/v1/get-game-hashes.html) |
| 7 | **壬天堂世界「中文ROM补完计划」** | 汉化专属维度（汉化组/简繁/修正版） | ✅ 736–1297 条 | 论坛附件，**难以自动化** |

#### D.2.2 ⭐ 最省事的做法：直接用 Hasheous API

**【实测可用，无需 API key】**
```
GET https://hasheous.org/api/v1/Lookup/ByHash/sha1/{sha1}
GET https://hasheous.org/api/v1/Lookup/ByHash/md5/{md5}
GET https://hasheous.org/api/v1/Lookup/ByHash/crc/{crc}
GET https://hasheous.org/api/v1/Lookup/ByHash/sha256/{sha256}
POST https://hasheous.org/api/v1/Lookup/ByHash          # 批量
GET https://hasheous.org/api/v1/Lookup/Platforms
```
它已摄入 TOSEC / No-Intro / Redump / MAME / RetroAchievements 等签名源，并把结果映射到 IGDB / TheGamesDb / RetroAchievements 的元数据链接。

**实测证据**：用汉化 NES ROM `1942 [tr zh MS emumax]` 的 SHA-1 查询，返回 `name: 1942` + `platform: Nintendo Entertainment System` + `publisher: Capcom` + IGDB 链接 + 原始 TOSEC rom 名（含 `[tr zh MS emumax][v.20060217]`）。SNES 汉化版 `Aretha … [tr zh]` 同样命中。

> **【推断】建议架构**：本地先用离线 DAT 索引（TOSEC + No-Intro + GoodNES）做一次查询，未命中再回落到 Hasheous 在线查询，并把结果写入本地缓存。这样既快又能覆盖离线 DAT 之外的长尾。

#### D.2.3 获取 DAT 的现实约束

**No-Intro DAT-o-MATIC**：
- `robots.txt`【实测】：`Disallow:`（空，即允许抓取）+ **`Crawl-delay: 5`** + **`Request-rate: 1/5`**（每 5 秒 1 页）
- 但**实际有封禁机制**：本次调研访问 `datomatic.no-intro.org` 时返回的错误页含「contact email to appeal a **ban**」
- [datoso](https://github.com/laromicas/datoso) 文档警告原文：*"Be careful when updating dats from datomatic, sometimes they put a **captcha**, and you may be **banned** if the captcha fails."*

> **【推断】结论**：**不要自建 No-Intro DAT 抓取器。** 要么让用户手动下载一次，要么使用 [libretro-database](https://github.com/libretro/libretro-database) 的 git 镜像（合法、稳定、可 `git pull` 增量更新），要么用 Hasheous 在线 API。

#### D.2.4 哈希算法选择【推断】

| 用途 | 推荐 | 理由 |
|---|---|---|
| 主键 | **SHA-1** | No-Intro/Redump/TOSEC/GoodTools/RHDN 全都有；rhdndat 也用它 |
| 兼容键 | **CRC32** | libretro RDB 的实际索引键；ZIP 中央目录里免费可得；中文社区 DAT 只有它 |
| 补充 | MD5 | RetroAchievements 与 Hasheous 都支持；GoodTools 系有 |
| 建议 | **一次读取同时算 CRC32 + MD5 + SHA-1 + SHA-256**，且**含头/去头各一套** | 全部都是流式算法，单次 I/O 的边际成本极低，却能匹配所有上游 |

---

### D.3 L2 —— 头部 / 序列号

当 L1 完全落空（汉化版不在任何 DAT 中）时，从 ROM 内部读取结构化字段。

#### D.3.1 各平台可提取的「准序列号」

| 平台 | 字段 | 偏移 | 汉化后是否通常保留？ |
|---|---|---|---|
| **GBA** | Game Code（`AGB-UTTD`） | `0x0AC`，4 字节 | **【推断】通常保留** —— 但 `gbafix -c` 明确提供了改它的能力，故必须验证 |
| **NDS** | Gamecode | `0x00C`，4 字节 | **【推断】通常保留**（重打包 ROM 一般沿用原头） |
| **N64** | Game Code | `0x3B`，4 字节（归一化后） | 【推断】通常保留 |
| **SNES** | Game Code（扩展头，需 `+0x1A == 0x33`） | LoROM `0x7FB2` / HiROM `0xFFB2`，4 字节 | 【推断】覆盖率本身就低（老卡无扩展头） |
| **Mega Drive** | Serial number | `0x180`，14 字节 | 【推断】通常保留 |
| **SMS / GG** | Product code + version | `$7FFC`–`$7FFE` | 头本身可选，日版大量缺失 |
| **PS1 / PS2** | `SYSTEM.CNF` 的 `BOOT=` / `BOOT2=` 序列号 | 光盘文件系统内 | **【推断】几乎必然保留** —— 改它会破坏启动 |
| **PSP** | `PARAM.SFO` 的 `DISC_ID` | `PSP_GAME/PARAM.SFO` | 【推断】通常保留 |
| **GC / Wii** | Game ID + Maker code | `0x00` / `0x04` | 【推断】通常保留 |
| **Saturn / DC** | IP.BIN 的 product number | 见 A.14 / A.15 | 【推断】通常保留 |
| **3DS** | Title ID / Product Code | NCSD `0x108` / NCCH `0x150` | 【推断】通常保留；**且加密 ROM 也能读**（A.6.2） |

> ⚠ **必须显式标注为推断**：本次调研**未找到**一手来源系统性统计「汉化版是否保留内部序列号」。上表的「通常保留」全部是基于「改动它没有收益、且部分平台改了会破坏启动」的推理。
> **建议**：工具应把 L2 的结果标为「可能的原版身份」，并**同时展示 L1 未命中这一事实**，让用户确认。

#### D.3.2 反向查询：序列号 → 游戏

| 平台类型 | 数据源 |
|---|---|
| 卡带系统 | [libretro-database `metadat/serial/`](https://github.com/libretro/libretro-database/tree/master/metadat/serial)（29 个平台，格式为 `serial "AGB-BJBJ-JPN"` + `rom ( crc … )`） |
| 光盘系统 | Redump DAT 的 `serial` 属性（game 层与 rom 层各一份） |

> ⭐ **重要能力缺口（本调研发现）**：`metadat/serial/` 里 serial 只是**挂在 CRC 上的元数据**，因为卡带系统的主键是 `rom.crc`。**RetroArch 因此无法用 GBA 内部 game code 去识别一个 CRC 对不上的汉化 ROM。**
> **这正是自建工具能显著超越 RetroArch 的地方**：把 `metadat/serial/` 反向索引成 `serial → {game name, crc, …}`，就得到了一个 RetroArch 没有的识别通道。

#### D.3.3 头部完整性校验（用于判断「头是否被改过」）

| 平台 | 校验字段 | 硬件是否强制 | 用途 |
|---|---|---|---|
| **GB/GBC** | `0x014D` header checksum（覆盖 `0x0134`–`0x014C`） | ✅ 是，不符则不启动 | 若自洽 → 汉化者修过头；结合 `0x014E` global checksum 可判断标题是否被改 |
| **GBA** | `0x0BD` complement check（覆盖 `0x0A0`–`0x0BC`） | ✅ 是 | 同上 |
| **SNES** | `$FFDE` checksum + `$FFDC` complement | ❌ 否 | 仅作头位置定位判据 |
| **NDS** | `0x15E` header CRC16 + `0x15C == CF56h` | 部分 | 格式与完整性校验 |
| **Mega Drive** | `0x18E` checksum（`0x200`→EOF） | ❌ 否 | 头 vs 实算不一致 → 该 ROM 被改过 |

> ⭐ **【推断】一个实用信号**：对 GB/GBA，若**实算的 header checksum 与存储值一致，但整文件哈希不在任何 DAT 中** → 高度提示这是一份「被人修改并正确修复了头」的 ROM，即汉化/魔改版。这可作为「自动打上 `[hack?]` 标签」的判据。

---

### D.4 L3 —— 结构指纹

#### D.4.1 直接可用：rcheevos 的「规范子集哈希」

[rcheevos](https://github.com/RetroAchievements/rcheevos) 已经实现并在生产中运行。**可以直接把 `rc_hash` 当库用**，或复刻其规则。

**对汉化识别最有价值的两条**：

1. ⭐ **PS1/PS2/PSP/PS3 只哈希启动可执行文件**
   `MD5( 启动 EXE 文件名 ‖ 该 EXE 的字节 )`，大小取自 `PS-X EXE` 头偏移 28 的值 +2048。
   > **含义**：**若光盘汉化只改数据文件（文本、字库）而不改主 EXE，RA 哈希与原版完全一致。** 这是一条现成的、对光盘类汉化直接有效的识别路径。

2. **NDS 只哈希头 + ARM9 + ARM7 + icon/title**
   > **【推断】** 若汉化只改 RomFS 内的资源而不改 ARM9/ARM7 代码，该哈希也会保持不变。**但汉化通常需要改字库渲染代码，命中率待验证。**

#### D.4.2 需自建：卡带类的分段哈希

| 思路 | 一手依据 | 可行性评估【推断】 |
|---|---|---|
| **NES 逐段 CRC（PRG / CHR 分开）** | NesCartDB DTD 的 `<prg crc sha1>` / `<chr crc sha1>`；Mesen 的 `PrgChrCrc32` | **较高**。汉化改文本与字库 → CHR 变、PRG 部分变；但**若只改 CHR（纯图形汉化），PRG 的 CRC 仍能匹配原版** |
| **64 KB 块级 CRC16** | APS (GBA) 格式就是这么设计的（C.1.5） | **中**。汉化改动通常集中在文本/字库区，代码区的块哈希应大量保留。**这是 ROM 领域唯一有先例的分块方案** |
| **只哈希代码段** | 无直接先例；rcheevos 的 NDS 规则是最接近的 | **需要逐平台确定代码段范围**，工作量大 |
| **ssdeep 模糊哈希** | [ssdeep](https://ssdeep-project.github.io/ssdeep/) 源码 | **不推荐**。签名上限 64 字符 → 4MB ROM 用 98304 字节的块；且**块大小相差 >2 倍时直接返回 0**，而汉化扩容常常正好超过 2 倍 |
| **TLSH 模糊哈希** | [TLSH](https://github.com/trendmicro/tlsh) README | **可试**。`diffxlen` 可显式忽略文件长度差异，比 ssdeep 更适合扩容场景 |

> **【推断】最佳性价比方案**：**固定 64 KB 分块的 CRC32/xxHash 序列**。
> - 对候选原版与待识别 ROM 各算一次分块哈希序列
> - 用「匹配块数 / 较小 ROM 的总块数」作为相似度
> - 扩容型汉化通常在**尾部追加**，头部的代码块会大量保持对齐 → 相似度高
> - 优点：可解释、可增量、无需第三方库、天然支持「哪些区域被改了」的可视化
> - 依据：这正是 APS (GBA) 已在用的思路，只是把 CRC16 换成更强的哈希

#### D.4.3 补丁文件辅助识别

若用户目录中同时存在补丁文件，**直接读补丁 footer 就能得到原版 CRC32**：

| 格式 | 读取位置 | 得到什么 |
|---|---|---|
| **BPS** | 文件末尾 `len-12`，4 字节 LE | 源 ROM CRC32（+ varint 的 source-size） |
| **UPS** | 文件末尾 `len-12`，4 字节 LE | 源 ROM CRC32（注意双向，有两个候选） |
| **RUP/NINJA2** | 记录内 | 源 **MD5** + 目标 MD5 + **声明的语言** |
| **APS (N64)** | `58`–`68` | Cart ID + Country + 内部 ROM CRC |
| **APS (GBA)** | 每条记录 | 源大小 + 每 64 KB 的 CRC16 |
| **IPS** | —— | **什么也没有** |

> ⚠ **IPS 在 FC/NES 汉化中占绝对主导，而它恰恰完全不可归因。**

---

### D.5 L4 —— 文件名解析

文件名不可靠，但**并非无信息** —— 尤其中文场景下，文件名往往是唯一记录了「汉化组 / 简繁 / 版本」的地方。

#### D.5.1 应实现的四套命名规范解析器

| 规范 | 语法 | 可提取信息 | 来源 |
|---|---|---|---|
| **TOSEC** | `Title version (demo) (date)(publisher)(system)(video)(country)(language)(copyright)(devstatus)(media type)(media label)[dump flags][more info]` | **`[tr zh <译者>]`** → 语言 + 汉化组；`[p]` 盗版；`[a]` 替代版 | [TOSEC Naming Convention](https://www.tosecdev.org/tosec-naming-convention) |
| **No-Intro** | `[BIOS] Title (Region) (Languages) (Version) (Devstatus) (Additional) (Special) (License) [Status]`（仅 Title 与 Region 必填） | 区域、语言、修订版 | [No-Intro Naming Convention](https://wiki.no-intro.org/index.php?title=Naming_Convention) |
| **GoodTools** | `[T+Chi_译者]` / `[T-Chi]` / `[h]` / `[b]` / `[o]` / `[p]` / `[!]` | `T+`=较新翻译、`T-`=较旧翻译 | `GoodCodes.txt`（[archive.org 副本](https://archive.org/download/SegaGameGearCollectionByGhostware/GoodCodes.txt)） |
| **中文社区（补完计划）** | `编号 - 主标题 - 副标题 (版本号) (修正版) (简繁) [汉化组织]` | **汉化组、简/繁、修正版** —— 其他规范都没有的维度 | [壬天堂世界帖 934484](https://bbs.newwise.com/archiver/tid-934484.html) |

> ⭐ **【推断】** 中文库尤其要实现第 4 套。`(简)` / `(繁)` 与 `[汉化组织]` 是**中文场景独有的、且对用户最有意义的**元数据，任何西方命名规范都不覆盖。

#### D.5.2 中文名映射

[yingw/rom-name-cn](https://github.com/yingw/rom-name-cn)：以 No-Intro/Redump/MAME/libretro 的 DAT 名为键的中文译名对照表，CSV/JSON/DAT 格式，50000+ 条、30+ 平台。
> **用途**：**L1/L2 识别成功之后**，把英文规范名翻译成中文显示名。它**不是**识别数据源，而是展示层数据源。

#### D.5.3 RetroArch 的名称宽松匹配（可借鉴）

[libretro 官方文档](https://docs.libretro.com/guides/roms-playlists-thumbnails/)描述的三级回退：
1. ROM 文件名精确匹配
2. 游戏名匹配（把 `` &*/:`<>?\| `` 替换为 `_`）
3. **截断到第一个左圆括号之前**的短名匹配（`Q-Bert's Qubes (USA) (1983)` → `Q-Bert's Qubes`）

---

### D.6 L5 —— 模型推断，以及「一次识别，永久记住」

#### D.6.1 模型只做候选生成

**【推断】** LLM 适合做的：
- 从混乱的中文文件名里抽取「游戏名 / 汉化组 / 版本 / 简繁」
- 把中文游戏名映射回英文规范名（再用规范名去查 L1 数据源）
- 在 L3 给出的多个候选原版之间，结合文件名语义排序

**不适合做的**：直接断言身份。模型输出必须标为最低置信度并要求确认。

#### D.6.2 ⭐ 持久化：最重要的工程建议

来自 [rhdndat](https://github.com/i30817/rhdndat) 的已验证实践：

| 机制 | 做法 |
|---|---|
| **扩展属性** | 把确认后的原版 SHA-1 写入 `user.rhdndat.rom_sha1` 之类的 xattr |
| **反向补丁** | 若有 `.rxdelta`（xdelta3 反向补丁），可把硬补丁 ROM 还原回原版再算哈希 |
| **版本文件** | `rhdndat.ver`：成对的「版本号 + 补丁页 URL」 |

> ⭐ **【推断】核心洞察**：**不要每次都试图从被修改的字节里反推原版。** 识别一次（哪怕靠人工）之后，就把结论作为旁路元数据持久化 —— xattr、sidecar JSON、或本地 SQLite（键 = 文件 SHA-1）。
> 对一个「大部分是汉化版」的库，这条建议的实际价值**高于**任何模糊匹配算法。

---

### D.7 推荐的完整流水线（伪代码）

```
for each file:
    # ---------- L0 归一化 ----------
    container = detect_container(file)            # zip/7z/chd/cue/iso/raw
    raw       = normalize(container)              # 解压/取数据轨
    hdr       = detect_rom_header(raw)            # iNES/SMC/LNX/A78/PCE/SMD/N64 字节序
    data      = strip_and_normalize(raw, hdr)

    hashes_headered   = {crc32, md5, sha1, sha256}(raw)
    hashes_headerless = {crc32, md5, sha1, sha256}(data)

    # ---------- 缓存 ----------
    if cached := lookup_cache(hashes_headerless.sha1):
        emit(cached, confidence=CACHED); continue

    # ---------- L1 精确哈希 ----------
    for src in [NoIntro, Redump, TOSEC, GoodTools, libretro_hacks, RA]:
        if hit := src.lookup(hashes_headerless) or src.lookup(hashes_headered):
            emit(hit, confidence=EXACT, evidence=src.name); goto next
    if hit := hasheous_api(hashes_headerless.sha1):        # 在线兜底
        emit(hit, confidence=EXACT_ONLINE); goto next

    # ---------- L1.5 补丁旁证 ----------
    for patch in sibling_patches(file):                    # *.bps *.ups *.rup *.aps
        if src_crc := read_patch_source_hash(patch):
            if hit := NoIntro.lookup_crc(src_crc):
                emit(hit, confidence=HIGH, note="via patch footer"); goto next

    # ---------- L2 头部 / 序列号 ----------
    serial = extract_internal_serial(data, platform)       # A 章各平台
    if serial and (hit := serial_index.lookup(serial)):
        emit(hit, confidence=HIGH, note="serial match; hash mismatch ⇒ 可能是汉化/魔改版")
        goto next

    # ---------- L3 结构指纹 ----------
    if platform in DISC_PLATFORMS:
        if hit := rc_hash_lookup(rc_hash(data)):           # PSX 系：只哈希启动 EXE
            emit(hit, confidence=MEDIUM, note="boot-EXE 相同，数据文件被改 ⇒ 汉化版")
            goto next
    blocks = block_hashes(data, 64*1024)
    if cands := blockdb.similar(blocks, threshold=0.6):
        emit(cands, confidence=MEDIUM, note="结构相似")

    # ---------- L4 文件名 ----------
    parsed = parse_name(filename, [TOSEC, NoIntro, GoodTools, ChineseConvention])
    #        → {title, region, lang, tr_group, 简繁, version}

    # ---------- L5 模型 ----------
    emit(model_guess(parsed, blocks_candidates), confidence=LOW, needs_review=True)

# 用户确认后：
on_user_confirm(file, identity):
    write_xattr(file, "user.scraper.rom_sha1", identity.base_sha1)
    cache.put(file_sha1, identity)
```

---

### D.8 针对「库里大部分是汉化版」的具体行动清单

按投入产出比排序【推断】：

| 优先级 | 行动 | 预期收益 |
|---|---|---|
| **1** | **引入 TOSEC DAT**（而不只是 No-Intro/Redump） | 立刻覆盖 FC 722 条、MD 86 条、GB 74 条、SFC 48 条、GBA 26 条中文汉化版 |
| **2** | **接入 Hasheous API 作在线兜底** | 零成本，实测能直接识别汉化 ROM 并给出 IGDB 元数据 |
| **3** | **同时算含头/去头两套哈希** | 解决 NES `.nes` vs `.unh`、SNES `.smc` 的全部匹配失败 |
| **4** | **导入 BizHawk 的 `gamedb_goodnes.txt` / `gamedb_sega_md.txt`** | 离线、纯文本、SHA-1 键，再补 646 条 NES + 172 条 MD 中文条目 |
| **5** | **实现中文命名规范解析（`(简)/(繁)/[汉化组]`）** | 即使识别不出原版，也能给出有意义的中文元数据 |
| **6** | **实现「序列号 → 游戏」反向索引** | 超越 RetroArch 的能力；对 GBA/NDS/PS1 汉化版尤其有效 |
| **7** | **实现 xattr / SQLite 持久化人工确认结果** | 边际收益随使用时间线性增长 |
| **8** | 尝试获取 RHDN 的 `romhacking.sql.zip`（6.8 MB，需 archive.org 登录） | 得到每个 hack 的基准 ROM CRC32/SHA-1 映射 |
| **9** | 实现 64 KB 分块哈希 + 相似度 | 兜底长尾；同时能可视化「哪些区域被汉化改动了」 |
| **10** | 对 PS1/PS2/PSP 实现 rcheevos 式「只哈希启动 EXE」 | 光盘类汉化如未改 EXE，可直接命中原版身份 |

> ⚠ **必须接受的现实**：GBA 与 NDS 的汉化版覆盖最差（TOSEC GBA 仅 26 条、NDS 无 DAT；BizHawk GBA/NDS gamedb 为 0；RA 中文仅 13 条）。而这恰恰是中文玩家库存量最大的两个平台。
> **【推断】** 对 GBA/NDS 汉化版，现实路径是：**L2（内部 game code）+ L4（文件名）+ L5（模型）+ 人工确认 + 持久化**，而不是指望上游数据库。

---

## 附录 · 一手来源清单

### 平台硬件文档 / 开发者 Wiki

| 平台 | URL |
|---|---|
| NES – iNES 格式 | https://www.nesdev.org/wiki/INES |
| NES – NES 2.0 格式 | https://www.nesdev.org/wiki/NES_2.0 |
| NES – NesCartDB XML DTD | https://archive.nes.science/nesdev-forums/f2/t5998.xhtml |
| SNES – ROM header | https://snes.nesdev.org/wiki/ROM_header |
| SNES – fullsnes（no$sns / Martin Korth） | https://problemkaputt.de/fullsnes.htm |
| SNES – SnesLab ROM Header | https://sneslab.net/wiki/SNES_ROM_Header |
| GB/GBC – Pan Docs 卡带头 | https://gbdev.io/pandocs/The_Cartridge_Header.html |
| GBA – GBATEK 卡带头 | https://problemkaputt.de/gbatek-gba-cartridge-header.htm |
| NDS – GBATEK 卡带头 | https://www.problemkaputt.de/gbatek-ds-cartridge-header.htm |
| DSi – GBATEK 卡带头 | https://www.problemkaputt.de/gbatek-dsi-cartridge-header.htm |
| NDS – GBATEK Icon/Title | https://www.problemkaputt.de/gbatek-ds-cartridge-icon-title.htm |
| NDS – GBATEK BIOS SWI 0Eh GetCRC16 | https://www.problemkaputt.de/gbatek-bios-misc-functions.htm |
| 3DS – NCSD | https://www.3dbrew.org/wiki/NCSD |
| 3DS – NCCH | https://www.3dbrew.org/wiki/NCCH |
| 3DS – CIA | https://www.3dbrew.org/wiki/CIA |
| 3DS – Titles / Title ID 结构 | https://www.3dbrew.org/wiki/Titles |
| 3DS – Title metadata (TMD) | https://www.3dbrew.org/wiki/Title_metadata |
| 3DS – Ticket | https://www.3dbrew.org/wiki/Ticket |
| N64 – ROM Header | https://n64brew.dev/wiki/ROM_Header |
| N64 – CIC-NUS | https://n64brew.dev/wiki/CIC-NUS |
| Mega Drive – ROM header | https://plutiedev.com/rom-header |
| SMS/GG – ROM Header | https://www.smspower.org/Development/ROMHeader |
| SMS/GG – Codemasters Header | https://www.smspower.org/Development/CodemastersHeader |
| SMS/GG – SDSC ROM Tag | https://www.smspower.org/Development/SDSCROMTagSpecification |
| SMS/GG – 校验和算法（Maxim's Header Reader） | https://github.com/maxim-zhao/sega8bitheaderreader |
| PC Engine – PCEdev Wiki（**无 ROM header 页**） | https://pce.nesdev.org/ |
| PS1 – psx-spx（nocash PSX 规范） | https://psx-spx.consoledev.net/cdromfileformats/ |
| PSP – PARAM.SFO（psdevwiki） | https://www.psdevwiki.com/psp/PARAM.SFO |
| PS3 – PARAM.SFO（psdevwiki） | https://www.psdevwiki.com/ps3/PARAM.SFO |
| Saturn – Sega 官方 IP 头源码 `SYS_ID.SRC` | https://antime.kapsi.fi/sega/docs.html （`IPGNUSRC.zip`） |
| Dreamcast – Marcus Comstedt, IP0000.BIN | http://mc.pp.se/dc/ip0000.bin.html |
| GameCube – YAGCD chapter 13 | https://www.gc-forever.com/yagcd/chap13.html |
| Wii – WiiBrew Wii Disc | https://wiibrew.org/wiki/Wii_Disc |

### 数据库项目

| 项目 | URL |
|---|---|
| No-Intro 门户 | https://no-intro.org |
| No-Intro DAT-o-MATIC | https://datomatic.no-intro.org |
| No-Intro DAT XSD v4 | https://datomatic.no-intro.org/stuff/schema_nointro_datfile_v4.xsd |
| No-Intro robots.txt | https://datomatic.no-intro.org/robots.txt |
| No-Intro 条款 | https://datomatic.no-intro.org/stuff/terms.txt |
| No-Intro header skipper（NES/FDS/LNX/A7800） | https://datomatic.no-intro.org/stuff/header_nes.zip 等 |
| No-Intro 命名规范 | https://wiki.no-intro.org/index.php?title=Naming_Convention |
| No-Intro File Convention | https://wiki.no-intro.org/index.php?title=File_Convention |
| No-Intro Aftermarket Guide | https://wiki.no-intro.org/index.php?title=Aftermarket_Guide |
| No-Intro Database Navigation Guide | https://wiki.no-intro.org/index.php?title=Database_Navigation_Guide |
| Redump 下载 | http://redump.org/downloads/ |
| Redump wiki（**已迁移**） | https://wiki.redump.info/ |
| Redump PlayStation Guide | https://wiki.redump.info/index.php?title=Sony_PlayStation_Guide |
| Redump PlayStation Serials | https://wiki.redump.info/index.php?title=Sony_PlayStation_Serials |
| Redump PSP Dumping Guide | https://wiki.redump.info/index.php?title=Sony_PlayStation_Portable_Dumping_Guide |
| Redump Saturn Guide | https://wiki.redump.info/index.php?title=Sega_Saturn_Guide |
| Redump Dreamcast Guide | https://wiki.redump.info/index.php?title=Sega_Dreamcast_Guide |
| Redump GameCube / Wii Guide | https://wiki.redump.info/index.php?title=Nintendo_GameCube_Guide |
| **TOSEC 命名规范** | https://www.tosecdev.org/tosec-naming-convention |
| TOSEC 下载 | https://www.tosecdev.org/downloads |
| Logiqx `datafile.dtd` v1.5（站点已下线，GitHub 镜像） | https://github.com/Logiqx/logiqx-www/blob/master/Dats/datafile.dtd |
| clrmamepro header 规范 `xmlheaders.txt` | https://mamedev.emulab.it/clrmamepro/docs/xmlheaders.txt |
| clrmamepro header 规范（ckmame 保存的副本） | https://github.com/nih-at/ckmame/blob/main/docs/xmlheaders.txt |
| MAME 官方文档 – About ROM Sets | https://docs.mamedev.org/usingmame/aboutromsets.html |
| MAME chdman 文档 | https://docs.mamedev.org/tools/chdman.html |
| MAME `hash/softwarelist.dtd` | https://raw.githubusercontent.com/mamedev/mame/master/hash/softwarelist.dtd |
| libretro-database | https://github.com/libretro/libretro-database |
| RetroArch `.rdb` 格式说明 | https://github.com/libretro/RetroArch/blob/master/libretro-db/README.md |
| libretro 播放列表/扫描文档 | https://docs.libretro.com/guides/roms-playlists-thumbnails/ |
| libretro-super 构建脚本（主键映射） | https://github.com/libretro/libretro-super/blob/master/libretro-build-database.sh |
| **Hasheous（免费哈希查询 API）** | https://github.com/gaseous-project/hasheous |
| RetroAchievements Hash Labels 指南 | https://docs.retroachievements.org/guidelines/content/hash-labels.html |
| RetroAchievements ROM hacks 政策 | https://docs.retroachievements.org/guidelines/content/achievements-for-rom-hacks.html |
| RetroAchievements `API_GetGameHashes` | https://api-docs.retroachievements.org/v1/get-game-hashes.html |
| RAPatches（补丁库） | https://github.com/RetroAchievements/RAPatches |

### 模拟器 / 工具源码

| 项目 | URL / 关键文件 |
|---|---|
| Mesen2（NES 头解析、`PrgChrCrc32`） | https://github.com/SourMesen/Mesen2 — `Core/NES/NesHeader.cpp`、`Core/NES/Loaders/{RomLoader,iNesLoader}.cpp` |
| bsnes（SNES 头定位打分） | https://github.com/bsnes-emu/bsnes — `bsnes/heuristics/super-famicom.cpp` |
| Snes9x（`ScoreHiROM`/`ScoreLoROM`） | https://github.com/snes9xgit/snes9x — `memmap.cpp` |
| mGBA（GB/GBA 文件判定） | https://github.com/mgba-emu/mgba — `src/gb/gb.c`、`src/gba/gba.c` |
| devkitPro ndstool（NDS CRC16 参考实现） | https://github.com/devkitPro/ndstool — `source/crc.h`、`source/crc.cpp` |
| devkitPro `gbafix`（GBA 头修复） | https://github.com/devkitPro/gba-tools/blob/master/src/gbafix.c |
| mupen64plus-core（N64 字节序、CIC 判定） | https://github.com/mupen64plus/mupen64plus-core — `src/main/rom.c`、`src/device/pif/cic.c` |
| Genesis Plus GX（MD/Sega CD） | https://github.com/ekeeke/Genesis-Plus-GX — `core/loadrom.c`、`core/cd_hw/cdd.c` |
| PicoDrive（`.smd` / Sega CD 判定） | https://github.com/irixxxx/picodrive — `pico/media.c` |
| MEKA（SMS/GG 的 CRC32 + MekaCRC） | https://github.com/ocornut/meka — `meka/srcs/checksum.cpp` |
| Beetle-PCE / Mednafen（PCE 识别） | https://github.com/libretro/beetle-pce-libretro — `mednafen/pce/huc.cpp`、`pce.cpp` |
| Geargrafx（PCE copier 头） | https://github.com/drhelius/Geargrafx — `src/media.cpp` |
| Yabause（Saturn IP 解析） | https://github.com/Yabause/yabause — `yabause/src/cs2.c` |
| DuckStation（PS1 序列号与哈希） | https://github.com/stenzek/duckstation — `src/core/system.cpp`、`src/core/game_database.cpp`、`src/util/cd_image_pbp.cpp` |
| PCSX2（PS2 序列号与 ELF「CRC」） | https://github.com/PCSX2/pcsx2 — `pcsx2/CDVD/CDVD.cpp`、`pcsx2/Elfheader.cpp`、`pcsx2/GameDatabase.cpp` |
| PPSSPP（PARAM.SFO / CSO） | https://github.com/hrydgard/ppsspp — `Core/ELF/ParamSFO.cpp`、`Core/FileSystems/BlockDevices.cpp` |
| pspsdk `mksfo.c`（PSP SFO 字段尺寸） | https://github.com/pspdev/pspsdk/blob/master/tools/mksfo.c |
| Dolphin（GC/Wii、WIA/RVZ 规范） | https://github.com/dolphin-emu/dolphin — `docs/WiaAndRvz.md`、`Source/Core/DiscIO/{DiscUtils.h,Enums.cpp,WbfsBlob.cpp}` |
| MAME（audit / infoxml / chd） | https://github.com/mamedev/mame — `src/frontend/mame/{audit.cpp,infoxml.cpp}`、`src/lib/util/chd.{h,cpp}`、`src/tools/chdman.cpp` |
| RetroArch（扫描器） | https://github.com/libretro/RetroArch — `tasks/task_database.c`、`tasks/task_database_cue.c`、`libretro-db/libretrodb.c`、`libretro-db/c_converter.c` |
| rcheevos（逐平台自定义哈希） | https://github.com/RetroAchievements/rcheevos — `src/rhash/hash_rom.c`、`src/rhash/hash_disc.c` |
| RomVault / RVWorld | https://github.com/RomVault/RVWorld — `FileScanner/FileHeaders.cs`、`FileScanner/ScannedFile.cs`、`RomVaultCore/Scanner/Compare.cs` |
| RomCenter 插件源码 | https://github.com/ebolefeysot/RomcenterPlugins — `Goodxxx/fmt/*.cpp` |
| igir（ROM 头检测表） | https://github.com/emmercm/igir — `src/models/files/romHeader.ts`；文档 https://igir.io/roms/headers/ |
| BizHawk gamedb（GoodTools 派生） | https://github.com/TASEmulators/BizHawk/tree/master/Assets/gamedb |

### 补丁格式与 ROM hack 相关

| 项目 | URL |
|---|---|
| IPS 规范（Zerosoft） | https://zerosoft.zophar.net/ips.php |
| Flips（IPS/UPS/BPS 参考实现） | https://github.com/Alcaro/Flips — `libips.cpp`、`libups.cpp`、`libbps.cpp` |
| BPS 规范（byuu） | https://github.com/Alcaro/Flips/blob/master/bps_spec.md |
| VCDIFF RFC 3284 | https://datatracker.ietf.org/doc/html/rfc3284 |
| xdelta3 | https://github.com/jmacd/xdelta |
| xdelta3 armor 模式文档 | https://jmacd.github.io/xdelta/armor/ |
| APS (N64) 格式 | https://github.com/btimofeev/UniPatcher/wiki/APS-(N64) |
| APS (GBA) 实现 | https://github.com/marcrobledo/RomPatcher.js/blob/master/rom-patcher-js/modules/RomPatcher.format.aps_gba.js |
| RUP / NINJA2 实现 | https://github.com/marcrobledo/RomPatcher.js/blob/master/rom-patcher-js/modules/RomPatcher.format.rup.js |
| PPF 3.0 规范（Paradox） | https://github.com/meunierd/ppf/blob/master/ppfdev/PPF3.txt |
| romhacking.net 只读公告 | https://www.romhacking.net/news/3074/ |
| romhacking.net 全库导出（archive.org，需登录） | https://archive.org/details/romhacking.net-20240801 |
| rhdndat（SHA-1 + `.rxdelta` 反向补丁） | https://github.com/i30817/rhdndat |
| SMDB / Hardware Target Game Database | https://github.com/frederic-mahe/Hardware-Target-Game-Database |
| GoodTools 命名码 `GoodCodes.txt` | https://archive.org/download/SegaGameGearCollectionByGhostware/GoodCodes.txt |
| archive.org `En-ROMs`（英化 DAT，受限） | https://archive.org/details/En-ROMs |
| archive.org `hackset` | https://archive.org/details/hackset |
| archive.org `rom-hack-patch-archive` | https://archive.org/details/rom-hack-patch-archive |
| FuSoYa's Lunar Expand（SNES 扩容） | https://fusoya.eludevisibility.org/le/index.html |
| uCON64 | https://ucon64.sourceforge.io/ |

### 中文相关来源

| 来源 | URL |
|---|---|
| **壬天堂世界「中文ROM补完计划」** | https://bbs.newwise.com/forum.php?action=printable&mod=viewthread&tid=934484 |
| 同上（archiver 视图，便于抓取） | https://bbs.newwise.com/archiver/tid-934484.html |
| yingw/rom-name-cn（中文译名对照，非哈希库） | https://github.com/yingw/rom-name-cn |
| archive.org NDS 中文非官方汉化合集（68 个，无哈希清单） | https://archive.org/details/nintendodschineseunofficialtranslations |

### 其他工具与规范

| 项目 | URL |
|---|---|
| PKWARE ZIP APPNOTE 6.3.9（中央目录 CRC32） | https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.9.TXT |
| ssdeep（CTPH 模糊哈希） | https://ssdeep-project.github.io/ssdeep/ |
| ssdeep 源码（块大小/比较约束） | https://github.com/ssdeep-project/ssdeep — `fuzzy.c`、`fuzzy.h` |
| TLSH | https://github.com/trendmicro/tlsh |
| ScreenScraper API | https://www.screenscraper.fr/webapi2.php |
| Skyscraper（刮削器，多源） | https://github.com/Gemba/skyscraper |
| RomM（自托管 ROM 管理器） | https://github.com/rommapp/romm |
| datoso（DAT 下载/整理） | https://github.com/laromicas/datoso |
| oxyromon | https://github.com/alucryd/oxyromon |
| auto-datfile-generator（No-Intro 每日重打包） | https://github.com/hugo19941994/auto-datfile-generator |

---

## 调研方法与局限

### 本文的证据等级

| 标记 | 含义 | 占比 |
|---|---|---|
| 未标记 / **【事实】** | 有一手来源 URL 直接支撑（规范原文、DTD/XSD、源码逐字） | 主体 |
| **【实测】** | 本次调研直接发起 HTTP 请求 / 下载并解析数据文件所得，附可复现命令 | 约 20 处 |
| **【推断】** | 基于事实的推理，**未经来源直接证实** | 已逐条标注 |

### 未能取得一手确认的事项

1. **汉化版是否保留内部序列号** —— 未找到任何一手来源做过系统性统计。本文 D.3.1 表中的「通常保留」全部是推断。
2. **SNES 校验和计算时校验字段的处理** —— snes.nesdev 说「清零」，fullsnes 说「当作 `FF FF 00 00`」，两者差 `0x1FE`。本文倾向 fullsnes（自洽推导见 A.2.4），但建议用已知良品 ROM 实测确认。
3. **Sega ST-040「Disc Format Standards Specification Sheet」** —— 是扫描版 PDF（CCITTFax，无文字层），无法提取。已用 Sega 自己的 `SYS_ID.SRC` 汇编源码替代（来源等级更高）。
4. **NKit 容器格式** —— 未找到权威规范；它与 Redump SHA-1 的关系仅为推断。
5. **PSP 的 PVD `0x373` → ISO9660「Application Used」字段映射** —— Redump 指南给了十六进制转储但未点名该字段，映射关系为推断。
6. **`Logiqx detector.dtd`** —— 该 URL 从未被 Wayback 归档，`logiqx.com` 整站已下线。唯一权威规范是 clrmamepro 作者的 `xmlheaders.txt`。
7. **32X 的头部判据** —— plutiedev 文档了 `0x100 == "SEGA 32X"`，但未见有模拟器据此识别（PicoDrive 用扩展名）。
8. **PC Engine CD 的卷头 magic** —— 与 Sega CD 的 `"SEGADISCSYSTEM"` 不同，本次调研在 PCEdev wiki 上未找到任何等价物的一手文档。

### 一处需要说明的过程问题

本次调研通过并行子代理收集材料。其中一个子代理在其中间报告里**虚构了几份 clrmamepro skipper XML 的内容**（`nes.xml`、`a7800.xml`、`fds.xml`、`lynx.xml`、`psid.xml`），随后自行发现并撤回。本文**已剔除全部虚构内容**，并改为：
- 直接下载 `https://datomatic.no-intro.org/stuff/header_*.zip` 取得 **No-Intro 官方 skipper 的真实原文**（见 B.5.1b，含实测 HTTP 200 与解压结果）
- 直接下载并逐行核对 `https://mamedev.emulab.it/clrmamepro/docs/xmlheaders.txt`（HTTP 200，14086 字节）作为 skipper 格式规范
- 直接下载 `https://raw.githubusercontent.com/ebolefeysot/RomcenterPlugins/master/Goodxxx/fmt/*.cpp` 核实 RomCenter 插件实现

其余章节的关键结论亦均经交叉验证（如 No-Intro NES 双 DAT 的条目数、SNES DAT 无 `.smc` 记录、TOSEC 的 `[tr zh]` 计数、Hasheous API 的实际返回，均为本文作者亲自执行的请求）。
