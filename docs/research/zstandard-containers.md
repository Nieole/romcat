# Zstandard（`.zst` / `.tar.zst`）作为 ROM 库归档格式的识别可行性

> **调研日期**：2026-08-31
> **来源约束**：仅采用一手来源 —— zstd 官方格式规范原文（v0.4.5, 2026-05-14）与 facebook/zstd 仓库源码、RFC 8878、POSIX.1-2024 pax 规范、GNU tar 手册、各 crate 的 crates.io/仓库源码、RetroArch / Vita3K / PCSX2 / DuckStation 源码。**本文所有头部字段名、位偏移与字节偏移都来自实际抓取到的规范文本或源码，未做任何推测性补全。**
> **标注约定**：
> - **【事实】**（不加标记的正文默认为此类）= 有一手来源 URL 直接支撑
> - **【实测】** = 本次调研在本机（macOS 24.6.0 / Apple Silicon / zstd CLI v1.5.7 / bsdtar 3.5.3 + libarchive 3.7.4）上跑出来的结果，附命令与输出
> - **【推断】** = 本文基于事实的推理，**未经来源直接证实**
> - **【未找到权威来源】** = 明确标注，不猜
>
> **相关文档**：ZIP / 7z / RAR 的「零解压读 CRC-32」结论见 [`containers-and-compressed-images.md`](./containers-and-compressed-images.md) 第 1 部分；本文是该文第 1 部分的补充章，专门回答「zst 能不能进同一条快路」。

---

## 目录

- [第 0 章 · 核心结论速览](#第-0-章--核心结论速览)
- [第 1 部分 · zstd 帧格式本身](#第-1-部分--zstd-帧格式本身)
  - [1.1 帧结构总览](#11-帧结构总览)
  - [1.2 `Frame_Header` 的完整字段表](#12-frame_header-的完整字段表)
  - [1.3 ⭐ `Frame_Content_Size`：可选，而且在 `.tar.zst` 里经常缺失](#13--frame_content_size可选而且在-tarzst-里经常缺失)
  - [1.4 `Content_Checksum`：XXH64 低 32 位，校验未压缩内容](#14-content_checksumxxh64-低-32-位校验未压缩内容)
  - [1.5 ⭐ 关键否定结论：zstd 格式里根本没有 CRC-32](#15--关键否定结论zstd-格式里根本没有-crc-32)
  - [1.6 Skippable frames 与 seekable format](#16-skippable-frames-与-seekable-format)
  - [1.7 字典压缩：检测方法与后果](#17-字典压缩检测方法与后果)
- [第 2 部分 · `.tar.zst` 双层结构（重点）](#第-2-部分--tarzst-双层结构重点)
  - [2.1 tar 头部结构与精确偏移](#21-tar-头部结构与精确偏移)
  - [2.2 tar 的 `chksum` 是什么算法](#22-tar-的-chksum-是什么算法)
  - [2.3 ⭐ 读出第一个条目的名字与大小需要解压多少字节](#23--读出第一个条目的名字与大小需要解压多少字节)
  - [2.4 ⭐ 列出全部条目必须解压整个流](#24--列出全部条目必须解压整个流)
  - [2.5 pax header 陷阱：第一个 512 字节块往往不是真条目](#25-pax-header-陷阱第一个-512-字节块往往不是真条目)
  - [2.6 单成员归档的零成本判定技巧](#26-单成员归档的零成本判定技巧)
  - [2.7 有没有「不解压就列出 tar 内容」的办法](#27-有没有不解压就列出-tar-内容的办法)
- [第 3 部分 · 性能估算](#第-3-部分--性能估算)
- [第 4 部分 · Rust 生态](#第-4-部分--rust-生态)
- [第 5 部分 · 生态支持](#第-5-部分--生态支持)
- [第 6 部分 · 对识别管线的可执行建议](#第-6-部分--对识别管线的可执行建议)
- [附录 · 一手来源清单](#附录--一手来源清单)

---

## 第 0 章 · 核心结论速览

1. **`.zst` 不能进「零解压读 CRC-32」那条快路，而且不是「差一点」，是结构上完全没有这个字段。** zstd 帧格式规范全文（v0.4.5）**零次**出现 "CRC"，RFC 8878 同样零次。zstd 唯一的完整性字段是 `Content_Checksum` = **XXH64 的低 32 位**。DAT 用 CRC-32，两者无法互换、无法转换。

2. **更糟的是：`.zst` 连「零解压读未压缩大小」都不保证。** `Frame_Content_Size` 是**可选**字段。**【实测】`tar --zstd -cf` 产生的 `.tar.zst` 里，`Frame_Content_Size` 与 `Content_Checksum` 双双缺席**（`Frame_Header_Descriptor = 0x00`，帧头只有 6 字节）。也就是说，对这类文件，零解压能拿到的信息只有「这是一个 zstd 帧」和 `Window_Size`，**连原始大小都没有**。

3. **`.zst` 在结构上就不可能有内部文件清单** —— 它是**单流压缩器**，不是归档器，没有「成员」概念。`.tar.zst` 里的文件清单在 tar 层，而 tar 的条目头**散布在整个流中**，且 zstd 解压器不可 `Seek`。**因此列出 `.tar.zst` 的完整清单 = 解压整个流。这是格式决定的，无解。**

4. **但「读第一个条目」极便宜，而且有精确上界。** zstd 的 `Block_Maximum_Size = min(Window_Size, 128 KiB)`，要拿到流开头的 tar header，只需读 `帧头(≤14B) + 块头(3B) + 第一个块(≤128 KiB)`，**约 131 KB 封顶**。**【实测】**在一个 13.5 MB 的 `.tar.zst` 上，读到第 **95,404** 字节（帧头 6 + 块头 3 + 第一块 95,395）时一次吐出 **131,072 字节**明文；少读 1 字节则吐出 0 字节 —— 解压的最小粒度就是一个 block，边界精确可预测。

5. **tar 的 `chksum` 字段是「512 字节头部的无符号字节简单求和」，以八进制 ASCII 存储 —— 对 DAT 匹配毫无用处。**【实测】已用该算法复算并与存储值比对通过。它校验的是**头部本身**（防止头部损坏），不是文件内容。

6. **⭐ pax 陷阱**：bsdtar / libarchive 默认写 **pax（POSIX.1-2001）**格式，**【实测】第一个 512 字节块是 `PaxHeader/file1.bin`（`typeflag='x'`）伪条目，真正的 `file1.bin` 条目在偏移 1024 处**。「读前 512 字节拿文件名」的朴素实现会拿到垃圾。必须按 tar 语义走条目链（幸好都落在第一个 128 KiB 块内）。

7. **⭐ 有一个零成本技巧可以确认「这是单成员归档」**：若 `Frame_Content_Size` 存在（记为 `FCS`，即整个 tar 的大小），且第一个条目声明大小为 `S`，则当 `512 + ceil(S/512)*512 + 1024 == FCS` 时，该 tar **必然只有一个成员**，无需任何额外解压即可确认。【推断，但为算术必然】

8. **seekable zstd 在本库里几乎肯定不存在，且判定是 O(1) 的**：seekable 格式要求文件**最后 4 字节**为 `Seekable_Magic_Number 0x8F92EAB1`（磁盘上小端 `B1 EA 92 8F`）。ratarmount 官方文档明确指出：**「两种压缩器都只写出单个 frame 和/或 block，使得该特性无法使用」** —— 标准 `zstd` CLI 与 `tar --zstd` 产生的都是**单帧**文件，不可随机访问。只有 `pzstd` / `t2sz` / `zeekstd` 才产生多帧。

9. **性能瓶颈在磁盘侧，不在 zstd 侧，而且差一个数量级。** 官方 benchmark：zstd 1.5.7 `-1` 解压 **1550 MB/s**，且规范原文强调「**解压速度在所有压缩级别下基本保持不变**」。**【实测】**本机单线程 1073 MB/s。而外置机械盘 100–200 MB/s。**全量解压 2.50 TiB 的耗时 ≈ 纯读盘耗时 ≈ 3.8–7.6 小时；zstd 解压本身只占其中约 0.6–0.8 小时，且可与 I/O 完全重叠。CPU 不是问题。**

10. **⭐ 生态支持是这件事的真正杀手，但方向和预期相反。**
    - **RetroArch 确实支持 `.zst`**（源码确认 `file_archive_get_file_backend()` 分派 `"zst"` → `zstd_backend`），但它把 `.zst` 当**单文件容器**，**内部文件名 = 外层文件名去掉 `.zst`**（源码注释原文：*"Derive the inner filename from the .zst path by stripping the .zst extension"*）。RetroArch 仓库里**根本不存在 tar 后端**（`libretro-common/file/` 只有 `archive_file_7z.c` / `archive_file_zlib.c` / `archive_file_zstd.c`）。**所以 `.tar.zst` 在 RetroArch 里会被解成一个裸 `.tar` 字节流丢给核心，等于没解。**
    - **RetroArch 的 zstd 后端硬性要求 `Frame_Content_Size` 存在**，源码注释原文：*"A frame that does not state its size is no use here"*，拿不到就 `return -1`。**结论 2 里那种 `tar --zstd` 产物会被 RetroArch 直接拒绝。**
    - **RetroArch 自己也拿不到 zst 的 CRC**：`zstd_parse_file_iterate_step()` 里写死 `userdata->crc = 0;`，而 `file_archive_get_file_crc32_and_size()` 直接 `return userdata.crc;`。**RetroArch 官方实现自己就承认这条路走不通。**
    - **Vita3K 完全不支持 `.zst`**：源码里安装归档的扩展名判断是 `ext == ".zip" || ext == ".vpk" || ext == ".vci"`，Qt 文件过滤器同样只有这三种。**库里的 `PSV/VPK zst/*.vpk.tar.zst` 与 `*.tar.zst` 在 Vita3K 里一个都装不了。**
    - **PCSX2 / DuckStation / PPSSPP 的 `.zst` 都不是给 ROM 用的**：PCSX2 的 `.zst` 只出现在 **GS dump**（`.gs.zst`）与存档压缩，光盘过滤器是 `*.bin *.iso *.cue *.mdf *.chd *.cso *.zso *.gz *.elf *.irx`，**没有 zst**；DuckStation 的 `.zst` 只用于 `.psxgpu.zst` GPU dump 与存档压缩；PPSSPP 仓库搜 `.zst` **零命中**。

11. **⭐ `.tar.zst` 不是 ROM 分发圈的通行惯例，更像是本库的本地重打包选择。** 【实测】archive.org 上两个大型 PSV 合集 —— `ps-vita-game-dumps`（74 个文件 / 169.2 GiB）与 `sony-playstation-vita-usa-full-set-nonpdrm-format`（1022 个文件 / 786.8 GiB）—— **扩展名 100% 是 `.zip`，`.tar.zst` 出现次数为 0**。结合库里的中文路径（`《月姬plus+disc》-PSV移植（unity重置版）`）以及该盘根目录存在 `peazip-9.8.0.WIN64.exe`（PeaZip 支持 `.tar.zst`），【推断】这批 `.tar.zst` 来自中文重打包渠道或用户自行转换，而非上游标准分发格式。

12. **最终取舍：选 (a)，在工具里支持 zst；但不要把它当成 zip/7z 的等价物，要当成「必须全量解压才能识别的慢路」。** 转换 2.50 TiB 到 7z 需要 **15–44 小时**（I/O 与 LZMA2 CPU 双向受限），而全量解压识别只需 **3.8–7.6 小时且只做一次**（结果按 `路径+size+mtime` 缓存 CRC 后永不重做）。**转换严格劣于直接支持：它同样要把 2.50 TiB 读一遍，还额外付出压缩 CPU、等量写回、以及销毁原文件的风险。** —— **但这是「识别」维度的结论；「可玩性」维度的结论相反，见第 6 部分。**

---

## 第 1 部分 · zstd 帧格式本身

> 来源：<https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md>（本次抓取版本 **0.4.5 (2026-05-14)**）与 <https://www.rfc-editor.org/rfc/rfc8878.html>（*Zstandard Compression and the 'application/zstd' Media Type*，2021-02，**Informational**，非标准轨）。
>
> 二者字段定义一致；facebook/zstd 仓库内的 `.md` 是持续演进的权威版本，RFC 8878 是 2021 年的快照。**本文以仓库版为准，RFC 8878 作为交叉印证。**

### 1.1 帧结构总览

规范原文的帧结构表：

| `Magic_Number` | `Frame_Header` | `Data_Block` | [More data blocks] | [`Content_Checksum`] |
|:---:|:---:|:---:|---|:---:|
| 4 bytes | **2-14 bytes** | n bytes | | **0-4 bytes** |

- `Magic_Number`：4 字节小端，值 **`0xFD2FB528`**（磁盘上字节序为 `28 B5 2F FD`）。
- zstd 定义了**两种**帧：**Zstandard frames**（含压缩数据）与 **Skippable frames**（用户元数据）。一个文件可以是多个帧的拼接，「多个拼接帧的解压结果 = 各帧解压结果的拼接」。

规范里有一句必须引用的话（Introduction 节）：

> **"The data format defined by this specification does not attempt to allow random access to compressed data."**
>
> （本规范定义的数据格式**不试图**支持对压缩数据的随机访问。）

这一句直接否决了「像 ZIP 中央目录那样跳着读」的任何设想。

### 1.2 `Frame_Header` 的完整字段表

| `Frame_Header_Descriptor` | [`Window_Descriptor`] | [`Dictionary_ID`] | [`Frame_Content_Size`] |
|---|---|---|---|
| 1 byte | 0-1 byte | 0-4 bytes | 0-8 bytes |

**`Frame_Header_Descriptor`（第 1 字节）的位定义**（规范原表，bit 7 为最高位）：

| Bit number | Field name |
|---|---|
| 7-6 | `Frame_Content_Size_flag` |
| 5 | `Single_Segment_flag` |
| 4 | `Unused_bit` |
| 3 | `Reserved_bit` |
| 2 | `Content_Checksum_flag` |
| 1-0 | `Dictionary_ID_flag` |

**`FCS_Field_Size` 由 `Frame_Content_Size_flag` 决定**：

| `Flag_Value` | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| `FCS_Field_Size` | **0 或 1** | 2 | 4 | 8 |

> `Flag_Value = 0` 时：若 `Single_Segment_flag` 置位则 `FCS_Field_Size = 1`；**否则为 0，即 `Frame_Content_Size` 不提供**。

**`DID_Field_Size` 由 `Dictionary_ID_flag` 决定**：

| `Flag_Value` | 0 | 1 | 2 | 3 |
|---|---|---|---|---|
| `DID_Field_Size` | 0 | 1 | 2 | 4 |

**`Window_Descriptor`（1 字节，`Single_Segment_flag` 置位时不存在）**：

| Bit numbers | 7-3 | 2-0 |
|---|---|---|
| Field name | `Exponent` | `Mantissa` |

```
windowLog   = 10 + Exponent;
windowBase  = 1 << windowLog;
windowAdd   = (windowBase / 8) * Mantissa;
Window_Size = windowBase + windowAdd;
```
最小 1 KB，最大 `(1<<41) + 7*(1<<38)` = 3.75 TB。

**因此：只读文件前 14 字节，就能零解压确定性地得到**：
- 是不是 zstd 帧（magic）
- 帧头总长（解 `Frame_Header_Descriptor` 一个字节即可，规范原文：*"Decoding this byte is enough to tell the size of `Frame_Header`"*）
- 是否带内容校验和
- `Window_Size`（→ 由此得到 `Block_Maximum_Size`，即解压粒度）
- 是否需要外部字典（`Dictionary_ID`）
- **如果存在**：未压缩总大小

### 1.3 ⭐ `Frame_Content_Size`：可选，而且在 `.tar.zst` 里经常缺失

规范原文：

> **"This is the original (uncompressed) size. This information is optional."**

`zstd.h` 里对应的 API 文档写得更直白：

> `@retval ZSTD_CONTENTSIZE_UNKNOWN` **The frame does not encode a decompressed size (typical for streaming).**

zstd CLI 手册里 `--content-size` 默认开启：

> `--[no-]content-size`: enable / disable whether or not the original size of the file is placed in the header of the compressed file. **The default option is `--content-size`**

**但「默认开启」只在 zstd 能事先知道大小时有效** —— 即输入是一个常规文件。输入来自管道时，大小未知，必须显式用 `--stream-size=#` 告知：

> `--stream-size=#`: **Sets the pledged source size of input coming from a stream.** This value must be exact, as it will be included in the produced frame header.

**而 `.tar.zst` 的典型生成方式恰恰是管道。**

#### 【实测】三种生成方式的帧头差异

```bash
# 约 17.2 MiB 的 tar，三种做法
zstd -f a.tar -o a.tar.zst        # A: 输入是文件，大小已知
tar --zstd -cf b.tar.zst src      # B: tar 内建 zstd（走流）
cat a.tar | zstd > c.tar.zst      # C: 显式管道
```

`zstd -l` 输出：

```
Frames  Skips  Compressed  Uncompressed  Ratio  Check  Filename
     1      0    12.9 MiB      17.2 MiB  1.333  XXH64  a.tar.zst
     1      0    12.9 MiB                        None  b.tar.zst      <-- 大小与校验和双双缺失
     1      0    12.9 MiB                       XXH64  c.tar.zst      <-- 大小缺失
```

用自写解析器逐字段拆帧头（`Frame_Header_Descriptor` 的位含义完全按 1.2 的规范表）：

```
### a.tar.zst
  Frame_Header_Descriptor = 0x84 -> FCS_flag=2 Single_Segment=0 Content_Checksum=1 DID_flag=0
  Window_Descriptor = 0x58 -> Window_Size = 2097152 B (2.00 MiB)
  ** Frame_Content_Size = 18011648 B (17.18 MiB) — 零解压可得原始大小 **
  帧头总长 = 10 字节

### b.tar.zst   (tar --zstd)
  Frame_Header_Descriptor = 0x00 -> FCS_flag=0 Single_Segment=0 Content_Checksum=0 DID_flag=0
  Window_Descriptor = 0x58 -> Window_Size = 2097152 B (2.00 MiB)
  ** Frame_Content_Size: 不存在 (未知) **
  帧头总长 = 6 字节

### c.tar.zst   (cat | zstd)
  Frame_Header_Descriptor = 0x04 -> FCS_flag=0 Single_Segment=0 Content_Checksum=1 DID_flag=0
  ** Frame_Content_Size: 不存在 (未知) **
  帧头总长 = 6 字节
```

> **【实测结论】`tar --zstd`（本机为 bsdtar 3.5.3 / libarchive 3.7.4）产生的 `.tar.zst` 既没有原始大小、也没有校验和。** 这是本次调研最重要的意外发现之一：不能假设 `.zst` 一定带 `Frame_Content_Size`，**必须在代码里处理 `None` 分支**。
>
> **【推断】** GNU tar 的 `--zstd` 是把数据通过管道喂给外部 `zstd` 程序（而非库内调用），因此行为应等同于上表的 C 行（无 FCS，有 XXH64）。本机为 macOS，无 GNU tar，**未直接验证**。
>
> **【未找到权威来源】** 库中这 2,685 个 `.zst` 文件里各种帧头组合的实际占比。**这必须实测** —— 好消息是每个文件只需读前 14 字节，2,685 个文件跑完不到一分钟，见第 6 部分的 Step 0。

### 1.4 `Content_Checksum`：XXH64 低 32 位，校验未压缩内容

规范原文：

> **"An optional 32-bit checksum, only present if `Content_Checksum_flag` is set. The content checksum is the result of [xxh64() hash function] digesting the original (decoded) data as input, and a seed of zero. The low 4 bytes of the checksum are stored in little-endian format."**

逐条拆解问题里问的三点：

| 问题 | 答案 |
|---|---|
| 什么算法？ | **XXH64**（seed = 0），**只存低 4 字节** |
| 是否可选？ | **可选**。由 `Frame_Header_Descriptor` bit 2 (`Content_Checksum_flag`) 决定。zstd CLI 默认 `-C`/`--check` 开启，但 **【实测】libarchive 的 `tar --zstd` 不开** |
| 校验的是压缩数据还是未压缩内容？ | **未压缩内容**（规范原文 *"digesting the original (decoded) data"`*） |

**它能不能替代 CRC-32 用于 DAT 匹配？——不能，而且是三重不能：**

1. **算法不同**：DAT 里是 CRC-32，这里是 XXH64 截断。两者不存在任何转换关系。
2. **作用域不同**：即使算法对得上，这个校验和覆盖的是**整个 tar 流**（含 512 字节对齐填充、tar header、结束的 1024 字节零块），**而不是内部某个 ROM 文件的内容**。DAT 的 `<rom crc=...>` 匹配的是 ROM 文件本身。
3. **不保证存在**：见上表。

### 1.5 ⭐ 关键否定结论：zstd 格式里根本没有 CRC-32

这一条做了穷尽式确认：

```bash
$ grep -ni "crc" zstd_compression_format.md
（无输出）
```

**zstd 帧格式规范 v0.4.5 全文 1777 行，"CRC" 出现 0 次。** RFC 8878 同样确认不含 CRC-32（本次通过全文检索确认）。

> **结论：`.zst` 无论如何都进不了「零解压读 CRC-32」这条路。这不是实现问题、不是工具问题，是格式里就没有这个字段。**

对比表（与 [`containers-and-compressed-images.md`](./containers-and-compressed-images.md) 第 1 部分的结论并列）：

| 容器 | 零解压可得内部文件名 | 零解压可得未压缩大小 | 零解压可得 CRC-32 | 可否直接进 DAT 快路 |
|---|:---:|:---:|:---:|:---:|
| **ZIP** | ✅ 中央目录 | ✅ | ✅ | ✅ |
| **7z** | ✅ 头部 | ✅ | ✅ | ✅ |
| **RAR4/5** | ✅ 文件头 | ✅ | ✅ | ✅ |
| **裸 `.zst`** | ❌ 无成员概念 | ⚠️ 仅当 `Frame_Content_Size` 存在 | ❌ **格式里没有** | ❌ |
| **`.tar.zst`** | ⚠️ 需解压第一个 block（≤128 KiB）；列全清单需解压全流 | ⚠️ 同上（tar header 的 `size` 字段） | ❌ **两层都没有** | ❌ |

### 1.6 Skippable frames 与 seekable format

#### Skippable frames（基础规范）

| `Magic_Number` | `Frame_Size` | `User_Data` |
|:---:|:---:|:---:|
| 4 bytes | 4 bytes | n bytes |

- `Magic_Number` = **`0x184D2A5?`**，规范原文：*"which means any value from 0x184D2A50 to 0x184D2A5F. All 16 values are valid"*。
- `Frame_Size` = 后续 `User_Data` 的字节数（4 字节小端，**不含** magic 与本字段）。
- 对解码器而言就是「跳过，忽略内容」。

**这意味着可跳过帧是一个 O(1) 可跳跃的定长结构** —— seekable format 正是利用了这一点。

#### Seekable format（`contrib/seekable_format`，版本 0.1.0 / 2017-11-04）

来源：<https://github.com/facebook/zstd/blob/dev/contrib/seekable_format/zstd_seekable_compression_format.md>

结构：一串独立压缩的帧 + **文件末尾一个装着 seek table 的 skippable frame**。

**`Seek_Table_Footer`（位于文件最末尾，共 9 字节）**：

| `Number_Of_Frames` | `Seek_Table_Descriptor` | `Seekable_Magic_Number` |
|---|---|---|
| 4 bytes | 1 byte | 4 bytes |

规范原文：

> `Seekable_Magic_Number` — **Value : 0x8F92EAB1. This value must be the last bytes present in the compressed file so that decoders can efficiently find it and determine if there is an actual seek table present.**

**`Seek_Table_Entries`（每条 8 或 12 字节）**：

| `Compressed_Size` | `Decompressed_Size` | `[Checksum]` |
|---|---|---|
| 4 bytes | 4 bytes | 4 bytes（仅当 `Checksum_Flag` 置位） |

- `Checksum` 的定义：*"the least significant 32 bits of the **XXH64** digest of the uncompressed data"* —— **仍然是 XXH64，仍然不是 CRC-32。**

#### 怎么判断一个 `.zst` 是不是 seekable？

**【事实】O(1) 判定法**：读文件**最后 4 字节**，若等于小端 `0x8F92EAB1`（磁盘字节序 `B1 EA 92 8F`）则**可能**是 seekable，再往前读 9 字节的 footer 解析 `Number_Of_Frames` 与 `Seek_Table_Descriptor` 做二次确认。

**【实测】**普通 `.zst` 的末 4 字节是压缩数据/校验和的尾巴，不会命中：
```
exp/a.tar.zst 末4字节: ef99913a
exp/b.tar.zst 末4字节: d5caf401
seekable 应为:        b1ea928f
```

注意规范自己的警告（针对 `Skippable_Magic_Number` `0x184D2A5E`）：*"Since it is legal for other Zstandard skippable frames to use the same magic number, it is not recommended for a decoder to recognize frames solely on this."* —— 所以要用**文件末尾的 `Seekable_Magic_Number`**（唯一值 `0x8F92EAB1`）判定，而不是 skippable magic。

#### 如果是 seekable，能只解压前几个块吗？

**能。** 有 seek table 就能算出任意帧在压缩文件中的偏移（原文：*"The cumulative sum of the `Compressed_Size` fields of frames `0` to `i` gives the offset in the compressed file of frame `i+1`"*），直接 `seek` 过去解一帧。

**但本库里几乎肯定没有 seekable 文件。** ratarmount 官方 README 说得非常明确：

> **"In contrast to bzip2 and gzip compressed files, true seeking on XZ and ZStandard files is only possible at block or frame boundaries. Even though both file formats do support multiple frames and XZ even contains a frame table at the end for easy seeking, both compressors write only a single frame and/or block out, making this feature unusable."**

要产生多帧可寻址的 zstd，必须用 **`pzstd` / `t2sz` / `zeekstd`** 之类的工具。**【实测】**本机 `zstd -l` 对三个测试文件均报告 `Frames 1, Skips 0` —— 单帧、无可跳过帧、无 seek table，与上述说法一致。

> **【推断】** 库里 2,685 个 `.zst` 中 seekable 的比例接近 0。**但这是 O(1) 可验证的**（每文件读末 4 字节），成本可忽略，**建议实际扫一遍再下定论**，见第 6 部分 Step 0。

### 1.7 字典压缩：检测方法与后果

**格式层面**：`Dictionary_ID` 是 `Frame_Header` 里的可选字段（`DID_Field_Size` ∈ {0,1,2,4}），规范原文：

> "This is a variable size field, which contains the ID of the dictionary required to properly decode the frame."
>
> "A value of `0` has same meaning as no `Dictionary_ID`, in which case **the frame may or may not need a dictionary to be decoded**, and the ID of such a dictionary is not specified. **The decoder must know this information by other means.**"

**字典文件本身的格式**（`zstd --train` 产物）：

| `Magic_Number` | `Dictionary_ID` | `Entropy_Tables` | `Content` |
|---|---|---|---|
| 4 字节，值 **`0xEC30A437`** 小端 | 4 字节小端，**不能为 0** | — | — |

**没有字典能否解压？**

**不能。** 字典内容参与 LZ 匹配的「历史」（规范：*"The content act as a 'past' in front of data to compress or decompress, so it can be referenced in sequence commands"*），缺失字典时 sequence 指令引用的偏移无从解析 —— 解码器会报错。

**怎么检测？**

- **有 `Dictionary_ID` 且非 0** → 明确需要字典，且知道要哪个。**零解压可检测**：
  ```rust
  // zstd-safe，只读前 ≤14 字节即可
  zstd_safe::get_dict_id_from_frame(&header_bytes) -> Option<NonZeroU32>
  ```
  文档原文：*"Returns `None` if the dictionary ID could not be decoded. This may happen if: The frame was not encoded with a dictionary. The frame intentionally did not include dictionary ID. The dictionary was non-conformant. `src` is too small..."*
- **`DID_Field_Size = 0`（或 ID 为 0）** → **无法零解压区分「不需要字典」与「需要字典但没记 ID」**。规范明说「解码器必须通过其它途径知道」。此时只能**试着解压**：若报 `dictionary mismatch` 类错误则说明缺字典。
- 也可用 `--no-dictID` 主动去掉 ID（CLI 手册原文：*"do not store dictionary ID within frame header... The decoder will have to rely on implicit knowledge about which dictionary to use"*）。

> **【推断】** ROM 归档用外部字典的可能性**极低** —— 字典的价值在于压缩大量**小而同质**的文件（如日志、JSON），对单个几百 MB 的游戏镜像几乎无收益，且会让归档失去自包含性（丢了字典 = 数据永久丢失），任何以分发为目的的打包者都不会这么干。**未找到任何 ROM 分发使用 zstd 字典的一手证据。**
> **工程建议**：仍然写上 `get_dict_id_from_frame()` 检查（一行代码，零成本），命中就报错跳过并记录，而不是让解压器抛出难懂的错误。

---

## 第 2 部分 · `.tar.zst` 双层结构（重点）

### 2.1 tar 头部结构与精确偏移

来源：GNU tar 手册 *Basic Tar Format* 节（<https://www.gnu.org/software/tar/manual/html_node/Standard.html>），其中直接引用了 GNU tar 源码 `src/tar.h`，注释标明来自 **POSIX 1003.1-2024**。

```c
/* tar Header Block, from POSIX 1003.1-2024 */
struct posix_header
{                              /* byte offset */
  char name[100];               /*   0 */
  char mode[8];                 /* 100 */
  char uid[8];                  /* 108 */
  char gid[8];                  /* 116 */
  char size[12];                /* 124 */
  char mtime[12];               /* 136 */
  char chksum[8];               /* 148 */
  char typeflag;                /* 156 */
  char linkname[100];           /* 157 */
  char magic[6];                /* 257 */
  char version[2];              /* 263 */
  char uname[32];               /* 265 */
  char gname[32];               /* 297 */
  char devmajor[8];             /* 329 */
  char devminor[8];             /* 337 */
  char prefix[155];             /* 345 */
                                /* 500 */
};
#define TMAGIC   "ustar"        /* ustar and a null */
#define TVERSION "00"           /* 00 and no null */
```

**归档整体结构**（GNU tar 手册原文）：

> "Physically, an archive consists of a series of file entries terminated by an end-of-archive entry, **which consists of two 512 blocks of zero bytes**."
>
> "Each file archived is represented by a header block which describes the file, followed by zero or more blocks which give the contents of the file."

**关键 `typeflag` 值**（同源）：`'0'`/`'\0'` 普通文件、`'1'` 硬链接、`'2'` 符号链接、`'5'` 目录、**`'x'` 扩展头（pax，作用于下一个条目）**、`'g'` 全局扩展头。

**数值字段编码**：POSIX pax 规范原文 —— *"All other fields are leading zero-filled **octal** numbers using digits from the ISO/IEC 646:1991 standard IRV."* 即 `size` 是 12 字节的前导零填充八进制 ASCII。

> **本项目关心的是**：`name`（偏移 0，100 字节）与 `size`（偏移 124，12 字节八进制）—— **这正是 DAT 匹配需要的「文件名 + 未压缩大小」**。tar 层能给出这两个，但给不出 CRC-32。

### 2.2 tar 的 `chksum` 是什么算法

POSIX.1-2024 pax 规范原文（<https://pubs.opengroup.org/onlinepubs/9799919799/utilities/pax.html>）：

> **"The _chksum_ field shall be the ISO/IEC 646:1991 standard IRV representation of the octal value of the simple sum of all octets in the header logical record. Each octet in the header shall be treated as an unsigned value. These values shall be added to an unsigned integer, initialized to zero, the precision of which is not less than 17 bits. When calculating the checksum, the _chksum_ field is treated as if it were all `<space>` characters."**

拆解：

| 属性 | 值 |
|---|---|
| 算法 | **512 字节头部的无符号字节简单求和**（不是 CRC，不是任何多项式） |
| 覆盖范围 | **仅 512 字节的 header block 本身**，**不含文件内容** |
| 计算时的特殊处理 | `chksum` 字段（偏移 148，8 字节）**当作 8 个空格** |
| 存储格式 | 八进制 ASCII |
| 累加器精度 | 「不少于 17 位」的无符号整数 |

**【实测】**按上述算法复算：

```
tar[0] name='PaxHeader/file1.bin' size=136  stored_chksum=15131(八进制) calc=15131(八进制) match=True
```

> **能不能用于 DAT 匹配？——绝对不能。** 它校验的是**头部元数据**（防止磁带介质损坏导致文件名/大小读错），与文件内容完全无关。两个内容完全不同但名字大小相同的文件，`chksum` 一模一样。

### 2.3 ⭐ 读出第一个条目的名字与大小需要解压多少字节

**理论上界（规范推导，【事实】）**：

规范定义 `Block_Maximum_Size`：

> "The size of `Block_Content` is limited by `Block_Maximum_Size`, which is determined once for a given frame and is the smallest of: `Window_Size`, 128 KiB (131.072 bytes). **Both the `Block_Content` and the decompressed size of any block in the frame must be no larger than `Block_Maximum_Size`.**"

注意规范强调**压缩后的 `Block_Content` 与解压后大小都**受此限制。因此：

```
读取上界 = Magic(4) + Frame_Header(≤10 额外字节, 共 ≤14) + Block_Header(3) + Block_Content(≤131072)
        ≈ 131,089 字节 ≈ 128 KiB
```

**一个 zstd block 解压出来至多 128 KiB 明文，而 tar 的第一个 header 只占 512 字节 —— 所以「第一个块」永远足够（且必然过量）。**

#### 【实测】精确边界验证

对 `b.tar.zst`（无 FCS，帧头 6 字节，第一个 block 压缩长 95,395 字节）：

```bash
$ for n in 95400 95403 95404 95410; do
    echo "读 $n B -> 明文 $(head -c $n b.tar.zst | zstd -dc 2>/dev/null | wc -c) B"
  done
  读 95400 B -> 明文 0 B
  读 95403 B -> 明文 0 B
  读 95404 B -> 明文 131072 B      <-- 6(帧头) + 3(块头) + 95395(块) = 95404
  读 95410 B -> 明文 131072 B
```

> **【实测结论】解压的最小粒度就是一个完整 block，不多不少。** 少 1 字节吐 0，够了一次吐满 128 KiB。这也解释了为什么「喂 64 KiB 什么都出不来」—— 第一个块压缩后有 95 KB，没读完就无法解码。
>
> **工程含义**：`.tar.zst` 的「读第一个条目」是 **`O(1)` 且有硬上界 ~131 KB** 的操作，和 ZIP 读中央目录属于同一量级的廉价操作。**这是 zst 唯一的好消息。**

#### zstd 流式解压能否读够 N 字节后提前中止？

**能。**【事实】

- **C 层**：`ZSTD_decompressStream()` 是显式的推-拉式流接口，调用方每次给一段输入、拿一段输出，随时可以停止调用并 `ZSTD_freeDCtx()`。
- **Rust 层**：`zstd::stream::read::Decoder`（即 `zstd::Decoder`）实现 `std::io::Read`。「提前中止」= **停止 `read()` 并 `drop` 掉 decoder**。底层 `Read` 源只会被读到 decoder 实际消费的位置。
- **【实测】**上面 `head -c 95404 | zstd -dc` 成功产出 131,072 字节明文然后正常结束（`head` 关闭管道），证明喂一个**截断的前缀**给流式解压器是完全可行的。

### 2.4 ⭐ 列出全部条目必须解压整个流

**这是 tar 格式的固有性质，不是实现缺陷。**

**为什么**：tar 是**顺序**格式，条目 = `[512B header][内容，512B 对齐填充][512B header][内容]...`。要找到第 N+1 个 header，必须先**跳过**第 N 个条目的全部内容。对一个可 `Seek` 的底层（裸 `.tar` 文件）这是 `lseek`，代价为零；**对 zstd 解压流，「跳过」只能靠真正解压并丢弃**。

**源码级确认**（`tar` crate，<https://github.com/composefs/tar-rs>）：

```rust
fn skip(&mut self, mut amt: u64) -> io::Result<()> {
    if let Some(seekable_archive) = self.seekable_archive {
        let pos = io::SeekFrom::Current(i64::try_from(amt)...);
        (&seekable_archive.inner).seek(pos)?;          // 可 Seek：真跳过
    } else {
        let mut buf = [0u8; 4096 * 8];
        while amt > 0 {                                 // 不可 Seek：读出来丢掉
            let n = cmp::min(amt, buf.len() as u64);
            let n = (&self.archive.inner).read(&mut buf[..n as usize])?;
            if n == 0 { return Err(other("unexpected EOF during skip")); }
            amt -= n as u64;
        }
    }
    Ok(())
}
```

`tar` crate 提供了 `Archive::entries_with_seek()`（文档：*"Seek will be used to efficiently skip over file contents"*），但它要求 `R: Read + Seek`。**`zstd::Decoder<R>` 不实现 `Seek`**，因此对 `.tar.zst` 只能走 `entries()` → 走上面的 `else` 分支 → **逐字节解压全流**。

> **明确回答问题 6 的第二半：是的，要列出 `.tar.zst` 的完整条目清单，必须解压整个流。**
>
> 唯一的例外是 seekable zstd（1.6 节），但那要求打包时就用 `pzstd`/`t2sz`/`zeekstd`，且**即便如此 tar 层依然是顺序的** —— seekable 只是让你能跳到压缩流的任意**字节偏移**，而你事先并不知道下一个 tar header 在哪个偏移，除非另有索引（见 2.7）。**seekable 单独并不能解决「列清单」问题。**

### 2.5 pax header 陷阱：第一个 512 字节块往往不是真条目

**【实测】**bsdtar 默认写 pax 格式：

```
$ tar -cf single.tar -C src file1.bin     # bsdtar 3.5.3，默认 pax
$ # 逐 512 字节块检视
  block0: name=b'PaxHeader/file1.bin'  typeflag='x'  size=b'00000000210 '
  block1: name=b'30 mtime=1788150628.836242432\n57 LIBARCHIVE.xattr.com.apple.provenance=...'  (扩展头的数据体)
  block2: name=b'file1.bin'  typeflag='0'  size=b'00026706600 '     <-- 真条目在偏移 1024
```

对比显式指定 ustar 格式：

```
$ tar --format=ustar -cf ustar.tar -C src file1.bin
  ustar block0: name=b'file1.bin' typeflag='0' size=b'00026706600 ' magic=b'ustar\x00'   <-- 偏移 0 就是真条目
```

> **【实测结论】「读前 512 字节 → 取 `name` 和 `size`」是错的实现。** 遇到 `typeflag == 'x'`（pax 扩展头）或 `'g'`（全局扩展头）必须跳过它自身的数据体再读下一个 header；GNU 的 `'L'`（longname）同理。
>
> 幸运的是这些前缀块都很小（几百字节到几 KB），**全部落在第一个 128 KiB block 里**，所以 2.3 节的 131 KB 上界依然成立。
>
> **工程建议**：不要手搓 tar 解析，直接用 `tar` crate 的 `entries()` 迭代器 —— 它已正确处理 pax / GNU longname / sparse，只取**第一个 `Entry`** 然后 `drop` 掉迭代器即可。

### 2.6 单成员归档的零成本判定技巧

**【推断，但为算术必然】**

若 `Frame_Content_Size`（= 整个 tar 流的字节数，记 `FCS`）存在，且已读出第一个真实条目的 `size = S`，则：

```
单成员归档的 tar 总长 = 512 (header)
                     + ceil(S / 512) * 512 (内容 + 对齐填充)
                     + 1024 (两个全零结束块)
```

**当且仅当该值 == `FCS` 时，这个 tar 只可能有一个成员**（多一个成员至少要多 512 字节 header）。

**代价：零。** `FCS` 来自帧头前 14 字节；`S` 来自已经解出的第一个 block。不需要任何额外 I/O 或解压。

**【实测】**验证逻辑本身（本例因 macOS 生成了 pax 头 + 真条目，故正确判定为「多成员」，符合预期）：

```
FCS(整个tar) = 6002688
tar[0] name='PaxHeader/file1.bin' size=136
512 + 512 + 1024 = 2048  vs FCS 6002688  -> 多成员
```

> **实用价值**：库里像 `TWEWY.vpk.tar.zst` 这种「一个 tar 只裹一个文件」的形态，**如果**帧头带 `FCS`，就能零成本确认「里面只有一个文件，名字是 X，大小是 Y」—— 这已经是 DAT 匹配需要的两项信息中的一项半了（缺 CRC-32）。
>
> **但注意**：1.3 节已证明 `tar --zstd` 产物**没有** `FCS`。这个技巧对那类文件失效。

### 2.7 有没有「不解压就列出 tar 内容」的办法

**【事实】有，但都需要「先付一次全量解压的代价来建索引」。**

**ratarmount**（MIT，<https://github.com/mxmlnkn/ratarmount>）是这条路上最成熟的一手实现：

- 支持 `.tar.zst`，zstd 侧由 `indexed_zstd` 提供。
- **建立 SQLite 索引文件**，默认路径：`<path to tar>.index.sqlite`，或 `~/.ratarmount/<path with / -> _>.index.sqlite`。
- 索引里存的就是每个 tar 条目的名字、大小、以及在**未压缩流**中的偏移。
- **但随机访问 zstd 仍受 1.6 节的限制**：README 原文 *"true seeking on XZ and ZStandard files is only possible at block or frame boundaries"*，且标准压缩器只写单帧 → 索引能省掉「重新扫描 tar 结构」的钱，**省不掉「解压到目标偏移」的钱**，除非文件本身是多帧的。

**其它形态**：
- **归档旁边存索引文件**：ratarmount 的 `.index.sqlite` 就是这个模式。**【未找到权威来源】** 任何 ROM 分发圈把这类索引随归档一起分发的证据 —— 实际上库里的 `.tar.zst` 旁边不会有索引。
- **zstd 官方没有任何「归档目录」机制** —— 它不是归档格式，规范里没有成员表、没有中央目录。这是设计取舍，不是遗漏。

> **对本项目的含义**：索引的价值在于**第二次及以后**的访问。本项目的识别管线本来就会把 `(路径, size, mtime) → 结果` 缓存进自己的数据库，**这就是等价的索引**，没必要引入 ratarmount 那套。真正的成本永远是「第一次必须全量解压」。

---

## 第 3 部分 · 性能估算

### 3.1 zstd 解压速度：官方数据

来源：facebook/zstd README *Benchmarks* 节（Core i7-9700K @ 4.9GHz，Ubuntu 24.04，lzbench，Silesia 语料）

| Compressor name | Ratio | Compression | **Decompress.** |
|---|---|---|---|
| **zstd 1.5.7 -1** | 2.896 | 510 MB/s | **1550 MB/s** |
| zlib 1.3.1 -1 | 2.743 | 105 MB/s | 390 MB/s |
| **zstd 1.5.7 --fast=1** | 2.439 | 545 MB/s | **1850 MB/s** |
| **zstd 1.5.7 --fast=4** | 2.146 | 665 MB/s | **2050 MB/s** |

**至关重要的一句**（README 原文）：

> **"Decompression speed is preserved and remains roughly the same at all settings, a property shared by most LZ compression algorithms."**

**这意味着：不管打包者用了 `-1` 还是 `-19`，解压速度都在同一量级（~1.5 GB/s 单线程）。** 无需担心「库里的文件是高压缩级别所以解压很慢」—— 那是 LZMA/7z 的问题，不是 zstd 的。

**【实测】**本机（Apple Silicon，单线程，数据在 page cache）：

```
$ zstd -b1 -e3 a.tar
 1#a.tar : 18011648 -> 13506129 (x1.334), 2165.9 MB/s, 1073.8 MB/s
 2#a.tar : 18011648 -> 13506046 (x1.334), 2179.8 MB/s, 1081.0 MB/s
 3#a.tar : 18011648 -> 13507098 (x1.333), 1966.0 MB/s, 1012.7 MB/s
```

本机解压 **~1073 MB/s**（测试数据是 base64 随机串，可压缩性低，属于保守场景）。

### 3.2 瓶颈在哪一侧

| 环节 | 吞吐 | 处理 2.50 TiB 压缩数据的耗时 |
|---|---|---|
| **外置机械盘顺序读** | 100 MB/s | **7.6 小时** |
| | 150 MB/s | **5.1 小时** |
| | 200 MB/s | **3.8 小时** |
| **zstd 解压**（官方 1550 MB/s） | 1550 MB/s | 0.57 小时（按 2.88 TiB 明文计） |
| **zstd 解压**（本机实测 1073 MB/s） | 1073 MB/s | 0.82 小时 |

> **【结论】瓶颈毫无疑问在磁盘侧，差距约 7–10 倍。**
>
> **只要用一个后台线程做 I/O、一个线程做解压（流水线重叠），全量解压 2.50 TiB 的墙钟时间 ≈ 纯读盘时间 ≈ 3.8–7.6 小时。** zstd 解压本身在这个流水线里是「免费的」。
>
> 未压缩总量的估算：库里 `.zst` 主要裹的是 PSV 内容（`.vpk` 本身就是 ZIP、PSARC/加密数据、媒体资源），**这些都是已压缩数据，zstd 二次压缩收益很小**。【推断】整体压缩比约 **1.05–1.3**，故 2.50 TiB 压缩 → **2.62–3.25 TiB** 明文。上表按 2.88 TiB 中值计算。**【未找到权威来源】** 该库的实际压缩比 —— 但只要 `Frame_Content_Size` 存在就能零成本精确求和（见 Step 0）。

### 3.3 「只读头部」路径的成本

若只读每个文件的前 128 KiB（拿 tar 第一个条目的名字 + 大小）：

```
实际读取 = 2685 × 131072 B = 336 MiB
顺序传输 @ 150 MB/s        =  2.3 秒
机械盘寻道 2685 次 @ 10 ms =   27 秒
------------------------------------
合计                        ≈ 30 秒
```

> **【结论】「读头部」路径 ≈ 30 秒，「全量解压」路径 ≈ 4–8 小时。相差约 500–900 倍。**
> 这个巨大的落差正是第 6 部分分层策略的依据。

### 3.4 转换成 7z 的成本（用于第 6 部分的取舍）

转换 = 读 2.50 TiB + zstd 解压 + **LZMA2 压缩** + 写回 ~2.4 TiB。

| 受限环节 | 假设 | 耗时 |
|---|---|---|
| **I/O**（读 2.50 + 写 2.4 = ~4.9 TiB 混合读写） | 有效 60 MB/s（机械盘混合读写惩罚） | **24.9 小时** |
| | 有效 100 MB/s | **15.0 小时** |
| **CPU**（LZMA2 压缩 2.88 TiB 明文） | 20 MB/s（`-mx9` 单/少线程） | **44.0 小时** |
| | 50 MB/s（`-mx5` 多线程） | **17.6 小时** |
| | 100 MB/s（`-mx1` 多线程） | **8.8 小时** |

> **【推断】转换 2.50 TiB 到 7z 的墙钟成本区间：15–44 小时**，取决于压缩级别与盘速，实际很可能在 **一整天到两天**。
>
> **而且这些内容大多是已压缩数据，LZMA2 相对 zstd 的额外压缩收益会很小**（【推断】几个百分点），付出的时间几乎买不到空间。
>
> **另外还有硬性前提**：转换需要足够的临时空间。本次在本机看到的 16 TiB 盘只剩 439 GiB 可用，**必须逐个文件原地替换**（转一个删一个），这在中途失败时有数据损失风险。

---

## 第 4 部分 · Rust 生态

### 4.1 zstd 相关 crate

数据来自 crates.io API（2026-08-31 抓取）。

| Crate | 版本 / 累计下载 / 最近更新 | 许可 | 纯 Rust | 流式解压 + 提前中止 | 零解压读帧头 | seekable |
|---|---|---|:---:|:---:|:---:|:---:|
| **`zstd`** <https://github.com/gyscos/zstd-rs> | 0.13.3 / 3.70 亿 / 2025-02 | **MIT** | ❌ FFI（绑定 libzstd） | ✅ `zstd::Decoder` 实现 `io::Read` | ⚠️ 需下沉到 `zstd-safe` | ❌ |
| **`zstd-safe`** 同仓 | 7.2.4 / 3.77 亿 / 2025-03 | **MIT OR Apache-2.0** | ❌ 安全封装层 | ✅ | ✅ **见下** | ❌ |
| **`ruzstd`** <https://github.com/KillingSpark/zstd-rs> | 0.9.0 / 6,148 万 / **2026-07** | **MIT** | ✅ **纯 Rust** | ✅ `decoding::StreamingDecoder` 实现 `io::Read` | ⚠️ 无公开的单独帧头 API | ❌ |
| **`async-compression`** <https://github.com/Nullus157/async-compression> | 0.4.43 / 2.06 亿 / **2026-07** | **MIT OR Apache-2.0** | ❌（`zstd` / `zstdmt` feature，底层仍是 `zstd` crate） | ✅ AsyncRead 适配器 | ❌ | ❌ |
| **`zstd-seekable`** <https://nest.pijul.com/pmeunier/zstd-seekable> | 0.1.23 / 22.9 万（最近仅 8.5k）/ **2023-06** | **BSD-3-Clause** | ❌ FFI | — | — | ✅ 唯一 |

**⭐ `zstd-safe` 提供的零解压帧头 API**（源码 `zstd-safe/src/lib.rs`）：

```rust
/// Args:
/// * `src`: A prefix of the compressed frame. It should at least include the frame header.
/// Returns:
/// * `Err(ContentSizeError)` if `src` is too small of a prefix, or if it appears corrupted.
/// * `Ok(None)` if the frame does not include a content size.
/// * `Ok(Some(content_size_in_bytes))` otherwise.
pub fn get_frame_content_size(src: &[u8]) -> Result<Option<u64>, ContentSizeError>

/// Wraps the `ZSTD_getDictID_fromFrame()` function.
pub fn get_dict_id_from_frame(src: &[u8]) -> Option<NonZeroU32>

pub fn is_frame(buffer: &[u8]) -> bool
pub fn find_frame_compressed_size(src: &[u8]) -> SafeResult
```

> **选型建议**：`zstd` (0.13, MIT) 做主力解压器 + `zstd-safe` 的 `get_frame_content_size` / `get_dict_id_from_frame` 做零解压探测。二者同仓同版本、`zstd` 本来就依赖 `zstd-safe`，**不引入新依赖**。
>
> `ruzstd` 作为纯 Rust 备选（避免 C 编译工具链、便于交叉编译），代价是速度 —— README 原文：*"my decoder is about 3.5 times slower [on enwik9]... for less compressible data (like a ubuntu installation .iso) my decoder comes close to only being 1.4 times slower"*。**本项目正好属于「低可压缩性」场景（已压缩的 ROM 数据），所以实际差距接近 1.4×，而瓶颈本来就在磁盘 —— `ruzstd` 完全可用。**
>
> **`zstd-seekable` 不建议引入**：BSD-3-Clause 许可没问题，但最近更新在 2023-06，最近下载量仅 8.5k，且 1.6 节已论证库里几乎不可能有 seekable 文件。

### 4.2 tar 解析：`tar` crate

<https://github.com/composefs/tar-rs>，**0.4.46 / 2.19 亿下载 / 2026-05 / MIT OR Apache-2.0**。

crate 描述原文里有一句正对本项目胃口：

> "This library does not currently handle compression, but it is **abstract over all I/O readers and writers**. Additionally, great lengths are taken to ensure that the **entire contents are never required to be entirely resident in memory** all at once."

**能不能与流式 zstd 组合做到「解压到够读 header 就停」？——能，而且是自然组合。**

```rust
use std::fs::File;
use std::io::BufReader;

// 只读第一个条目的名字与大小，读完即停
fn first_entry(path: &std::path::Path) -> std::io::Result<(String, u64)> {
    let f = BufReader::new(File::open(path)?);
    let dec = zstd::Decoder::new(f)?;          // impl Read
    let mut ar = tar::Archive::new(dec);
    let mut it = ar.entries()?;                 // 顺序迭代器
    let e = it.next().unwrap()?;                // <-- 只拉第一个；tar crate 已处理 pax/longname
    let name = e.path()?.display().to_string();
    let size = e.header().size()?;
    Ok((name, size))
    // it / ar / dec 在此 drop —— 底层 File 只被读到 ~131 KB 处
}
```

**为什么这真的只读 131 KB**：`Archive::entries()` 返回惰性迭代器；取第一个 `Entry` 只需要 tar 的前 512（或 pax 情况下前 1024–2048）字节；`zstd::Decoder` 为满足这次 `read()` 只会向底层 `File` 拉取「一个完整 block」所需的压缩字节；`drop` 之后不再有任何 I/O。**2.3 节的实测已从 CLI 层面验证了同一行为。**

**⚠️ 但要避免的反模式**：

```rust
for entry in ar.entries()? { ... }   // <-- 这会解压整个流！见 2.4 节的 skip() 源码
```

**⚠️ `entries_with_seek()` 用不了**：它要求 `R: Read + Seek`，而 `zstd::Decoder` 只实现 `Read`。

---

## 第 5 部分 · 生态支持

### 5.1 ⭐ RetroArch：支持 `.zst`，但期望的是**裸 zst 单文件**，且对 `.tar.zst` 无能为力

**源码：`libretro-common/file/archive_file.c`**（<https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file.c>）

```c
const struct file_archive_file_backend* file_archive_get_file_backend(const char *path)
{
#ifdef HAVE_7ZIP
   if (string_is_equal_noncase(file_ext, "7z"))     return &sevenzip_backend;
#endif
   ... zip / apk -> &zlib_backend ...
#if defined(HAVE_ZSTD) || defined(HAVE_RZSTD)
   if (string_is_equal_noncase(file_ext, "zst"))    return &zstd_backend;
#endif
   return NULL;
}
```

> **✅ 核实通过：此前该项目「RetroArch 归档后端支持 zip / 7z / apk / zst 四种」的结论正确。**
> **补充核实**：`libretro-common/file/` 目录下**只有** `archive_file.c` / `archive_file_7z.c` / `archive_file_zlib.c` / `archive_file_zstd.c` —— **没有 rar 后端，也没有 tar 后端**。

**源码：`libretro-common/file/archive_file_zstd.c`** —— 三条决定性发现：

**(1) `.zst` 被当作「单文件容器」，内部文件名靠外层文件名推导**

```c
/* Derive the inner filename from the .zst path by stripping the .zst extension */
static void zstd_derive_inner_filename(const char *path, char *s, size_t len)
{
   const char *base = path_basename(path);
   strlcpy(s, base, len);
   ext = strrchr(s, '.');
   if (ext && string_is_equal_noncase(ext, ".zst"))
      ((char*)ext)[0] = '\0';
}
```

以及函数注释：

```c
/* Extract the file from a .zst archive (single-file container).
 * If needle doesn't match the derived inner filename, returns -1. */
```

> **明确回答问题 11 的后半：RetroArch 期望的是「裸 zst 单文件」，即 `SuperGame.sfc.zst`。**
> 对 `TWEWY.vpk.tar.zst`，RetroArch 会推导出内部名 `TWEWY.vpk.tar`，解出一个**裸 tar 字节流**丢给核心 —— 而 RetroArch 没有 tar 后端，核心也不认识 tar。**等于没解。**

**(2) 硬性要求 `Frame_Content_Size` 存在，否则直接失败**

```c
   /* A frame that does not state its size is no use here: the whole
    * member has to be allocated before it is decoded. */
   if (  content_size == ZSTD_SIZE_UNKNOWN
      || content_size == ZSTD_SIZE_ERROR)
      return -1;

   /* decompressed_size is a uint32_t and feeds a later malloc; reject
    * values that would truncate to a smaller size... */
   if (content_size > UINT32_MAX)
      return -1;
```

> **两条硬限制**：
> - **没有 `Frame_Content_Size` → 直接拒绝。** 结合 1.3 节的实测，`tar --zstd` 产生的文件 **RetroArch 打不开**。
> - **未压缩 > 4 GiB（`UINT32_MAX`）→ 直接拒绝。** PSV 大作解包后超 4 GiB 并非罕见。
> - 另外它把整个成员 `malloc` 到内存里解 —— 一个 3 GiB 的 tar 需要 3 GiB 常驻内存。

**(3) RetroArch 自己也拿不到 zst 的 CRC —— 写死为 0**

```c
   userdata->crc  = 0;                             // archive_file_zstd.c
   userdata->size = (uint64_t)content_size;
```

而 `archive_file.c` 的 CRC 查询函数最终就是把它原样返回：

```c
uint32_t file_archive_get_file_crc32_and_size(const char *path, uint64_t *size)
{
   ...
   *size = userdata.size;
   return userdata.crc;        // <-- 对 .zst 恒为 0
}
```

> **⭐ 这是全文最有说服力的一条外部佐证：RetroArch 是目前唯一在生产环境实现了 zst 归档后端的主流前端，它自己就承认「zst 拿不到 CRC」，把字段写死成 0。** 对比 zip/7z 后端，那两个都是从容器元数据里读出真 CRC 填进同一个字段。
>
> 【推断】其后果是：RetroArch 的数据库扫描器对 `.zst` 内容无法按 CRC 匹配 —— 与本项目面临的是同一个问题。
>
> **【未找到权威来源】** `HAVE_RZSTD` 这个编译开关的确切含义（RetroArch 仓库 `deps/` 下确实 vendor 了完整的 `zstd`，代码搜索因 API 限流未能完成）。不影响上述结论 —— 两个分支走的都是同一个 `zstd_backend`。

### 5.2 独立模拟器

| 模拟器 | `.zst` / `.tar.zst` 支持 | 源码级证据 |
|---|---|---|
| **Vita3K**（本库最相关） | ❌ **完全不支持**。只认 `.zip` / `.vpk` / `.vci` | `vita3k/gui-qt/src/archive_install_dialog.cpp`:50 `if (ext == ".zip" \|\| ext == ".vpk" \|\| ext == ".vci")`；Qt 过滤器 `"...(*.zip *.vpk *.vci);;..."`；按钮文案 `"Select a file (.zip / .vpk / .vci)..."`；无归档文件时提示 `"The selected directory contains no .zip, .vpk, or .vci files."` |
| **PCSX2** | ❌ 光盘镜像**不支持** zst。`.zst` 仅用于 **GS dump**（`.gs.zst`）与存档压缩 | `pcsx2-qt/MainWindow.cpp` `OPEN_FILE_FILTER = "All File Types (*.bin *.iso *.cue *.mdf *.chd *.cso *.zso *.gz *.elf *.irx *.gs *.gs.xz *.gs.zst *.dump)"`；`VMManager.cpp` `IsGSDumpFileName()` 才检查 `.gs.zst` |
| **DuckStation** | ❌ 光盘镜像**不支持** zst。`.zst` 仅用于 `.psxgpu.zst` GPU dump 与存档压缩模式 | `src/core/system.cpp` `EndsWithNoCase(path, ".psxgpu.zst")`；`src/core/settings.cpp` 里 `"Zstandard (Default)"` / `"Zstandard (High)"` 是 `SaveStateCompressionMode` 的枚举名 |
| **PPSSPP** | ❌ 仓库内搜 `.zst` **零命中** | GitHub 代码搜索 `repo:hrydgard/ppsspp .zst` → `total_count = 0` |
| **melonDS** | ✅ **唯一的正面例外**：走 libarchive，扩展名白名单里明确含 `".tar.zst"` | 见 [`containers-and-compressed-images.md`](./containers-and-compressed-images.md) 3.2 节（`Window.cpp` 白名单：`".zip", ".7z", ".rar", ".tar", ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.lz4", ".tar.zst", …`） |

> **⭐ 对本库最要命的一条：库里的 `.tar.zst` 集中在 PSV 目录下，而 PSV 的唯一模拟器 Vita3K 连 `.zst` 都不认识，更不用说 `.tar.zst`。**
> 这批文件**以当前形态既无法被识别管线快速识别，也无法被目标模拟器直接使用**。

### 5.3 `.tar.zst` 在 ROM 分发圈的使用惯例

**【实测】** archive.org 上两个大型 PSV 合集的扩展名分布（通过 `https://archive.org/metadata/<item>` API 统计）：

| Item | 文件数 | 总大小 | 扩展名分布 |
|---|---|---|---|
| `ps-vita-game-dumps` | 74 | 169.2 GiB | **`.zip` 100%** |
| `sony-playstation-vita-usa-full-set-nonpdrm-format` | 1022 | 786.8 GiB | **`.zip` 100%** |
| | | | **`.tar.zst` 出现次数：0** |

> **【结论】`.tar.zst` **不是** PSV / ROM 分发圈的通行格式。** 上游 NoNpDrm 全集用的是 `.zip`。
>
> **【推断】** 库里这 2,685 个 `.tar.zst` 是**本地重打包产物**。旁证：
> - 路径带中文重制/汉化标记（`《月姬plus+disc》-PSV移植（unity重置版）`），指向中文重打包渠道；
> - 目录名 `VPK zst` 本身就像是「我把 VPK 转成 zst 之后放这里」的人工命名；
> - 该库所在盘根目录存在 `peazip-9.8.0.WIN64.exe`（PeaZip 原生支持 `.tar.zst` 打包）。
>
> **【未找到权威来源】** 「`.tar.zst` 通常裹的是什么」没有可引用的规范或社区约定文档。**但从库里的实际样例可以直接读出两种形态**（这是最可靠的一手证据）：
> - `PSV/VPK zst/TWEWY.vpk.tar.zst` → **裹单个 `.vpk` 安装包**（而 `.vpk` 本身是 ZIP，所以这是 **zip → tar → zst 三层**）
> - `PSV/《月姬plus+disc》-PSV移植（unity重置版）/ATSP11823.tar.zst` → 文件名是 PSV 的 **Title ID** 格式，【推断】裹的是**整个 NoNpDrm 游戏目录**（`ATSP11823/` 下的 `sce_sys/`、`eboot.bin` 等），多成员。
>
> **⭐ 一个免费的识别捷径**：PSV 内容的 Title ID **已经写在外层文件名里了**（`ATSP11823`）。对 PSV 这一类，**根本不需要打开归档**就能拿到最关键的标识符。见第 6 部分 Step 2。

---

## 第 6 部分 · 对识别管线的可执行建议

### 6.1 直接回答核心问题

> **`.zst` / `.tar.zst` 能不能进「零解压读 CRC-32」那条快路？**

**不能。三层原因，每层都是硬性的：**

1. **zstd 帧格式里没有 CRC-32 字段**（规范全文 0 次命中）。唯一的校验和是 XXH64 低 32 位，且可选、且覆盖整个流而非单个成员。
2. **tar 头部里也没有 CRC-32**。`chksum` 是 512 字节头部的无符号字节简单求和，只保护元数据，与文件内容无关。
3. **两层都没有「成员 → 内容哈希」的映射表**。ZIP 的中央目录 / 7z 的头部是把这个映射**显式存下来**了；zstd 是流压缩器，tar 是顺序磁带格式，二者的设计里都没有这个概念。

> **退而求其次的最便宜路径是什么？代价多大？**

**分层，让绝大多数文件停在便宜的那一层。**

| 层 | 能拿到什么 | 单文件 I/O | 2,685 文件总代价 |
|---|---|---|---|
| **L0 · 读帧头（前 14 字节）** | 是否 zstd 帧、是否需字典、`Window_Size`、**（若存在）未压缩总大小** | 14 B + 1 次寻道 | **~30 秒** |
| **L0b · 读末 4 字节** | 是否 seekable | 4 B | 与 L0 合并，~0 |
| **L1 · 解第一个 block（≤131 KB）** | tar **第一个条目**的**文件名 + 未压缩大小**；配合 L0 的 FCS 可**零成本判定是否单成员** | ≤131 KB + 1 次寻道 | **~30 秒** |
| **L2 · 全量流式解压** | **全部条目的名字 + 大小 + 真 CRC-32**（自己边解边算） | 整个文件 | **3.8–7.6 小时**（磁盘受限，**只做一次**） |

**关键洞察：L2 的代价 ≈ 纯读盘代价。** 第 3 部分已证明 zstd 解压（~1.5 GB/s）比机械盘（100–200 MB/s）快 7–10 倍，**解压这件事在流水线里是免费的**。所以「全量解压 2.50 TiB」实际上就是「把这 2.50 TiB 读一遍」——而任何**认真的**识别方案迟早都得把数据读一遍（哪怕只为算 SHA-1）。

**并且 L2 顺手就把 CRC-32 算出来了**：流式解压时对每个 tar 成员的字节流同时喂给 `crc32` / `sha1` / `md5`，**不需要落盘、不需要额外内存**（`tar` crate 的 `Entry` 本身就是 `Read`）。**所以 `.zst` 拿到的 CRC-32 质量与 zip/7z 完全一致，只是拿的过程贵了 500–900 倍。**

### 6.2 推荐的实施步骤

**Step 0 · 先花 1 分钟做一次帧头普查（在写任何解压代码之前）**

对全部 2,685 个 `.zst` 各读**前 14 字节 + 末 4 字节**，统计：

- `Frame_Content_Size` 存在的比例 → **决定 L1 的「单成员零成本判定」技巧适用于多少文件**，也直接给出全库未压缩总量的精确值（用于修正第 3 部分的估算）
- `Dictionary_ID` 非 0 的比例 → 预期为 0，若非 0 立刻标记为「需要字典，无法处理」
- 末 4 字节 == `B1 EA 92 8F` 的比例 → 预期为 0
- `Content_Checksum_flag` 的比例 → 侧面反映打包工具（无 FCS + 无 check ≈ libarchive 的 `tar --zstd`）

**这一步是整个决策的前提，成本约 30 秒，回报是把第 3 部分所有 `【推断】` 变成 `【事实】`。**

**Step 1 · 按扩展名分流**

`*.tar.zst` 走 tar 路径；裸 `*.zst`（非 `.tar.zst`）按 RetroArch 的语义处理 —— 内部名 = 外层名去 `.zst`，直接就是单个文件，L0 的 FCS 即其大小。

**Step 2 · ⭐ 先用文件名做一轮免费匹配（PSV 专属捷径）**

`ATSP11823.tar.zst` 里的 `ATSP11823` 就是 PSV Title ID。**PSV 的 DAT / 元数据源本来就以 Title ID 为主键**（见 [`metadata-formats.md`](./metadata-formats.md)）。对这一类文件，**零 I/O 即可高置信度识别**。

`TWEWY.vpk.tar.zst` 这种带游戏名的，也能进模糊匹配的候选池。

> **这一步很可能覆盖库里 `.tar.zst` 的绝大部分** —— 因为它们集中在 PSV 目录下。**【推断】，需 Step 0/1 的实际文件名分布验证。**

**Step 3 · L1 补充证据**

对 Step 2 未定案的，解第一个 block 拿内部文件名 + 大小。用 `tar` crate 的 `entries().next()`，**不要 for 循环**（2.4 节）。

**Step 4 · L2 只对仍然存疑的文件做**

在数据库里按 `(路径, size, mtime)` 缓存结果，**保证全量解压永远只发生一次**。

即使最坏情况（Step 2/3 全部失效，2,685 个文件都要走 L2），代价也就是 **一个通宵（4–8 小时）**，且之后永久免费。

**Step 5 · 在 UI 里明确标注 zst 的代价**

因为 `.zst` 的识别代价与 `.zip`/`.7z` 差 2–3 个数量级，**必须让用户知道「扫描 PSV 目录会慢很多」**，并提供「跳过 zst / 仅按文件名识别 zst」的开关。

### 6.3 ⭐ 取舍建议：(a) 在工具里支持 zst，还是 (b) 让用户转换 2.50 TiB？

**必须把这个问题拆成两个 —— 它们的答案相反。**

#### 问题一：为了**识别**，该选哪个？ → **明确选 (a)**

| | (a) 工具支持 zst | (b) 用户转成 7z/rar |
|---|---|---|
| **一次性时间成本** | **3.8–7.6 小时**（纯读盘受限，可后台跑，可中断续跑） | **15–44 小时**（3.4 节：I/O 与 LZMA2 CPU 双向受限） |
| **是否也要把 2.50 TiB 读一遍** | 要 | **同样要** —— 转换无法绕开这一步 |
| **额外付出** | 无 | 额外的 LZMA2 压缩 CPU + 等量 2.4 TiB 写回 |
| **空间要求** | 无 | 需要临时空间；本机所见盘仅剩 439 GiB → 必须「转一个删一个」，**中途失败有数据损失风险** |
| **压缩率收益** | — | **【推断】接近 0**：内容多为已压缩数据（VPK=ZIP、PSARC、加密数据、媒体），LZMA2 相对 zstd 提升有限 |
| **可逆性** | 完全可逆（不改动用户数据） | **不可逆**，原文件被销毁 |
| **开发成本** | 低：`zstd` + `tar` 两个成熟 crate（均 MIT/Apache-2.0，无 RAR 那种许可雷区），核心逻辑就是 6.2 节那个 15 行的函数 | 零（但把成本转嫁给用户） |
| **7z 的新风险** | — | **引入 solid block 问题**（见 `containers-and-compressed-images.md` 1.6：7z solid 是本项目已确认的最大性能陷阱）—— **等于用一个已知陷阱换掉另一个** |
| **后续重扫成本** | 缓存后为零 | 缓存后为零（**打平**） |

> **结论：(b) 在识别维度上被 (a) 严格支配。** 转换同样要把 2.50 TiB 读一遍（这本来就是 (a) 的全部成本），却额外要付出压缩 CPU、等量写回、空间风险与不可逆性，换来的压缩率收益接近于零，还可能踩进 7z solid block 的坑。
>
> **而且许可面也支持 (a)**：`zstd`(MIT) / `zstd-safe`(MIT OR Apache-2.0) / `ruzstd`(MIT) / `tar`(MIT OR Apache-2.0) 全部干净，**不存在 `unrar` 那种「非自由软件 + 禁止复现算法」的硬约束**（见 `containers-and-compressed-images.md` 1.8）。

#### 问题二：为了**可玩性**，该建议用户做什么？ → **建议转换，但目标格式是 `.zip`，不是 7z/rar**

第 5 部分已确认：

- **Vita3K 只认 `.zip` / `.vpk` / `.vci`** —— 库里的 `.tar.zst` 在 Vita3K 里**一个都装不了**。
- **RetroArch 虽然认 `.zst`，但没有 tar 后端**，`.tar.zst` 解出来是个裸 tar 流，等于没解；而且它还**硬性要求 `Frame_Content_Size` 存在**（部分文件不满足）、**拒绝 >4 GiB**、**整个成员 `malloc` 进内存**。
- **archive.org 上的两个 PSV 大合集是 100% `.zip`** —— `.tar.zst` 不是上游格式，是本地重打包的产物。

> **所以给用户的建议应该是：**
>
> **「这 2.50 TiB 的 `.tar.zst` 在 Vita3K 里完全无法使用。如果你想真的玩这些游戏，需要转换 —— 但目标格式是 `.zip`（Vita3K 原生支持、也是 archive.org 上游 PSV 合集的标准格式），不是 7z 或 rar。而且这跟识别无关：本工具无论如何都能识别它们，只是慢一些。」**
>
> 转成 `.zip`（store 或 deflate）的成本比转 7z **低得多** —— 内容本已压缩，用 `-0`(store) 或 `-1` 即可，**CPU 不再是瓶颈**，回到纯 I/O 受限的 **15–25 小时**（3.4 节的 I/O 行）。**而且顺带把识别路径也升级成了「零解压读 CRC-32」的快路** —— 一石二鸟。

#### 一句话总结

> **工具端选 (a)：支持 zst，把它实现成「L0/L1 便宜探测 + L2 全量解压兜底 + 结果永久缓存」的分层路径，一次性代价 4–8 小时且受磁盘而非 CPU 限制。**
>
> **同时向用户提出一条独立的、以可玩性为由的建议：把 PSV 的 `.tar.zst` 转成 `.zip`（不是 7z/rar），因为 Vita3K 根本读不了 `.tar.zst`；这么做会顺带让它们进入零解压快路。**
>
> **绝不要把「转成 7z/rar」作为识别方案的前提条件** —— 那是把 15–44 小时的成本转嫁给用户，去换取一个工具自己花 4–8 小时就能拿到的结果，还要额外承担不可逆、空间不足与 7z solid block 的风险。

---

## 附录 · 一手来源清单

### zstd 格式与实现

| 来源 | URL | 用于 |
|---|---|---|
| Zstandard Compression Format **v0.4.5 (2026-05-14)** | <https://github.com/facebook/zstd/blob/dev/doc/zstd_compression_format.md> | 帧结构、`Frame_Header` 位定义、`Frame_Content_Size`、`Content_Checksum`、`Block_Maximum_Size`、Skippable frames、Dictionary Format；**「CRC」全文 0 次命中** |
| RFC 8878（Informational, 2021-02） | <https://www.rfc-editor.org/rfc/rfc8878.html> | 交叉印证：FCS 可选、Content_Checksum = XXH64 低 32 位、**不含 CRC-32** |
| Zstandard Seekable Format **v0.1.0 (2017-11-04)** | <https://github.com/facebook/zstd/blob/dev/contrib/seekable_format/zstd_seekable_compression_format.md> | `Seekable_Magic_Number 0x8F92EAB1`（须为文件最后 4 字节）、Seek_Table 结构、Checksum 亦为 XXH64 |
| `lib/zstd.h` | <https://github.com/facebook/zstd/blob/dev/lib/zstd.h> | `ZSTD_getFrameContentSize()` 与 `ZSTD_CONTENTSIZE_UNKNOWN`（*"typical for streaming"*）、`ZSTD_findFrameCompressedSize()` |
| `programs/zstd.1.md`（CLI 手册） | <https://github.com/facebook/zstd/blob/dev/programs/zstd.1.md> | `--[no-]content-size` 默认开启、`--stream-size=#`、`-C/--check`、`--no-dictID` |
| facebook/zstd README *Benchmarks* | <https://github.com/facebook/zstd/blob/dev/README.md> | zstd 1.5.7 `-1` 解压 1550 MB/s；**「解压速度在所有级别下基本不变」** |

### tar 格式

| 来源 | URL | 用于 |
|---|---|---|
| GNU tar 手册 *Basic Tar Format* | <https://www.gnu.org/software/tar/manual/html_node/Standard.html> | `struct posix_header` 全部字段与字节偏移（引自 `src/tar.h`，标注来源 POSIX 1003.1-2024）、`typeflag` 枚举、两个 512 字节全零结束块 |
| POSIX.1-2024 `pax` 规范 | <https://pubs.opengroup.org/onlinepubs/9799919799/utilities/pax.html> | `chksum` 算法定义（无符号字节简单求和、chksum 字段视作空格、八进制 ASCII、≥17 位累加器）、数值字段为八进制 |

### Rust crate

| Crate | URL | 用于 |
|---|---|---|
| `zstd` 0.13.3 (MIT) | <https://github.com/gyscos/zstd-rs> · <https://docs.rs/zstd/> | `zstd::Decoder` 实现 `io::Read`；`stream` / `bulk` / `dict` 模块 |
| `zstd-safe` 7.2.4 (MIT OR Apache-2.0) | <https://github.com/gyscos/zstd-rs>（`zstd-safe/src/lib.rs`） | `get_frame_content_size()` / `get_dict_id_from_frame()` / `is_frame()` / `find_frame_compressed_size()` 的签名与文档注释 |
| `ruzstd` 0.9.0 (MIT) | <https://github.com/KillingSpark/zstd-rs> · <https://docs.rs/ruzstd/> | 纯 Rust；`decoding::StreamingDecoder` 实现 `io::Read`；速度对比原话（enwik9 慢 3.5×，低可压缩数据慢 1.4×） |
| `async-compression` 0.4.43 (MIT OR Apache-2.0) | <https://github.com/Nullus157/async-compression> | `zstd` / `zstdmt` feature 存在（crates.io features 元数据） |
| `tar` 0.4.46 (MIT OR Apache-2.0) | <https://github.com/composefs/tar-rs>（`src/archive.rs`） | `entries()` / `entries_with_seek()` 文档；**`skip()` 在不可 Seek 时逐字节读取丢弃**的源码 |
| `zstd-seekable` 0.1.23 (BSD-3-Clause, 2023-06) | <https://nest.pijul.com/pmeunier/zstd-seekable> | 唯一支持 seekable 格式的 Rust crate；维护活跃度数据来自 crates.io API |
| ratarmount (MIT) | <https://github.com/mxmlnkn/ratarmount> | `.tar.zst` 索引（`.index.sqlite`）；**「两种压缩器都只写单帧，使该特性无法使用」**原话 |

### 模拟器与前端源码

| 项目 | URL | 用于 |
|---|---|---|
| RetroArch `archive_file.c` | <https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file.c> | `file_archive_get_file_backend()` 的 7z/zip/apk/zst 分派；`file_archive_get_file_crc32_and_size()` |
| RetroArch `archive_file_zstd.c` | <https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file_zstd.c> | 「单文件容器」定位、内部名推导注释、**要求 FCS 存在**、拒绝 >`UINT32_MAX`、**`userdata->crc = 0`** |
| RetroArch `libretro-common/file/` 目录清单 | GitHub Contents API | 只有 7z / zlib / zstd 三个归档后端，**无 tar、无 rar** |
| Vita3K `archive_install_dialog.cpp` | <https://github.com/Vita3K/Vita3K/blob/master/vita3k/gui-qt/src/archive_install_dialog.cpp> | `ext == ".zip" \|\| ext == ".vpk" \|\| ext == ".vci"`；Qt 文件过滤器 |
| PCSX2 `MainWindow.cpp` / `VMManager.cpp` | <https://github.com/PCSX2/pcsx2> | `OPEN_FILE_FILTER` 无 zst；`IsGSDumpFileName()` 才用 `.gs.zst` |
| DuckStation `system.cpp` / `settings.cpp` | <https://github.com/stenzek/duckstation> | `.psxgpu.zst`（GPU dump）；`"Zstandard (Default)"` 属 `SaveStateCompressionMode` |
| PPSSPP | <https://github.com/hrydgard/ppsspp> | 代码搜索 `.zst` → `total_count = 0` |

### 分发形态实证

| 来源 | URL | 用于 |
|---|---|---|
| archive.org `ps-vita-game-dumps` | <https://archive.org/metadata/ps-vita-game-dumps> | 74 文件 / 169.2 GiB，**100% `.zip`** |
| archive.org `sony-playstation-vita-usa-full-set-nonpdrm-format` | <https://archive.org/metadata/sony-playstation-vita-usa-full-set-nonpdrm-format> | 1022 文件 / 786.8 GiB，**100% `.zip`** |

### 本机实测环境

- macOS Darwin 24.6.0（Apple Silicon）
- `zstd` CLI **v1.5.7**（Homebrew）
- `tar` = **bsdtar 3.5.3 / libarchive 3.7.4**（macOS 系统自带）
- 测试语料：3 × 6,000,000 字节 base64 化随机数据（可压缩性低，接近 ROM 场景），tar 后 18,011,648 字节
- **注**：本次调研**未能**在真实库文件上验证 —— 当前挂载的 16 TiB 卷内不含 PSV 目录，`.tar.zst` 样本不可达。所有实测均基于本机合成样本；第 6 部分 Step 0 的普查正是为了用真实数据替换这些外推。
