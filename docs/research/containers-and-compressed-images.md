# 压缩容器与压缩光盘镜像在 ROM 识别管线中的处理

> **调研日期**：2026-08-31
> **来源约束**：仅采用一手来源 —— 格式规范原文、官方仓库源码、官方 wiki。**本文所有头部字段名与偏移量都来自实际抓取到的规范文本或源码，未做任何推测性补全。**
> **标注约定**：
> - **【事实】**（不加标记的正文默认为此类）= 有一手来源 URL 直接支撑，多数附源码/规范原文引用
> - **【推断】** = 本文基于事实的推理，**未经来源直接证实**
> - **【未找到权威来源】** = 明确标注，不猜
> **相关文档**：容器内部各平台 ROM 头部字段见 [`rom-identification.md`](./rom-identification.md) 第 A 章；CHD 与 Redump 哈希的关系见该文 B.2.4（本文不重复，只补充「零解压能拿到什么」的视角）。

---

## 目录

- [第 0 章 · 核心结论速览](#第-0-章--核心结论速览)
- [第 1 部分 · 通用归档容器](#第-1-部分--通用归档容器)
  - [1.1 ZIP](#11-zip)
  - [1.2 7z](#12-7z)
  - [1.3 RAR4 与 RAR5](#13-rar4-与-rar5)
  - [1.4 不解压能否拿到 SHA-1 / MD5](#14-不解压能否拿到-sha-1--md5)
  - [1.5 部分解压：读内部文件前 N 字节的代价](#15-部分解压读内部文件前-n-字节的代价)
  - [1.6 ⭐ solid block：本项目最大的性能陷阱](#16--solid-block本项目最大的性能陷阱)
  - [1.7 分卷压缩](#17-分卷压缩)
  - [1.8 ⭐ RAR 的许可问题](#18--rar-的许可问题)
  - [1.9 Rust 生态选型](#19-rust-生态选型)
- [第 2 部分 · 压缩光盘镜像](#第-2-部分--压缩光盘镜像)
  - [2.1 CHD](#21-chdmame-compressed-hunks-of-data)
  - [2.2 CSO / ZSO / DAX](#22-cso--zso--dax)
  - [2.3 PBP](#23-pbp)
  - [2.4 WIA / RVZ（含 GCZ）](#24-wia--rvz含-gcz)
  - [2.5 WBFS](#25-wbfs)
  - [2.6 NKit](#26-nkit)
  - [2.7 Rust 生态](#27-rust-生态压缩镜像)
- [第 3 部分 · 模拟器与前端的容器支持矩阵](#第-3-部分--模拟器与前端的容器支持矩阵)
  - [3.1 RetroArch](#31-retroarch含-libretro-核心)
  - [3.2 独立模拟器](#32-独立模拟器)
  - [3.3 前端](#33-前端)
- [第 4 部分 · 两张总表](#第-4-部分--两张总表)
- [第 5 部分 · 对识别管线的可执行建议](#第-5-部分--对识别管线的可执行建议)
- [附录 · 一手来源清单](#附录--一手来源清单)

---

## 第 0 章 · 核心结论速览

1. **ZIP / 7z / RAR 三者的容器元数据里都只有 CRC-32，没有 SHA-1、没有 MD5。** RAR5 可选地额外存 BLAKE2sp。因此「零解压拿到内部文件的 SHA-1/MD5」在三种格式里都**不可能**。
2. **但这不是坏消息**：No-Intro / Redump / TOSEC 的 DAT 每条 `<rom>` 都同时带 `size` 与 `crc`。**「CRC-32 + 未压缩大小」这一对，三种容器都能零解压读到**，足以作为高置信度的第一道命中层。这是整个项目性能可行性的关键。
3. **7z 的 solid block 是真实且严重的风险**，不是理论问题。官方 LZMA SDK 的 `SzArEx_Extract` 签名里直接有 `UInt32 *blockIndex /* index of solid block */` 和「把 `*outBuffer` 当成 solid block 的缓存」的注释 —— 读一个小文件确实要解压整块。**但 solid 与否可以在零解压的前提下从头部判定**（`SubStreamsInfo.NumUnPackStreamsInFolders[i] > 1`），且 7z 头部提供了 `文件 → folder` 的完整映射，因此可以「按 folder 分组、每个 folder 只解压一次」把代价从 O(文件数 × 块大小) 降到 O(总大小)。RAR solid 同理但更糟：RAR 头部没有等价的 folder 索引，只有逐文件的 solid 标志。
4. **RAR 的许可是工程上的硬约束**：`unrar` 官方源码不是自由软件（Debian 归类在 `non-free`），其许可禁止用它开发 RAR 兼容压缩器或复现 RAR 压缩算法，且要求转发时原样附带许可段落。Rust 生态里成熟的 `unrar` crate 正是这份 C++ 源码的 FFI 绑定。
5. **压缩光盘镜像里，只有 CHD 在头部存了原始数据的 SHA-1**（`rawsha1`），**其余 CSO / ZSO / DAX / PBP / WIA / RVZ / WBFS 的头部都没有任何「原始镜像内容哈希」字段** —— 这一条逐一核对了各自的 struct 定义，是「结构里就是没有这个字段」级别的确认。
6. **压缩光盘镜像反而比归档容器更容易做「零解压识别」**，因为它们几乎都把光盘头部或索引明文放在文件开头：
   - **PBP**：`PARAM.SFO` 在 `param_sfo_offset`（通常 `0x28`）处**未压缩明文**，DuckStation 就是 `fseek` + `fread` 直接读的 → `DISC_ID` / `TITLE` 零解压可得。
   - **WIA / RVZ**：头部里有 `u8 disc_header[0x80]` —— **光盘镜像的前 0x80 字节原样未压缩存放**（文件偏移 `0x58`）→ GameCube/Wii 的 Game ID 零解压可得。
   - **WBFS**：Wii 光盘头 256 字节在第二个 HD 扇区起始处（通常文件偏移 `0x200`）未压缩存放 → Game ID 零解压可得。
   - **CSO / ZSO / DAX**：分块压缩 + 完整块索引表，**读 ISO 前 64 KB 只需解压最前面的几十个块**。
   - **CHD**：hunk 级随机访问，代价是「先解压一次 hunk map，之后任意扇区 O(1) 定位 + 解压 1 个 hunk（≤512 KB）」。
7. **RetroArch 确实只支持 zip / 7z（外加 apk / zst），不支持 rar** —— 这是源码级确认：`file_archive_get_file_backend()` 的扩展名分派只有 `7z`、`zip`、`apk`、`zst`，且仓库里根本不存在 `archive_file_rar.c`。归档支持是**前端层面统一提供**的，但**具体行为按核心的 `need_fullpath` 而定**。
8. **rar 在模拟器生态里基本是死路，但不是完全没有**：melonDS（走 libarchive）与 DeSmuME 的 Windows 前端（内置 File_Extractor + unrar）是明确的例外。绝大多数（RetroArch、Mesen、Snes9x、ares、Dolphin、PCSX2、DuckStation、PPSSPP、Flycast、ES-DE）都不支持。**结论：库里的 .rar 必须转换成 .zip/.7z 才能可靠游玩。**
9. **PPSSPP 不支持 ZSO 与 CSO v2**（源码里 `hdr.ver > 1` 直接报错「CSO version too high!」，dispatch 只认 `CISO` / `\0PBP` / `MComprHD`），对应 issue 已被 **closed as not planned**。**PCSX2 反而支持 zso**。这意味着「PSP 用 ZSO」是个坑。
10. **NKit 是识别管线的头号误报源**：Dolphin 源码里的原话是「**这个文件的 CRC32 可能和好 dump 的 CRC32 相同，即使两个文件并不完全一样**」。检测方法明确：光盘偏移 `0x200` 处的 magic `"NKIT"`。

---

# 第 1 部分 · 通用归档容器

## 1.0 三种格式的结构范式差异（决定一切）

| | ZIP | 7z | RAR4 / RAR5 |
|---|---|---|---|
| 元数据位置 | **文件末尾**的 central directory（集中） | **文件末尾**的 Header（集中，且**可能被压缩**） | **散布全文**，每个文件头紧挨着自己的数据 |
| 元数据是否需要解压 | 否，明文 | **可能需要**（`kEncodedHeader`） | 否，明文（除非用了 `-hp` 加密头） |
| 读文件列表的 I/O 代价 | 2 次 seek（EOCD → central dir） | 2 次 seek + 一次小规模 LZMA 解压 | **顺序跳跃扫描全文**（每个 header 读完 seek 过它的数据） |
| 压缩单元 | **每个文件独立** | **folder（= solid block），可含多文件** | **可跨文件连续（solid）** |
| 随机访问单个文件 | 天然支持 | 非 solid 时支持；solid 时不支持 | 非 solid 时支持；solid 时不支持 |

> **【推断】** 对 7.5 万条目的库来说，这张表意味着：ZIP 最友好（O(1) 定位 + 逐文件独立）；7z 次之（头部一次性给出全部映射，可做最优调度）；RAR 最差（列目录要扫全文，solid 时无法调度）。

---

## 1.1 ZIP

**规范**：PKWARE APPNOTE.TXT 6.3.10（2022-11-01），PKWARE 官方发布
<https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.10.TXT>

### 1.1.1 容器元数据里存了什么

**Central directory file header（§4.3.12，定长部分 46 字节）** —— 规范原文逐字段：

```
central file header signature   4 bytes  (0x02014b50)   +0x00
version made by                 2 bytes                 +0x04
version needed to extract       2 bytes                 +0x06
general purpose bit flag        2 bytes                 +0x08
compression method              2 bytes                 +0x0A
last mod file time              2 bytes                 +0x0C
last mod file date              2 bytes                 +0x0E
crc-32                          4 bytes                 +0x10   ★
compressed size                 4 bytes                 +0x14
uncompressed size               4 bytes                 +0x18   ★
file name length                2 bytes                 +0x1C
extra field length              2 bytes                 +0x1E
file comment length             2 bytes                 +0x20
disk number start               2 bytes                 +0x22
internal file attributes        2 bytes                 +0x24
external file attributes        4 bytes                 +0x26
relative offset of local header 4 bytes                 +0x2A   ★
file name (variable size)                               +0x2E
extra field (variable size)
file comment (variable size)
```

> 偏移量由规范给出的字段长度累加得出，并与 RetroArch 的 ZIP 后端逐字节吻合 —— [`archive_file_zlib.c`](https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file_zlib.c) 里就是 `read_le(entry + 16, 4) /* CRC32 */`、`read_le(entry + 20, 4) /* compressed size */`、`read_le(entry + 24, 4) /* uncompressed size */`、`read_le(entry + 42, 4) /* relative offset of local file header */`。

**End of central directory record（§4.3.16）**：`+0x00` signature `0x06054b50`；`+0x0A` total entries（2）；`+0x0C` size of central directory（4）；`+0x10` offset of start of central directory（4）；`+0x14` ZIP file comment length（2）。定位方法是从文件尾向前扫描找 signature。

**Local file header（§4.3.7，定长 30 字节）**：signature `0x04034b50`、version needed、flag、method、time、date、**crc-32**、compressed size、uncompressed size、file name length、extra field length，随后是文件名与 extra field，再往后才是数据。

### 1.1.2 校验和算法：是 CRC-32，且规范逐字定义了它

APPNOTE §4.4.7 原文：

> "The CRC-32 algorithm was generously contributed by David Schwaderer… The **'magic number' for the CRC is 0xdebb20e3**. The proper CRC pre and post conditioning is used, meaning that the **CRC register is pre-conditioned with all ones (a starting value of 0xffffffff)** and the value is **post-conditioned by taking the one's complement** of the CRC residual."

即标准 IEEE 802.3 CRC-32（`0xEDB88320` 反射多项式），与 No-Intro / Redump DAT 里的 `crc` 是同一个算法。**这意味着 ZIP 的 central directory CRC 可以直接拿去比 DAT。**

### 1.1.3 ⚠ 关键陷阱：必须读 central directory，不能读 local header

APPNOTE §4.4.4 general purpose bit flag 的 **Bit 3**：

> "If this bit is set, the fields **crc-32, compressed size and uncompressed size are set to zero in the local header**. The correct values are put in the **data descriptor** immediately following the compressed data. (Note: … newer versions of PKZIP recognize this bit for any compression method.)"

**流式写入的 ZIP（很多打包工具、很多 HTTP 下载得到的 zip）local header 里的 CRC 与大小全是 0。** 识别管线必须走 central directory，绝不能图省事读 local header。

### 1.1.4 ⚠ ZIP64 陷阱

APPNOTE §4.5.3：当 local 或 central directory 记录里的 size / offset 字段放不下时，真实值存在 **Zip64 Extended Information Extra Field（tag `0x0001`）**里，而对应的原字段被置为 `0xFFFF` / `0xFFFFFFFF` 哨兵值：

> "The order of the fields in the zip64 extended information record is fixed, but the fields MUST only appear if the corresponding Local or Central directory record field is **set to 0xFFFF or 0xFFFFFFFF**."

> extra field 内布局：`0x0001`(2) + Size(2) + Original Size(8) + Compressed Size(8) + Relative Header Offset(8) + Disk Start Number(4)。

**【推断】** 本项目里单个 ROM 超 4 GB 的情况存在（PS3 / Wii U / Xbox360 镜像），因此 ZIP64 解析不是可选项。选 crate 时必须确认它处理 ZIP64。

### 1.1.5 零解压可得清单（ZIP）

| 信息 | 可得？ | 出处 |
|---|---|---|
| 内部文件名（含路径） | ✅ | central header + `file name` |
| 未压缩大小 | ✅ | `uncompressed size`（ZIP64 时看 extra field） |
| 压缩后大小 | ✅ | `compressed size` |
| 压缩方法（0=store / 8=deflate / 14=LZMA / 93=zstd…） | ✅ | `compression method` |
| **CRC-32** | ✅ | `crc-32` |
| MD5 / SHA-1 | ❌ | 格式里不存在该字段 |
| 内部文件数据的起点 | ✅ | `relative offset of local header` + 30 + nameLen + extraLen |

---

## 1.2 7z

**规范**：7-Zip 官方源码仓库 `DOC/7zFormat.txt`（"7z Format description (18.06)"）
<https://github.com/ip7z/7zip/blob/main/DOC/7zFormat.txt>
**参考实现**：LZMA SDK `C/7z.h` <https://github.com/ip7z/7zip/blob/main/C/7z.h>
**官方格式介绍页**：<https://7-zip.org/7z.html>（明列 "Solid compressing" 与 "Archive headers compressing" 为格式特性）

### 1.2.1 结构总览（规范原文）

```
Archive structure
~~~~~~~~~~~~~~~~~
SignatureHeader
[PackedStreams]
[PackedStreamsForHeaders]
[
  Header
  or
  {
    Packed Header
    HeaderInfo
  }
]
```

**SignatureHeader（32 字节，`k7zStartHeaderSize 0x20`）**：

```
BYTE kSignature[6] = {'7','z',0xBC,0xAF,0x27,0x1C};   +0x00
BYTE Major; BYTE Minor;                                +0x06
UINT32 StartHeaderCRC;                                 +0x08
StartHeader {
  REAL_UINT64 NextHeaderOffset                         +0x0C
  REAL_UINT64 NextHeaderSize                           +0x14
  UINT32      NextHeaderCRC                            +0x1C
}
```

**元数据位置**：`NextHeaderOffset`（相对 SignatureHeader 之后）指向 Header。若该处是 `kEncodedHeader (0x17)`，说明**头部本身被压缩过**，要先按 `StreamsInfo` 解一次（通常几十 KB 的 LZMA）才能拿到真正的 Header。这是 7-Zip 默认行为（"Archive headers compressing"）。

### 1.2.2 CRC 存在哪里：`kCRC` 属性（`0x0A`）

规范里 CRC 出现在**三个层级**：

```
PackInfo
  [] BYTE NID::kCRC (0x0A)   PackStreamDigests[NumPackStreams]  []   ← 压缩后流的 CRC

Coders Info (kUnPackInfo)
  [] BYTE NID::kCRC (0x0A)   UnPackDigests[NumFolders]          []   ← 每个 folder 解压后整体的 CRC

SubStreams Info (kSubStreamsInfo)
  [] BYTE NID::kNumUnPackStream (0x0D)  UINT64 NumUnPackStreamsInFolders[NumFolders]  []
  [] BYTE NID::kSize (0x09)             UINT64 UnPackSizes[]                          []
  [] BYTE NID::kCRC  (0x0A)             Digests[Number of streams with unknown CRC]   []   ← ★ 每个文件的 CRC
```

`Digests` 结构：

```
Digests (NumStreams)
  BYTE AllAreDefined
  if (AllAreDefined == 0) { for(NumStreams) BIT Defined }
  UINT32 CRCs[NumDefined]
```

**结论**：
- ✅ **7z 的 header 里存每个内部文件的 CRC**，位于 `SubStreamsInfo` 的 `kCRC`，宽度 `UINT32`。7-Zip 实现为 CRC-32（LZMA SDK `C/7z.h` 里对应字段是 `CSzBitUi32s CRCs;`；`sevenz-rust2` 的公开 API 也把它标注为 "CRC32 checksum of uncompressed data"）。
- ⚠ **规范上 `kCRC` 是可选块**（用 `[]` 标注），且 `Digests` 里还有逐流的 `Defined` 位。**【推断】** 7-Zip 自己压缩时总会写 CRC，但解析器必须处理「某条目没有 CRC」的情况，不能假定一定有。
- ✅ **未压缩大小**：`SubStreamsInfo.kSize` 的 `UnPackSizes[]`；SDK 里表现为 `SzArEx_GetFileSize(p,i) = UnpackPositions[i+1] - UnpackPositions[i]`。
- ✅ **文件名**：`FilesInfo` 的 `kNames (0x11)`，UTF-16LE，NUL 结尾。
- ❌ MD5 / SHA-1：格式里不存在。

### 1.2.3 folder = solid block，且头部直接给出映射

LZMA SDK `C/7z.h` 的 `CSzArEx`：

```c
typedef struct
{
  CSzAr db;
  ...
  UInt64 *UnpackPositions;  // NumFiles + 1
  Byte *IsDirs;
  CSzBitUi32s CRCs;
  ...
  UInt32 *FolderToFile;   // NumFolders + 1
  UInt32 *FileToFolder;   // NumFiles      ← ★
  size_t *FileNameOffsets;
  Byte *FileNames;  /* UTF-16-LE */
} CSzArEx;
```

**`FileToFolder[]` 与 `UnpackPositions[]` 是零解压可得的**，它们直接告诉你：每个文件属于哪个 solid block、在该 block 解压流里的起止偏移。这是做最优调度的全部所需信息。

判定某个 folder 是不是 solid：`SubStreamsInfo.NumUnPackStreamsInFolders[i] > 1`。

---

## 1.3 RAR4 与 RAR5

**RAR 5.0 官方规范**：<https://www.rarlab.com/technote.htm>（RARLAB 官方，只覆盖 RAR 5.0/7.0）
**RAR 4.x**：RARLAB **没有**发布公开规范。**【未找到权威来源】** —— 唯一可用的一手来源是 UnRAR 官方源码本身。本文 RAR4 部分全部来自 unrar 源码的实际解析代码：
- <https://github.com/aawc/unrar/blob/master/headers.hpp>（旗标与结构定义）
- <https://github.com/aawc/unrar/blob/master/arcread.cpp>（实际读取顺序）
- <https://github.com/aawc/unrar/blob/master/hash.hpp>（哈希类型）

> 注：`aawc/unrar` 是 RARLAB 官方 unrar 源码包的 GitHub 镜像。**【推断】** 它与 rarlab.com 下载的 `unrarsrc` 内容一致（文件名、许可文本、结构定义都对得上），但严格说不是 rarlab.com 自身托管。

### 1.3.1 RAR4 file header 的实际布局

RAR4 每个 block 前 7 字节是 `SIZEOF_SHORTBLOCKHEAD`（`headers.hpp`）：`HeadCRC`(2) + `HeaderType`(1) + `Flags`(2) + `HeadSize`(2)。文件头 type = `HEAD3_FILE = 0x74`。

`arcread.cpp` 的 `HEAD_FILE` 分支读取顺序（`SIZEOF_FILEHEAD3 = 32`）：

```c
hd->DataSize  = Raw.Get4();     // PackSize          block 起始 +7
uint LowUnpSize = Raw.Get4();   // UnpSize(low 32)              +11
hd->HostOS    = Raw.Get1();                                    //+15
hd->FileHash.Type = HASH_CRC32;
hd->FileHash.CRC32 = Raw.Get4();   // ★ CRC32，无条件读取        +16
uint FileTime = Raw.Get4();                                    //+20
hd->UnpVer    = Raw.Get1();                                    //+24
hd->Method    = Raw.Get1()-0x30;                               //+25
size_t NameSize = Raw.Get2();                                  //+26
hd->FileAttr  = Raw.Get4();                                    //+28
// if (Flags & LHD_LARGE) : HighPackSize(4) + HighUnpSize(4)
// then FileName[NameSize]
```

> **RAR4 的 CRC32 是无条件存在的**（源码里没有任何 flag 判断，直接 `hd->FileHash.Type=HASH_CRC32`）。这与 RAR5 不同。

**RAR4 相关旗标**（`headers.hpp`）：

| 常量 | 值 | 含义 |
|---|---|---|
| `MHD_VOLUME` | `0x0001` | 分卷 |
| `MHD_SOLID` | `0x0008` | **归档级 solid** |
| `MHD_NEWNUMBERING` | `0x0010` | 新式分卷命名（`.partNN.rar`） |
| `MHD_PASSWORD` | `0x0080` | 头部加密 |
| `LHD_SPLIT_BEFORE` | `0x0001` | 该文件数据从上一卷续来 |
| `LHD_SPLIT_AFTER` | `0x0002` | 该文件数据续到下一卷 |
| `LHD_PASSWORD` | `0x0004` | 数据加密 |
| `LHD_SOLID` | `0x0010` | **该文件是 solid 流的一部分** |
| `LHD_LARGE` | `0x0100` | 使用 64 位大小字段 |
| `LHD_UNICODE` | `0x0200` | 文件名含 Unicode 编码段 |

### 1.3.2 RAR5 file header —— 官方规范逐字段

RAR5 头部全部使用变长整数（vint），**因此不存在固定偏移量，只有固定顺序**。technote.htm 原文（file/service header）：

| 字段 | 类型 | 说明 |
|---|---|---|
| Header CRC32 | uint32 | 头部自身的 CRC32 |
| Header size | vint | |
| Header type | vint | 2 = file header，3 = service header |
| Header flags | vint | 通用旗标 |
| Extra area size | vint | 仅当 header flag `0x0001` 置位 |
| Data size | vint | 仅当 header flag `0x0002` 置位；**对 file header 即打包后大小** |
| **File flags** | vint | `0x0001` 目录 / `0x0002` Unix 时间存在 / **`0x0004` CRC32 字段存在** / `0x0008` 未压缩大小未知 |
| **Unpacked size** | vint | 解压后大小 |
| Attributes | vint | |
| mtime | uint32 | 仅当 file flag `0x0002` |
| **Data CRC32** | uint32 | **仅当 file flag `0x0004` 置位** ★ |
| Compression information | vint | 见下 |
| Host OS | vint | |
| Name length | vint | |
| **Name** | ? bytes | **UTF-8，无结尾 NUL** |
| Extra area | ... | 见下 |
| Data area | ... | |

**Compression information 的位域**（规范原文）：
- 低 6 位（`0x003f`）压缩算法版本（0 = RAR5，1 = RAR7）
- **第 7 位（`0x0040`）solid 旗标** —— 原文："If it is set, RAR **continues to use the compression dictionary left after processing preceding files**."
- 第 8–10 位（`0x0380`）压缩方法，0 = 不压缩
- 第 11–15 位（`0x7c00`）最小字典大小 = `128 KB * 2^N`

### 1.3.3 ⭐ RAR5 存的是 CRC32 还是 BLAKE2sp？—— 两者都可能，CRC32 是默认

technote.htm 的 **File hash record** 一节原文：

> "**Only the standard CRC32 checksum can be stored directly in file header. If other hash is used, it is stored in this extra area record:**
> Size(vint) / Type(vint)=`0x02` / **Hash type(vint): `0x00` = BLAKE2sp hash function** / Hash data: **32 bytes of BLAKE2sp** for `0x00` hash type."

unrar 源码完全对应：

```c
// headers5.hpp
#define FHFL_CRC32          0x0004 // CRC32 field is present.
#define FHEXTRA_HASH        0x02   // File hash.
#define FHEXTRA_HASH_BLAKE2 0x00

// hash.hpp
enum HASH_TYPE {HASH_NONE, HASH_RAR14, HASH_CRC32, HASH_BLAKE2};
...
blake2sp_state *blake2ctx;

// arcread.cpp
hd->FileHash.Type=HASH_NONE;
if ((hd->FileFlags & FHFL_CRC32)!=0) { hd->FileHash.Type=HASH_CRC32; hd->FileHash.CRC32=Raw.Get4(); }
...
case FHEXTRA_HASH:
  { uint Type=(uint)Raw->GetV();
    if (Type==FHEXTRA_HASH_BLAKE2)
    { hd->FileHash.Type=HASH_BLAKE2;
      Raw->GetB(hd->FileHash.Digest,BLAKE2_DIGEST_SIZE); } }
```

**结论**：
- RAR5 的 **CRC32 是可选的**（由 file flag `0x0004` 决定），但**默认存在**（WinRAR 默认用 CRC32）。
- **BLAKE2sp 是可选的 extra area 记录**（type `0x02`），只有用户显式要求更强校验时才写入。**两者不会同时用于同一文件** —— unrar 的 `FileHash` 是一个 union，后读到的 BLAKE2 会覆盖 `Type`。
- **BLAKE2sp 对识别管线毫无用处**：No-Intro / Redump / TOSEC 的 DAT 里没有 BLAKE2 字段。**如果一个 RAR5 只写了 BLAKE2sp 而没写 CRC32，那这个容器就退化成「必须完整解压才能识别」。** 这是一个必须显式处理的分支。

### 1.3.4 ⚠ 分卷 RAR 的 CRC 语义会变

technote.htm 关于 **Data CRC32** 的原文：

> "CRC32 of unpacked file or service data. **For files split between volumes it contains CRC32 of file packed data contained in current volume for all file parts except the last.**"

File hash record（BLAKE2sp）同样：

> "For files split between volumes it contains a hash of file **packed** data contained in current volume for all file parts except the last. **For files not split between volumes and for last parts of split files it contains an unpacked data hash.**"

**这意味着：在分卷 RAR 里，只有含 `LHD_SPLIT_AFTER` / `HFL_SPLITAFTER` 为假的那个「最后一段」的文件头，其 CRC 才是整个未压缩文件的 CRC。** 中间卷的 CRC 是**打包后数据**的 CRC，拿去比 DAT 必然全部落空。

RAR4 的分卷 CRC 语义 —— **【未找到权威来源】**（technote 只写 RAR5）。**【推断】** 从 unrar 的提取流程看行为应当一致，但不要在代码里依赖这一点，遇到 `LHD_SPLIT_*` 一律走「找最后一段」的路径。

### 1.3.5 零解压可得清单（RAR）

| 信息 | RAR4 | RAR5 |
|---|---|---|
| 文件名 | ✅（`LHD_UNICODE` 时需解 unrar 的私有 Unicode 编码） | ✅ UTF-8 直读 |
| 未压缩大小 | ✅（`LHD_LARGE` 时 64 位） | ✅（`FHFL_UNPUNKNOWN` 时无效） |
| **CRC-32** | ✅ 无条件存在 | ⚠ 仅当 `FHFL_CRC32`（默认置位） |
| BLAKE2sp | ❌ 格式无此项 | ⚠ 可选 extra 记录 |
| MD5 / SHA-1 | ❌ | ❌ |
| solid 标志 | ✅ `LHD_SOLID` / `MHD_SOLID` | ✅ CompInfo bit `0x0040` / archive flag `0x0004` |
| 分卷续接标志 | ✅ `LHD_SPLIT_BEFORE/AFTER` | ✅ `HFL_SPLITBEFORE/AFTER` |
| 快速定位（免全文扫描） | ❌ | ⚠ 可选：main header extra area 的 **Locator record（`0x01`）**带 quick open 偏移；规范原文："**This record is optional. If it is missing, it is still necessary to scan the entire archive**" |

---

## 1.4 不解压能否拿到 SHA-1 / MD5

**答案：三种格式全部不能。已逐一确认到「格式里根本没有这个字段」的级别。**

| 格式 | 元数据里的校验和 | 有 MD5？ | 有 SHA-1？ |
|---|---|---|---|
| ZIP | CRC-32（APPNOTE §4.4.7） | ❌ | ❌ |
| 7z | `kCRC` UINT32（7zFormat.txt Digests 结构） | ❌ | ❌ |
| RAR4 | CRC-32（`arcread.cpp` 无条件读） | ❌ | ❌ |
| RAR5 | CRC-32（可选）+ **BLAKE2sp**（可选 extra 记录） | ❌ | ❌ |

> ZIP 的 Strong Encryption 扩展会用到 SHA-1，但那是密钥派生用的，与文件内容哈希无关。7z 的 AES 编码器同样。**没有任何一种归档格式把内容的 SHA-1/MD5 存进元数据。**

**推论（本项目最重要的一条）**：如果识别管线的第一道命中层坚持用 SHA-1，那么 **10 TB 的 zip/7z/rar 必须全量解压**，这在物理上就决定了项目不可行。**改用 `CRC-32 + 未压缩大小` 作为第一层，代价降到「只读头部」。**

**关于 CRC-32 碰撞**：CRC-32 是 32 位，生日界约 2^16 ≈ 6.5 万条 —— 7.5 万条目的库里，**纯 CRC-32 碰撞在理论上是会发生的**。但 DAT 匹配时同时约束 `size`，两者同时相同的概率极低。**【推断】** 实践中「CRC + size」双约束足以作为自动通过的依据；只有当同一 (crc,size) 命中多条 DAT 记录时，才需要落到完整解压 + SHA-1 做仲裁。这与 [B.4 libretro-database 的做法](./rom-identification.md#b4-libretro-database-与-retroarch-的实现) 一致 —— RetroArch 的扫描器就是纯 CRC + size 匹配的。

---

## 1.5 部分解压：读内部文件前 N 字节的代价

需求场景（见 [rom-identification.md 第 A 章](./rom-identification.md)）：
- **NES iNES 头**：前 16 字节
- **GBA game code**：前 `0xC0` 字节
- **N64 / MD / PCE 等**：前几 KB
- **SFC 内部头**：`0x7FC0`（LoROM）/ `0xFFC0`（HiROM）/ **`0x40FFC0`（ExHiROM ≈ 4.06 MB）** —— 最坏情况要读到 4 MB
- **NDS 头**：前 `0x200` 字节

### 1.5.1 各格式的代价

| 场景 | 代价 | 依据 |
|---|---|---|
| **ZIP，method = 0 (stored)** | **O(1)**：`local header offset + 30 + nameLen + extraLen + 目标偏移`，直接 seek | APPNOTE §4.3.7/4.3.8；RetroArch `archive_file_zlib.c` 里 `offsetData = cdata + 26 + 4 + nl + el` |
| **ZIP，method = 8 (deflate)** | 从条目数据起点流式 inflate，读够 N 字节即中止。**代价 ∝ N**，与文件总大小无关 | RFC 1951 deflate 是流式格式；`zip` crate 的 `ZipFile` 实现 `Read` |
| **7z，非 solid folder** | 从该 folder 起点流式解压到 N。代价 ∝ N | 7zFormat.txt；folder 内只有 1 个 substream |
| **7z，solid folder** | **必须从 folder 起点解压到「该文件在 folder 内的起始偏移 + N」** | 见 1.6 |
| **RAR，非 solid** | 从该文件数据起点流式解压到 N。代价 ∝ N | |
| **RAR，solid** | **必须从该 solid 组的第一个非 solid 文件开始，把它之前的所有文件全部解压完** | technote.htm："RAR continues to use the compression dictionary left after processing preceding files" |

### 1.5.2 各编解码器能否「解到一半就停」

| 编解码器 | 可流式提前中止？ | 备注 |
|---|---|---|
| Store / Copy | ✅（甚至可直接 seek） | |
| Deflate | ✅ | 位流解码器，任何时刻可停 |
| BZip2 | ✅（按 900 KB block 粒度） | |
| LZMA / LZMA2 | ✅ | 解码器是有状态的流式机器；**【推断】** LZMA2 的 chunk header 理论上带 dictionary reset 位、可作为重启点，但 7-Zip 的 folder 解码器不对外暴露这些重启点，所以实践中只能从头解 |
| **PPMd** | ✅ 可提前中止，但**严格顺序、状态无法跳跃** | PPMd 是上下文混合模型，没有任何重启点 |
| BCJ / BCJ2 / Delta 等 filter | ✅ | 但 BCJ2 是 4 输入流的复杂 coder（`Is Complex Coder` 位），实现成本高于普通 filter |

**共同结论**：**没有任何一种压缩编解码器支持「跳到中间直接解」。** 所有「部分解压」本质上都是「从某个起点顺序解到目标位置然后停」。因此**决定代价的唯一变量是「起点到目标位置的距离」**，而这个距离由 solid 与否决定。

### 1.5.3 ⚠ SFC ExHiROM 的特殊代价

读 `0x40FFC0` 意味着要顺序解压 **4 MB 以上**。**【推断】** 但这个代价可以规避：
- 先用零解压得到的 `未压缩大小` 判断 —— ROM ≤ 4 MB 时不可能是 ExHiROM，直接排除；
- 而且如果 `CRC + size` 已经命中 DAT，压根不需要读内部头。**内部头只在 CRC 未命中（汉化版 / 魔改版）时才需要**，而那时本来就要完整解压来算 SHA-1 与做结构指纹。

---

## 1.6 ⭐ solid block：本项目最大的性能陷阱

### 1.6.1 官方来源确认「读一个小文件要解压整块」

**7z —— 官方 LZMA SDK `C/7z.h` 的注释与签名（这是最硬的证据）**：

```c
/*
  SzArEx_Extract extracts file from archive
  *outBuffer must be 0 before first call for each new archive.
  Extracting cache:
    If you need to decompress more than one file, you can send
    these values from previous call:
      *blockIndex, *outBuffer, *outBufferSize
    You can consider "*outBuffer" as cache of solid block.
    If your archive is solid, it will increase decompression speed.
*/
SRes SzArEx_Extract(
    const CSzArEx *db,
    ILookInStreamPtr inStream,
    UInt32 fileIndex,         /* index of file */
    UInt32 *blockIndex,       /* index of solid block */     ← ★
    Byte **outBuffer,         /* pointer to pointer to output buffer */
    size_t *outBufferSize,    /* buffer size for output buffer */
    size_t *offset,           /* offset of stream for required file in *outBuffer */  ← ★
    size_t *outSizeProcessed, /* size of file in *outBuffer */
    ISzAllocPtr allocMain, ISzAllocPtr allocTemp);
```

`outBuffer` 装的是**整个 solid block 解压后的内容**，目标文件只是其中 `*offset` 起 `*outSizeProcessed` 字节。**官方 API 本身就承认「一次解压一整块」是唯一路径**，并把「复用上一次的块缓存」作为唯一优化手段。

同时 `SzAr_DecodeFolder()` 的签名也是 folder 级的：

```c
SRes SzAr_DecodeFolder(const CSzAr *p, UInt32 folderIndex,
    ILookInStreamPtr stream, UInt64 startPos,
    Byte *outBuffer, size_t outSize, ISzAllocPtr allocMain);
```

**纯 Rust 实现的说法一致** —— `sevenz-rust2` README：

> "**Solid compression**: Solid archives can in theory provide better compression rates, but **decompressing a file needs all previous data to also be decompressed**."
> <https://github.com/hasenbanck/sevenz-rust>

**RAR** —— rarlab technote.htm 对 Compression information bit `0x0040` 的定义：

> "7th bit (0x0040) defines the **solid flag**. If it is set, RAR **continues to use the compression dictionary left after processing preceding files**."

### 1.6.2 风险有多大：一个具体的量化

**【推断】**（基于上述事实的推理，未实测）：假设一个 7z 里放了 200 个 SFC ROM，总解压后 400 MB，被压成一个 solid folder。
- **朴素实现**（对每个文件独立调用「解压这一个」）：解压量 ≈ Σ(每个文件在块内的结束偏移) ≈ **200 × 400MB / 2 = 40 GB**，即 100× 放大。
- **按 folder 分组的实现**：解压量 = **400 MB**，1× 。

**这个 100× 的差距，就是「项目能不能在可接受时间内跑完 10 TB」的分水岭。**

### 1.6.3 ⭐ 关键好消息：solid 与否可以零解压判定，且 7z 能做最优调度

**7z 的判定与调度（完全可行）**：

1. 读 header（可能要解一次 `kEncodedHeader`，几十 KB 级）。
2. `SubStreamsInfo.NumUnPackStreamsInFolders[i]` → folder `i` 里有几个文件。**>1 即 solid**。
3. `CSzArEx.FileToFolder[fileIndex]` → 每个文件属于哪个 folder。
4. `CSzArEx.UnpackPositions[]` → 每个文件在其 folder 解压流里的起止偏移。
5. **调度策略**：把待处理文件按 folder 分组 → 每个 folder 只调用一次 `SzAr_DecodeFolder`（或 `SzArEx_Extract` 时始终复用 `blockIndex`/`outBuffer` 缓存）→ 在内存里一次性切出该 folder 内所有目标文件并算哈希。
6. **内存约束**：`SzAr_GetFolderUnpackSize(p, folderIndex)` 零解压可得。若某个 folder 解压后超过内存预算，退化为「流式解压 + 边流边喂给多个 hasher」—— 因为一个 folder 内的文件在解压流里是**按顺序连续排列**的，一次顺序流即可给所有文件算哈希，**根本不需要把整块驻留内存**。

> **这是本文最有工程价值的一条**：7z solid block 的性能陷阱是**完全可解**的，代价是必须自己做调度，不能用「按文件名解压一个文件」这种简单 API。

**RAR 的判定与调度（部分可行，明显更差）**：

1. 逐 block 扫描全文拿到文件列表（每个 header 读完 seek 过它的 `PackSize`）。
2. 逐文件的 solid 标志：RAR4 `LHD_SOLID`、RAR5 CompInfo bit `0x0040`。
3. **solid 组的边界 = 从某个 solid 标志为假的文件开始，到下一个 solid 标志为假的文件之前**。
4. **调度策略**：把整个 solid 组按顺序一次性提取（`unrar` 的顺序 iterator 天然如此），边提取边给每个文件算哈希。
5. ⚠ **RAR 头部没有 7z 那样的「文件 → 块 → 块内偏移」索引**，所以只能顺序走，无法跳过组内不关心的文件。
6. ⚠ `unrar` crate 的 API 是「打开归档 → 顺序迭代」，天然契合这个模式；**千万不要用「按文件名逐个 extract」的循环**。

**ZIP 没有这个问题** —— 每个条目独立压缩，可以任意顺序、任意并行处理。

### 1.6.4 规避 solid 陷阱的工程手段清单

| 手段 | 说明 |
|---|---|
| **头部先行** | 任何容器，第一步永远是「只读头部」。拿到 `(名字, 大小, CRC32, 是否 solid, 属于哪个块)` 之后再决策。 |
| **CRC-32 优先命中** | 如果 CRC+size 已命中 DAT，**根本不需要解压**，solid 与否无关紧要。这条能消灭绝大多数工作量。 |
| **按块分组、单趟流式** | 必须解压时，按 folder / solid 组分组，一个块只解一次，一次顺序流同时喂多个 hasher。 |
| **块级并行而非文件级并行** | 并行度设在「块」这一层。文件级并行会导致同一块被多个线程重复解压。 |
| **内存预算护栏** | `SzAr_GetFolderUnpackSize` 零解压可得；超预算就走流式而非缓冲。 |
| **重打包（治本）** | 对已识别的条目，重打包成 **非 solid 的 7z（`-ms=off`）或 zip**，之后所有增量扫描都变成 O(1)。见第 5 部分。 |
| **持久化** | 一旦为某个容器内某文件算出过 SHA-1，就把 `(容器路径, 容器 mtime+size, 内部路径, crc, sha1)` 落库。第二次扫描只读头部对 CRC，永不重复解压。 |

---

## 1.7 分卷压缩

### 1.7.1 三种完全不同的机制，必须分清

| 命名 | 机制 | 处理方式 |
|---|---|---|
| `name.z01`, `name.z02`, …, `name.zip` | **ZIP 官方的 split archive**（APPNOTE §8） | 各段不是独立 zip；**central directory 在最后那个 `.zip` 里** |
| `name.part1.rar`, `name.part2.rar` / `name.rar`, `name.r00`, `name.r01` | **RAR 官方分卷** | 文件数据跨卷续接，靠 `SPLIT_BEFORE/AFTER` 标志串接 |
| `name.7z.001`, `name.zip.001`, `name.iso.001` | **7-Zip 的通用「Split」格式** —— 就是把任意一个文件按字节切成等长块 | **各段拼接（`cat`）起来就是原文件** |

### 1.7.2 ZIP split（`.z01` … `.zip`）

APPNOTE §8.3.3 / §8.3.4 原文：

> "To avoid name collisions, split archives are named as follows:
> **Segment 1 = filename.z01 / Segment n-1 = filename.z(n-1) / Segment n = filename.zip**"
> "§8.3.4 The **.ZIP extension is used on the last segment to support quickly reading the central directory**."

§8.3.1 还区分了 spanned（各段同名，DOS 卷标编号）与 split（同目录、不同扩展名）两种。

**处理要点**：入口是那个 `.zip`（最后一段），`disk number start` 字段告诉你条目数据在第几段。**【推断】** 大多数 Rust zip crate 不支持 split archive；实践中最省事的做法是先用外部工具合并（`zip -s 0 name.zip --out merged.zip`）或直接跳过并标记为「需人工处理」。

### 1.7.3 RAR 分卷（`.partNN.rar` / `.rNN`）

unrar `pathfn.cpp` 的 `NextVolumeName()` 明确了两套方案：

```c
void NextVolumeName(std::wstring &ArcName, bool OldNumbering)
{
  ...
  if (!OldNumbering)          // 新式：MHD_NEWNUMBERING / RAR5 默认
  {
    size_t NumPos = GetVolNumPos(ArcName);   // 定位 .partNN. 里的数字
    while (++ArcName[NumPos]=='9'+1) { ArcName[NumPos]='0'; ... 
      // Convert .part:.rar (.part9.rar after increment) to part10.rar.
      ArcName.insert(NumPos+1,1,'1'); ... }
  }
  else                        // 旧式
  {
    if (!IsDigit(ArcName[DotPos+2]) || !IsDigit(ArcName[DotPos+3]))
      ArcName.replace(DotPos+2, npos, L"00");   // From .rar to .r00.
    else { ... ArcName[NumPos]='a'; // From .999 to .a00 ... }
  }
}
```

`GetVolNumPos()` 的注释还明确它能处理 `name.part##of##.rar` 这类命名。

**处理要点**：
- 入口必须是**第一卷**（`.part1.rar` 或 `.rar`），不是任意一卷。RAR4 用 `MHD_FIRSTVOLUME (0x0100)` 标识首卷。
- 跨卷文件的 CRC 语义见 1.3.4 —— **只有最后一段的 CRC 才是未压缩数据的 CRC**。
- `unrar` crate 提供 `open_for_listing_split` / `ListSplit` 模式来处理分卷（README 明确列出）。

### 1.7.4 `.001` 系列 —— 其实是最简单的

7-Zip 源码里有一个专门的 **Split handler**，注册名与扩展名如下（`CPP/7zip/Archive/SplitHandler.cpp` 末尾的格式注册）：

```c
"Split", "001", NULL, 0xEA,
```

它的 `CSeqName::GetNextName()` 就是把末尾的数字（或字母）加一。也就是说：**`.001/.002/.003` 是「把一个文件按字节切开」，不是压缩格式，各段没有独立头部。**

**处理要点**：
- **`cat name.7z.001 name.7z.002 … > name.7z` 即可完全还原。** 不需要 7-Zip。
- 对识别管线：把 `xxx.001 … xxx.NNN` 视为**一个逻辑文件**，用一个「拼接读取器」（`Read + Seek` 的多文件适配器）喂给下游解析器，**不必真的在磁盘上合并**。
- `xxx.iso.001` 这种直接就是被切开的 ISO，拼接后当普通 ISO 处理。

### 1.7.5 ⭐ 分卷对「变体成型规则」的影响

对照 [CONTEXT.md](../../CONTEXT.md) 的术语：**「分卷压缩的多个分卷合起来才是一个透明容器」**。落到成型规则上：

| 规则 | 内容 |
|---|---|
| **聚合键** | 剥掉分卷后缀后的基名。`Game.part1.rar` / `Game.part2.rar` → 键 `Game`；`Game.7z.001..003` → 键 `Game.7z`；`Game.z01`/`Game.zip` → 键 `Game` |
| **主文件** | 必须指向**入口卷**：RAR 新式 → `.part1.rar`（或最小的 partNN）；RAR 旧式 → `.rar`；ZIP split → **`.zip`**（最后一段！）；`.001` → `.001` |
| **附属文件** | 其余所有分卷 |
| **完整性校验** | 分卷缺失是常见故障。RAR 可用 `EARC_NEXT_VOLUME` / `EHFL_NEXTVOLUME` 判断「还有下一卷」；`.001` 系列只能靠「编号连续 + 除最后一段外大小相等」这一启发式（**【推断】**） |
| **不要做的事** | 绝不能把每一卷当成独立变体入库；绝不能对单个分卷算哈希去比 DAT |

---

## 1.8 ⭐ RAR 的许可问题

### 1.8.1 UnRAR license 全文要点

来源：<https://github.com/aawc/unrar/blob/master/license.txt>（与 rarlab 分发的 `unrarsrc` 内 `license.txt` 同源）

原文关键两段：

> "**1.** All copyrights to RAR and the utility UnRAR are exclusively owned by the author - Alexander Roshal."
>
> "**2.** UnRAR source code may be used in any software to handle RAR archives **without limitations free of charge, but cannot be used to develop RAR (WinRAR) compatible archiver and to re-create RAR compression algorithm, which is proprietary**. Distribution of modified UnRAR source code in separate form or as a part of other software is permitted, **provided that full text of this paragraph, starting from "UnRAR source code" words, is included in license, or in documentation if license is not available, and in source code comments of resulting package**."

**逐条解读**：

| 条款 | 对本项目的影响 |
|---|---|
| 「可用于任何处理 RAR 归档的软件，免费无限制」 | ✅ **读取/解压 RAR 是明确许可的**，包括商业用途 |
| 「不得用于开发 RAR 兼容压缩器 / 复现 RAR 压缩算法」 | ✅ 本项目只读不写 RAR，不触碰 |
| 「分发（含修改后的）源码时必须原样附带该段落，且要出现在**许可文件、文档、以及产物包的源码注释**里」 | ⚠ **这是真正的负担**：只要二进制里静态链接了 unrar，就必须在许可声明里带上这段文字 |
| （反推）没有「可自由修改并以任意许可再授权」的条款 | ⚠ **不是 OSI 意义上的开源许可** |

**Debian 的官方归类**：`unrar-nonfree` 位于 **non-free** 组件（<https://tracker.debian.org/pkg/unrar-nonfree>），即不符合 DFSG。

**【推断】** 由此可推出两条工程结论（未找到 FSF/rarlab 的直接声明，故标推断）：
1. **与 GPL 不兼容**：UnRAR license 的「不得用于开发兼容压缩器」是一条 GPL 不允许的使用领域限制。若本项目将来采用 GPL，静态链接 unrar 会有冲突。
2. **发行形态受限**：如果做成「用户自行安装 unrar/WinRAR，本工具通过外部进程调用」，则完全绕开分发义务 —— 这是最保守的路线。

### 1.8.2 Rust 生态里的 RAR 实现盘点

| crate | 版本 / 下载量 / 更新 | 许可 | 实现方式 | 评价 |
|---|---|---|---|---|
| **`unrar`** <https://github.com/muja/unrar.rs> | 0.5.8 / 50.7 万 / 2025-02 | crate 本身 **MIT OR Apache-2.0**；**内嵌 C++ 源码使用其自有许可**（README 原文："The embedded C/C++ library uses its own license. For more informations, see its license file."） | FFI 绑定 + 内嵌 unrar C++ 源码 | **生态里最成熟的**。API 支持 `open_for_listing` / `open_for_listing_split`「只读头部、跳过 payload」。⚠ **UnRAR license 的分发义务随内嵌源码传染** |
| `unrar_sys` | 0.5.8 / 47.5 万 | 同上 | 上者的 `-sys` 层 | |
| **`unrar-ng`** <https://github.com/ttys3/unrar.rs> | 0.7.7 / 1.4 万 / 2026-05 | MIT OR Apache-2.0 | `unrar` 的活跃维护 fork | 同样内嵌 C++ 源码，许可问题相同 |
| **`rars`** <https://github.com/bitplane/rars> | 0.9.3 / 6.6 万 / 2026-08 | MIT OR Apache-2.0 | **纯 Rust**，作者自述覆盖从 DOS 时代 `RE~^` 到 RAR 7 的全部格式，并提供 Python / WASM 绑定 | ⚠ **非常新**（2026-05-13 首发，3 个月 26 个版本）。作者自评："It's a bit slower than WinRAR, it uses more memory and has slightly worse compression. **It could probably use more testing, too.**" **README 未讨论 UnRAR 许可或 cleanroom 实现方法** —— 若其解压算法源自阅读 unrar 源码，许可清白性存疑。**【推断】** 生产环境不建议作为唯一依赖，可作为对照实现 |
| `rar` <https://github.com/Roba1993/RAR> | 0.4.0 / 8972 / 2025-11 | MIT | 纯 Rust，基于 nom | 只有 2 个版本，2018 年创建。**【推断】** 事实上未维护 |
| `nzbdav-rar` | 0.5.7 / 483 | MIT | **纯 Rust 的 RAR4/RAR5 头部解析器**（只解析头部，不解压） | ⭐ 下载量极低但**定位精准**：如果只需要「零解压读 CRC+大小+文件名」，这类库正好够用且完全无许可负担。**【推断】** 需自行审计代码质量 |
| `unarc-rs` <https://github.com/mkrueger/unarc-rs> | 0.6.2 / 1.15 万 | MIT OR Apache-2.0 | 多格式统一提取（7z / ZIP / **RAR** / LHA / ARJ / …） | 未核实其 RAR 实现来源与完整度 —— **【未找到权威来源】** |
| （C 侧参考）**libarchive** | — | BSD-2-Clause | README 原文："**RAR and RAR 5.0 archives (with some limitations due to RAR's proprietary status)**" | melonDS 走的就是这条路。**许可干净**，但 README 自己承认 RAR 支持有限制 |

**推荐路线（见 1.9 汇总）**：
- **零解压阶段**：自己写或用纯 Rust 头部解析器（`nzbdav-rar` 或自实现），**完全避开 UnRAR license**。RAR4/RAR5 的头部结构本文已完整给出，自实现工作量可控。
- **必须解压时**：用 `unrar` crate（成熟度最高），并在产品许可声明里附上 UnRAR 那一段文字；或者退一步走「外部 `unrar` 命令行进程」，把分发义务甩给用户。

---

## 1.9 Rust 生态选型

### 1.9.1 ZIP

| crate | 版本 / 下载量 | 许可 | 只读 header 不解压？ | 备注 |
|---|---|---|---|---|
| **`zip`**（zip-rs/zip2） <https://github.com/zip-rs/zip2> | 9.0.0-pre3 / 2.52 亿 | MIT | ✅ **完全支持** | 事实标准。`ZipArchive::new()` 打开时即解析 central directory；`ZipFile` 提供 `crc32() -> u32`、`size() -> u64`、`compressed_size() -> u64`、`name() -> &str`、`compression() -> CompressionMethod`，**这些都不需要解压**（<https://docs.rs/zip/latest/zip/read/struct.ZipFile.html>） |
| **`rc-zip` / `rc-zip-sync`** <https://github.com/bearcove/rc-zip> | 5.4.1 / 4.4.2，各 160–180 万 | Apache-2.0 OR MIT | ✅ | I/O 无关的 zip 格式实现，适合自定义读取源（比如喂给它一个「多分卷拼接读取器」） |
| `async_zip` | 0.0.19 / 570 万 | MIT | ✅ | 异步场景 |

**结论**：`zip` crate 直接满足需求 —— 打开归档只做两次 seek，即可拿到全部条目的 CRC32 + 大小 + 名字。**这就是 ZIP 的「零解压识别」。**

### 1.9.2 7z

| crate | 版本 / 下载量 | 许可 | 备注 |
|---|---|---|---|
| **`sevenz-rust2`** <https://github.com/hasenbanck/sevenz-rust> | 0.22.2 / 95.8 万 / 2026-08 | **Apache-2.0** | ⭐ 当前活跃维护的分支。纯 Rust。解压支持 COPY / LZMA / LZMA2 / BZIP2 / **PPMD** / BROTLI\* / DEFLATE\* / LZ4\* / ZSTD\*（\*需 feature），filter 支持 BCJ 全系 + BCJ2 + DELTA |
| `sevenz-rust`（原版） | 0.6.1 / 141 万 / 2024-07 | Apache-2.0 | **已停止维护**（`sevenz-rust2` README 原文："This is a fork of the original, **unmaintained** sevenz-rust crate"） |

**⚠ `sevenz-rust2` 的关键限制**：其 `ArchiveEntry` 暴露了 `name`、`crc: u64`（"CRC32 checksum of uncompressed data"）、`compressed_crc`、`size`、`compressed_size`、`has_stream`，**但不暴露该条目属于哪个 folder / solid block**（<https://docs.rs/sevenz-rust2/latest/sevenz_rust2/struct.ArchiveEntry.html>）。

> **【推断】** 这意味着：
> - **零解压读 CRC + 大小 + 名字：`sevenz-rust2` 直接够用。** ✅
> - **做 solid block 最优调度：公开 API 不够。** 需要 (a) 自己解析 7z header 拿 `NumUnPackStreamsInFolders` / `FileToFolder`，或 (b) 检查该 crate 是否有更底层的 `Archive` / `Folder` API（README 未提及），或 (c) 用启发式代替：**「同一 7z 里若条目数 > folder 数」无法直接判断时，退化为「一次性顺序提取全部条目」** —— 对本项目其实完全可接受，因为需要解压时通常就是要处理整个包。

**替代路线**：FFI 到官方 LZMA SDK（`C/7z.h` 的 `SzArEx_*` API），可直接拿到 `FileToFolder` / `blockIndex` / `SzAr_DecodeFolder`。LZMA SDK 是 **public domain**（`7z.h` 头部注释："Igor Pavlov : Public domain"），无任何许可负担。

### 1.9.3 底层编解码器（自实现解析器时会用到）

| crate | 用途 | 下载量 | 许可 |
|---|---|---|---|
| `flate2` | deflate（ZIP method 8、PBP 的 PSAR 块） | 极高 | MIT/Apache-2.0 |
| `lzma-rs` | LZMA / LZMA2 / XZ 纯 Rust | 3660 万 | MIT |
| `ppmd-rust` | PPMd（7z method） | 1749 万 | CC0-1.0 OR MIT-0 |
| `libdeflater` / `zlib-rs` | 高性能 deflate | 高 | — |

### 1.9.4 选型建议汇总

```
零解压层（覆盖 ~全部条目）：
  .zip  → zip crate（ZipArchive::new + 遍历 ZipFile 取 crc32/size/name）
  .7z   → sevenz-rust2 读 ArchiveEntry（crc/size/name）
  .rar  → 自实现 RAR4/RAR5 头部解析器（本文 1.3 已给出完整字段表）
          或 nzbdav-rar；★ 避开 UnRAR license
  .001  → 多文件拼接读取器 → 转交上面三者之一

解压层（只在 CRC 未命中时触发）：
  .zip  → zip crate（逐条目独立，可并行）
  .7z   → sevenz-rust2 顺序全量提取（天然按 folder 顺序，避免重复解压）
          若需精细调度 → FFI LZMA SDK（public domain）
  .rar  → unrar crate 顺序迭代提取（必须顺序！）
          ★ 分发时附带 UnRAR license 段落；或改走外部进程
```

---

# 第 2 部分 · 压缩光盘镜像

> **前提**：按 [CONTEXT.md](../../CONTEXT.md) 的术语，**压缩镜像不是透明容器** —— 它就是变体本身的形态，模拟器直接读它。因此这里问的不是「怎么解包」，而是「不解压能拿到多少识别信息」。

## 2.1 CHD（MAME Compressed Hunks of Data）

**它是什么**：MAME 的通用压缩镜像格式，可容纳 CD-ROM、GD-ROM、DVD、硬盘、LaserDisc。由 **`chdman`**（MAME 官方工具）产生。
**规范来源**：MAME `src/lib/util/chd.h` 头部注释里有 V1–V5 完整的头部布局
<https://github.com/mamedev/mame/blob/master/src/lib/util/chd.h>
**便携实现**：libchdr <https://github.com/rtissera/libchdr>（RetroArch 内置于 `libretro-common/include/libchdr/`）

### 2.1.1 头部是否存原始数据的哈希：✅ 存，且是唯一一个

**V5 头（`chd.h` 原文注释）**：

```
[  0] char   tag[8];         // 'MComprHD'
[  8] uint32_t length;       // length of header (including tag and length fields)
[ 12] uint32_t version;      // drive format version
[ 16] uint32_t compressors[4];// which custom compressors are used?
[ 32] uint64_t logicalbytes; // logical size of the data (in bytes)
[ 40] uint64_t mapoffset;    // offset to the map
[ 48] uint64_t metaoffset;   // offset to the first blob of metadata
[ 56] uint32_t hunkbytes;    // number of bytes per hunk (512k maximum)
[ 60] uint32_t unitbytes;    // number of bytes per unit within each hunk
[ 64] uint8_t  rawsha1[20];  // raw data SHA1              ★
[ 84] uint8_t  sha1[20];     // combined raw+meta SHA1     ★
[104] uint8_t  parentsha1[20];// combined raw+meta SHA1 of parent
[124] (V5 header length)

If parentsha1 != 0, we have a parent (no need for flags)
If compressors[0] == 0, we are uncompressed (including maps)
```

**三个 SHA-1 的语义（`chd.h` 注释逐字）**：

| 字段 | 偏移(V5) | 注释原文 | 语义 |
|---|---|---|---|
| `rawsha1` | 64 | "raw data SHA1" | **压缩前那条「逻辑原始数据流」的 SHA-1** |
| `sha1` | 84 | "combined raw+meta SHA1" | rawsha1 与全部带 checksum 标志的 metadata blob 的组合哈希 |
| `parentsha1` | 104 | "combined raw+meta SHA1 of parent" | 差分 CHD 的父镜像的 combined SHA-1 |

**历史版本的位置不同**（同一注释块，非常容易搞错）：

```
V3: [ 44] md5[16]  [ 60] parentmd5[16]  [ 76] hunkbytes  [ 80] sha1[20]  [100] parentsha1[20]
V4: [ 44] hunkbytes  [ 48] sha1[20](combined)  [ 68] parentsha1[20]  [ 88] rawsha1[20]
V5: [ 64] rawsha1[20]  [ 84] sha1[20](combined)  [104] parentsha1[20]
```

> **V3 是唯一存 MD5 的版本**（`[44] md5[16]` = "MD5 checksum of raw data"）。V4 起 MD5 被移除。

### 2.1.2 ⭐ `chdman info` 的 SHA1 能不能直接比 Redump？—— 不能，两个都不能

**这一点已在 [rom-identification.md B.2.4](./rom-identification.md#b24--chd-与-redump-的哈希关系最易搞错的点) 详述，这里只复述结论并补充「为什么」**：

| 哈希 | 是什么 | 等于某个 Redump 轨道的 SHA-1？ |
|---|---|---|
| `chdman info` 的 `SHA1:` | V5 头 offset 84 = **combined raw+meta** | **否** |
| `chdman info` 的 `Data SHA1:` | V5 头 offset 64 = **rawsha1** | **否** |
| Redump DAT 的 `sha1` | **单条 `.bin` 轨道文件**（以及 `.cue` 文件）的 SHA-1 | 定义如此 |

**根本原因（两条，缺一不可）**：
1. **粒度不同**：CHD 的 raw stream 是**整张盘所有轨道按 CD 帧连续拼接、并按 hunk 边界补齐**的**单一**字节流；Redump 是**逐轨分开**的多个 `.bin` 文件。
2. **扇区布局不同**：chdman 对 CD 采用 **2352 + 96（含 subcode）** 的内部扇区；Redump 的 `.bin` 是纯 2352 用户数据。

**因此**：CHD 与 Redump 之间**没有任何可以直接比对的哈希**。要校验只能 `chdman extractcd --splitbin` 完整解出再逐轨算哈希（且 `.cue` 通常对不上，因为 chdman 是自行重建 cue）。

**同理**：MAME `-listxml` 里 `<disk sha1="...">` 用的是 `chd_file::sha1()` 即 combined SHA-1，**只能校验 MAME 自己的 CHD 集，与 Redump 毫无关系**。

### 2.1.3 不解压能读到什么

| 信息 | 代价 | 说明 |
|---|---|---|
| magic `MComprHD` / version / 压缩器 / `logicalbytes` / `hunkbytes` | **只读 124 字节** | |
| `rawsha1` / `sha1` / `parentsha1` | **只读 124 字节** | 可作为**本地沉淀库的主键**（同一 CHD 必然同 SHA1），也可比对 MAME 的 `<disk>` |
| 轨道表（轨道数、每轨扇区数、类型、pregap） | 读 `metaoffset` 处的 metadata 链，**不需要解 hunk** | metadata tag：`CHT2`（现代 CD）、`CHTR`（旧 CD）、`CHSE`（session）、`CHGD`（GD-ROM）、`DVD `、`GDDD`（硬盘）。格式串见 rom-identification.md B.2.4 |
| **任意扇区内容（如 PS1 的 `SYSTEM.CNF`）** | **先解一次 hunk map，再解 1 个 hunk（≤512 KB）** | 见下 |

**hunk 级随机访问的真实代价** —— `chd.h` 的 V5 map 格式注释：

```
V5 uncompressed map format:
[  0] uint32_t offset;        // starting offset / hunk size

V5 compressed map format header:
[  0] uint32_t length;        // length of compressed map     ← ★ 整个 map 是被压缩存放的
[  4] UINT48   datastart;     // offset of first block
[ 10] uint16_t crc;           // crc-16 of the map
[ 12] uint8_t  lengthbits;    // bits used to encode complength
[ 13] uint8_t  hunkbits;      // bits used to encode self-refs
[ 14] uint8_t  parentunitbits;// bits used to encode parent unit refs
[ 15] uint8_t  reserved;
[ 16] (compressed header length)

Each compressed map entry, once expanded, looks like:
[  0] uint8_t  compression;   // compression type
[  1] UINT24   complength;    // compressed length
[  4] UINT48   offset;        // offset
[ 10] uint16_t crc;           // crc-16 of the data
```

**结论**：
- **未压缩 CHD**（`compressors[0] == 0`）：map 就是每 hunk 一个 `uint32_t offset`，**O(1) 随机访问，零解压**。
- **压缩 CHD**：**必须先把整个 map 解码一次**（一次性成本，与 hunk 数成正比，不与数据量成正比），之后每个 hunk 的 `offset`/`complength` 就是 O(1) 查表，只需解压那 1 个 hunk（≤512 KB）。
- 每个 map 条目还带**该 hunk 数据的 CRC-16**，可做低成本完整性抽检。
- `CHD_CODEC_SELF = 1 // copy of another hunk` 表明 CHD 做 hunk 级去重 —— **【推断】** 解某个 hunk 时可能被重定向到另一个 hunk，实现时要跟随。

> **实用结论**：读 CHD 里 PS1 的 `SYSTEM.CNF` / PS2 的 `SYSTEM.CNF` / Saturn 的 IP.BIN，代价是「解一次 map + 解几个 hunk」，**远低于完整解压**。RetroArch 的扫描器正是这么做的（`task_database_chd_get_serial()`）。

---

## 2.2 CSO / ZSO / DAX

### 2.2.1 CSO（CISO）

**它是什么**：PSP / PS2 用的分块压缩 ISO。原始格式由 BOOSTER 设计。
**事实标准实现 / 规范**：maxcso 的 `README_CSO.md`
<https://github.com/unknownbrackets/maxcso/blob/master/README_CSO.md>
**消费方**：PPSSPP（`Core/FileSystems/BlockDevices.cpp`）、PCSX2（`pcsx2/CDVD/CsoFileReader.cpp`）

**头部（maxcso README_CSO.md 原文，little endian）**：

```
char[4]  magic;             // Always "CISO".
uint32_t header_size;       // Does not always contain a reliable value.
uint64_t uncompressed_size; // Total size of original ISO.
uint32_t block_size;        // Size of each block, usually 2048.
uint8_t  version;           // May be 0 or 1.
uint8_t  index_shift;       // Indicates left shift of index values.
uint8_t  unused[2];         // May contain any values.
```

PPSSPP 源码里的同一结构，带明确偏移注释（`BlockDevices.cpp`）：

```c
typedef struct ciso_header
{
    unsigned char magic[4];    // +00 : 'C','I','S','O'
    u32_le header_size;        // +04 : header size (==0x18)
    u64_le total_bytes;        // +08 : number of original data size
    u32_le block_size;         // +10 : number of compressed block size
    unsigned char ver;         // +14 : version 01
    unsigned char align;       // +15 : align of index value
    unsigned char rsv_06[2];   // +16 : reserved
    // INDEX BLOCK at +18 : uint32 index[0..last+1]
    // DATA BLOCK
} CISO_H;
```

**索引表语义（README 原文）**：
> "The number of index entries can be found by taking `ceil(uncompressed_size / block_size) + 1`."
> "The **lower 31 bits** of each index entry, when shifted left by `index_shift`, indicate the position within the file of the block's compressed data. The length of the block is **the difference between this entry's offset and the following index entry's offset value**."
> "**The high bit of the index entry indicates whether the block is uncompressed.**"
> "blocks are compressed using the **raw deflate** algorithm, with window size being 15"

**CSO v2（README 原文，标注 EXPERIMENTAL）**：`header_size` 必须是 `0x18`，`version` 必须是 2；索引格式相同但高位语义变了 —— 「当压缩块长度 ≥ `block_size` 时必须不压缩；当 < `block_size` 时一定被压缩，**高位置 1 表示 lz4，置 0 表示 deflate**」。

### 2.2.2 ZSO

**规范**：maxcso `README_ZSO.md` <https://github.com/unknownbrackets/maxcso/blob/master/README_ZSO.md>
原文："This format has been proposed by codestation in a patch to procfw." 且明确标注 "**this format is not final, and is experimental**"。

头部与 CSO v1 **完全同构**，只有两点不同：
```
char[4]  magic;             // Always "ZISO".      ← 唯一的结构性差异
uint32_t header_size;       // Always 0x18.
uint64_t uncompressed_size;
uint32_t block_size;
uint8_t  version;           // Always 1.
uint8_t  index_shift;
uint8_t  unused[2];
```
> "Unlike the original CSO format, blocks are compressed using **lz4** rather than deflate."

### 2.2.3 DAX

**规范**：**【未找到独立规范文档】**。一手来源是 maxcso 的 `src/dax.h`
<https://github.com/unknownbrackets/maxcso/blob/master/src/dax.h>

```c
static const char *DAX_MAGIC = "DAX\0";
static const uint32_t DAX_FRAME_SIZE  = 0x2000;   // 8 KB 固定块
static const uint32_t DAX_FRAME_MASK  = 0x1FFF;
static const uint8_t  DAX_FRAME_SHIFT = 13;

struct DAXHeader {
    char     magic[4];
    uint32_t uncompressed_size;
    uint32_t version;
    uint32_t nc_areas;       // "non-compressed areas" 数量
    uint32_t unused[4];
};

struct DAXNCArea {
    uint32_t start;
    uint32_t count;
};
```

紧随头部的是：`index[frames]`（uint32）、`size[frames]`（uint16）、`DAXNCArea[nc_areas]`（maxcso `src/input.cpp` 的读取顺序确证了这个布局）。

**maxcso README 的官方建议**：
> "**Avoid DAX where CSOs using larger block sizes are supported, since DAX is less efficient.**"

### 2.2.4 头部有没有原始 ISO 的哈希：❌ 三者都没有

逐一核对上面三个 struct 定义：**CSO 只有 magic / header_size / uncompressed_size / block_size / version / index_shift / unused[2]；ZSO 相同；DAX 只有 magic / uncompressed_size / version / nc_areas / unused[4]。没有任何一个字段可以放哈希。** 这是「结构里就是没有」级别的确认。

maxcso 有 `--crc  Log CRC32 checksums` 选项，但那是**边解压边算**的，不是从头部读的。

### 2.2.5 不解压能读到什么

| 信息 | 代价 |
|---|---|
| magic（`CISO` / `ZISO` / `DAX\0`）→ 判定格式与压缩算法 | 4 字节 |
| **原始 ISO 的总大小** | 头部（`uncompressed_size` / `total_bytes`）—— ⭐ **这个字段直接可以拿去比 Redump DAT 的 `<rom size>`，做第一轮候选收窄** |
| 块大小、块数量 | 头部 |
| **ISO 前 N KB（→ PVD、`SYSTEM.CNF`、`PARAM.SFO` 路径、UMD_DATA.BIN）** | **只需解压最前面 `ceil(N / block_size)` 个块**。block_size 通常 2048，读前 64 KB = 解 32 个块 |
| 原始 ISO 的 CRC32 / MD5 / SHA-1 | ❌ **必须完整解压** |

**随机访问性质（README 原文已给出）**：索引表是完整的、单调递增的偏移数组，`index[i]` 与 `index[i+1]` 之差即块长度 → **任意块 O(1) 定位、独立解压。** 因此「读 ISO 内任意位置」的代价与位置无关，只与读取长度有关。

---

## 2.3 PBP

**它是什么**：PSP 的 EBOOT 打包格式，也是 PS1 Classics（PSN 上的初代 PS 游戏）的载体。
**一手来源**：
- DuckStation `src/util/cd_image_pbp.cpp` <https://github.com/stenzek/duckstation/blob/master/src/util/cd_image_pbp.cpp>
- PPSSPP `Core/FileSystems/BlockDevices.cpp`（`NPDRMDemoBlockDevice`）
- PSDevWiki PARAM.SFO <https://www.psdevwiki.com/ps3/PARAM.SFO>

### 2.3.1 头部结构（DuckStation，带 `static_assert` 保证）

```c
#pragma pack(push, 1)
struct PBPHeader
{
  u8 magic[4]; // "\0PBP"
  u32 version;
  union {
    u32 offsets[PBP_HEADER_OFFSET_COUNT];
    struct {
      u32 param_sfo_offset;  // 0x00000028
      u32 icon0_png_offset;
      u32 icon1_png_offset;
      u32 pic0_png_offset;
      u32 pic1_png_offset;
      u32 snd0_at3_offset;
      u32 data_psp_offset;
      u32 data_psar_offset;
    };
  };
};
static_assert(sizeof(PBPHeader) == 0x28);
```

即：`+0x00` magic、`+0x04` version、`+0x08` param_sfo_offset、`+0x0C` icon0、`+0x10` icon1、`+0x14` pic0、`+0x18` pic1、`+0x1C` snd0、`+0x20` data_psp、`+0x24` data_psar。

### 2.3.2 ⭐ `PARAM.SFO` 就在文件开头，且是未压缩明文

**这是本文对 PSP/PS1 识别最有价值的一条。** DuckStation 的 `LoadSFOHeader()` 原样如下：

```c
bool CDImagePBP::LoadSFOHeader(Error* error)
{
  if (!FileSystem::FSeek64(m_file, m_pbp_header.param_sfo_offset, SEEK_SET, error))
    return false;
  if (std::fread(&m_sfo_header, sizeof(SFOHeader), 1, m_file) != 1) { ... }
  if (std::memcmp(m_sfo_header.magic, "\0PSF", 4) != 0) { ... }
  ...
}
```

**纯 `fseek` + `fread`，没有任何解压调用。** `param_sfo_offset` 通常是 `0x28`（紧接头部）。

**SFO 自身结构（DuckStation，带 `static_assert`）**：

```c
struct SFOHeader {
  u8  magic[4];          // "\0PSF"
  u32 version;
  u32 key_table_offset;  // Relative to start of SFOHeader, 0x000000A4 expected
  u32 data_table_offset; // Relative to start of SFOHeader, 0x00000100 expected
  u32 num_table_entries; // 0x00000009
};
static_assert(sizeof(SFOHeader) == 0x14);

struct SFOIndexTableEntry {
  u16 key_offset;       // Relative to key_table_offset
  u16 data_type;
  u32 data_size;        // Size of actual data in bytes
  u32 data_total_size;  // Size of data field in bytes, data_total_size >= data_size
  u32 data_offset;      // Relative to data_table_offset
};
static_assert(sizeof(SFOIndexTableEntry) == 0x10);
```

**结论**：**`DISC_ID`（如 `SLPS01234`）、`TITLE`、`CATEGORY`、`DISC_NUMBER` 等全部可以零解压读到**（读量 < 1 KB）。这直接给出了发行版序列号 → 可查 Redump / No-Intro 的 serial 索引。

### 2.3.3 `DATA.PSAR`（真正的光盘数据）是压缩的

DuckStation `OpenDisc()` 的读取顺序（相对每张盘的起点 `iso_header_start`）：

| 相对偏移 | 内容 |
|---|---|
| `+0x000` | `"PSISOIMG0000"`（12 字节 magic）。**多盘时 PSAR 起点是 `"PSTITLEIMG000000"`（16 字节）** |
| `+0x400` | 若为 `"\0PGD"` → 已加密，不支持（DuckStation 原文："Encrypted PBP images are not supported"） |
| `+0x800` | TOC（BCD 时间码条目） |
| `+0xBFC` | `u32 iso_offset` —— 压缩 ISO 数据区的相对偏移 |
| `+0x4000` | **块表**，`BLOCK_TABLE_NUM_ENTRIES = 32256` 条 |

块表条目（`static_assert(sizeof(BlockTableEntry) == 0x20)`）：
```c
struct BlockTableEntry {
  u32 offset;
  u16 size;
  u16 marker;
  u8  checksum[0x10];
  u64 padding;
};
```

数据块用 **raw deflate** 解压（`inflateInit2(&m_inflate_stream, -MAX_WBITS)`）。

> **结论**：PSAR 内的 ISO 数据是**分块压缩 + 完整块表**，因此和 CSO 一样支持 **O(1) 块级随机访问** —— 读 ISO 前 N KB 只需解压最前面几个块。
> ⚠ `BlockTableEntry` 里有个 `u8 checksum[0x10]`（16 字节）。**【未找到权威来源】** 说明它是什么算法、覆盖压缩前还是压缩后的数据；DuckStation 源码里没有使用它。**不要假定它是 MD5。**

### 2.3.4 头部有没有原始镜像的哈希：❌

`PBPHeader` 里只有 magic、version 和 8 个 section 偏移。**没有任何哈希字段。** PSAR 内的 `PSISOIMG` 头部（`+0x000`~`+0x4000`）里有 TOC 和 `iso_offset`，DuckStation 未读取任何哈希字段。

---

## 2.4 WIA / RVZ（含 GCZ）

**它是什么**：GameCube / Wii 压缩镜像。
- **WIA** 由 **wit（Wiimms ISO Tools）** 引入
- **RVZ** 是 Dolphin 基于 WIA 改进的格式，**Dolphin 官方推荐格式**
- **GCZ** 是 Dolphin 的旧压缩格式

**规范（一手，且是难得的完整格式文档）**：Dolphin 仓库内的 `docs/WiaAndRvz.md`
<https://github.com/dolphin-emu/dolphin/blob/master/docs/WiaAndRvz.md>
**实现**：`Source/Core/DiscIO/WIABlob.h` / `WIABlob.cpp`

### 2.4.1 头部结构与三个 SHA-1 的准确语义

**magic（`WIABlob.h`）**：
```c
constexpr u32 WIA_MAGIC = 0x01414957;  // "WIA\x1" (byteswapped to little endian)
constexpr u32 RVZ_MAGIC = 0x015A5652;  // "RVZ\x1" (byteswapped to little endian)
```

**`wia_file_head_t` / `WIAHeader1`（偏移 `0x0`，长度 `0x48`，文档原文说 "its format will never be changed"）**：

| 偏移 | 类型与名字 | `WiaAndRvz.md` 的描述原文 |
|---|---|---|
| `0x00` | `char magic[4]` | Always contains `"WIA\x1"` |
| `0x04` | `u32 version` | |
| `0x08` | `u32 version_compatible` | |
| `0x0C` | `u32 disc_size` / `header_2_size` | The size of the `wia_disc_t` struct |
| `0x10` | `sha1_hash_t disc_hash` / `header_2_hash` | **"The SHA-1 hash of the `wia_disc_t` struct"** |
| `0x24` | `u64 iso_file_size` | **"The original size of the disc (…the size of the ISO file that has the same contents as this WIA file)"** |
| `0x2C` | `u64 wia_file_size` | The size of this file |
| `0x34` | `sha1_hash_t file_head_hash` / `header_1_hash` | **"The SHA-1 hash of this struct, up to but not including `file_head_hash` itself"** |

> 偏移由 Dolphin 的 `static_assert(sizeof(WIAHeader1) == 0x48)` 与字段顺序推得。

**⭐ 关键结论：`disc_hash` 和 `file_head_hash` 都是「结构体自身的 SHA-1」，不是原始 ISO 内容的 SHA-1。** 头部里唯一与原始镜像有关的量是 `iso_file_size`（大小，不是哈希）。**WIA/RVZ 头部没有存原始 ISO 的哈希。**

**`wia_disc_t` / `WIAHeader2`（偏移 `0x48`，长度 `0xdc`）**：

```c
struct WIAHeader2 {
  u32 disc_type;            // 0=unknown, 1=GameCube, 2=Wii     → 0x48
  u32 compression_type;     // 0 NONE,1 PURGE,2 BZIP2,3 LZMA,4 LZMA2, (RVZ) 5 Zstandard  → 0x4C
  s32 compression_level;    // Informative only                 → 0x50
  u32 chunk_size;                                               // → 0x54
  std::array<u8, 0x80> disc_header;   // ★ "The first 0x80 bytes of the disc image"  → 0x58
  u32 number_of_partition_entries;
  u32 partition_entry_size;
  u64 partition_entries_offset;
  Common::SHA1::Digest partition_entries_hash;  // "The SHA-1 hash of the wia_part_t structs"
  u32 number_of_raw_data_entries;
  u64 raw_data_entries_offset;
  u32 raw_data_entries_size;
  u32 number_of_group_entries;
  u64 group_entries_offset;
  u32 group_entries_size;
  u8  compressor_data_size;
  u8  compressor_data[7];
};
static_assert(sizeof(WIAHeader2) == 0xdc, "Wrong size for WIA header 2");
```

### 2.4.2 ⭐ 零解压可得：光盘头 0x80 字节就在文件偏移 `0x58`

`WIAHeader1` 长 `0x48`，`WIAHeader2` 的前 4 个字段各 4 字节 = `0x10`，因此 **`disc_header[0x80]` 位于文件偏移 `0x48 + 0x10 = 0x58`，且是未压缩原样存放**（文档原文："The first 0x80 bytes of the disc image"）。

**这 0x80 字节就是 GameCube/Wii 的光盘头**，包含 Game ID（偏移 0，6 字节）、maker code、disc number、version、magic word、game title（偏移 `0x20`）。

> **结论：WIA / RVZ 的 Game ID、标题、版本号可以只读 216 字节零解压拿到。** 这是 GC/Wii 识别的最优路径。

### 2.4.3 随机访问与压缩

- 文档原文："Like essentially all compressed GC/Wii disc image formats, **WIA divides the data into blocks (called chunks in wit). Each chunk is compressed separately, making random access of compressed data possible.**"
- `wia_group_t`（8 字节）/ `rvz_group_t`（12 字节）给出每个 chunk 的 `data_off4`（偏移÷4）与 `data_size`。
- ⚠ **group 表本身是压缩存放的**（`group_entries_size` = "The total compressed size of the group entries"）→ **随机访问前要先解一次 group 表**（与 CHD map 同理）。
- RVZ 额外特性：Zstandard 压缩；chunk 可小于 2 MiB（最小 32 KiB）；`data_size` 的最高位表示该块是否压缩；**RVZ packing**（用 Lagged Fibonacci PRNG `f=xor, j=32, k=521` 无损还原光盘的伪随机填充数据）。

### 2.4.4 RVZ 能不能对上 Redump / No-Intro？

**能，但必须完整解压重建 ISO。** Dolphin 自己就是这么做的 —— `Source/Core/DiscIO/VolumeVerifier.cpp` 里的 `RedumpVerifier`：

```c
request.Get("https://redump.info/datfile/" + system + "/serial,version", ...)
...
potential_match.hashes.crc32 = ParseHash(rom.attribute("crc").value());
...
if (HashesMatch(hashes.crc32, p.hashes.crc32) && HashesMatch(hashes.md5, p.hashes.md5) && ...)
```

`VolumeVerifier` 构造时 `Hashes<bool> hashes_to_calculate{.crc32 = true, .md5 = true, .sha1 = true}` —— 它是**在读取（解压）整个镜像的同时**滚动计算 CRC32/MD5/SHA-1，再与 Redump datfile 比对。

> **结论**：RVZ 是**无损**格式（能字节精确还原原始 ISO），所以理论上可以对上 Redump；但**头部没有缓存这个哈希**，每次校验都要走一遍完整解压。**【推断】** 因此本项目应当在首次校验后把结果落进沉淀库，以 RVZ 文件的 `(路径, size, mtime)` 或 `header_1_hash` 为键，永不重复。

### 2.4.5 GCZ

Dolphin `Source/Core/DiscIO/CompressedBlob.h` 定义了 `GCZ_MAGIC`，`Blob.cpp` 的 `CreateBlobReader()` 按 magic 分派到 `CompressedBlobReader`。**GCZ 是 Dolphin 的旧格式，NKit README 明确说 "RVZ replaced it as the recommended compressed format"。** 头部是否含原始哈希 —— 本次未展开核实，标 **【未找到权威来源】**（Dolphin 未提供 GCZ 的格式文档）。

---

## 2.5 WBFS

**它是什么**：Wii 备份文件系统，最初是给 USB Loader 用的「一个分区里放多个 Wii 游戏」的容器，后来退化成单游戏文件格式（`.wbfs`）。
**一手来源**：Dolphin `Source/Core/DiscIO/WbfsBlob.h` / `WbfsBlob.cpp`
<https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/WbfsBlob.cpp>

### 2.5.1 头部结构（Dolphin，`#pragma pack(1)`）

```c
static constexpr u32 WBFS_MAGIC = 0x53464257;  // "WBFS"

struct WbfsHeader
{
  u32 magic;              // +0x00  "WBFS"
  u32 hd_sector_count;    // +0x04  (big endian)
  u8  hd_sector_shift;    // +0x08  → hd_sector_size = 1 << shift
  u8  wbfs_sector_shift;  // +0x09  → wbfs_sector_size = 1 << shift
  u8  padding[2];         // +0x0A
  u8  disc_table[500];    // +0x0C
};
```

紧接着，在偏移 `hd_sector_size`（通常 512 = `0x200`）处：

```c
static constexpr u64 WII_DISC_HEADER_SIZE = 256;
...
m_files[0].file.Seek(m_hd_sector_size + WII_DISC_HEADER_SIZE, File::SeekOrigin::Begin);
m_files[0].file.Read(Common::AsWritableU8Span(m_wlba_table));
```

即：`[hd_sector_size, hd_sector_size + 256)` 是 **Wii 光盘头的 256 字节（未压缩原样）**，其后是 `wlba_table`（每个 wbfs 扇区映射到磁盘上哪个块，u16 big-endian）。

### 2.5.2 有没有哈希：❌ 没有

`WbfsHeader` 只有 magic / 扇区计数 / 两个 shift / padding / disc_table。**没有任何哈希字段。**

### 2.5.3 ⭐ WBFS 是有损的

`WbfsBlob.h`：
```c
u64  GetDataSize() const override;
DataSizeType GetDataSizeType() const override { return DataSizeType::UpperBound; }   // ★
```

`SeekToCluster()` 的逻辑是 `cluster_address = m_wbfs_sector_size * m_wlba_table[base_cluster]` —— **被丢弃的（未使用的）簇在 wlba 表里记为 0，读到它会落到文件开头（头部所在处），而不是原始光盘的填充数据。** Dolphin 的 `Read()` 没有任何「重新生成 junk data」的逻辑。

> **【推断】**（Dolphin 未直接声明，但由 `DataSizeType::UpperBound` 与 `SeekToCluster` 的实现可推出）：**WBFS 丢弃了光盘上未被文件系统使用的区域，因此无法字节精确还原原始 ISO，也就无法对上 Redump/No-Intro 的哈希。** 这与 RVZ（无损）形成鲜明对比。

### 2.5.4 零解压可得

| 信息 | 代价 |
|---|---|
| magic、hd/wbfs 扇区大小、disc_table | 前 **512 字节**（`4+4+1+1+2+500 = 0x200`，即 `sizeof(WbfsHeader)`） |
| **Wii 光盘头 256 字节（Game ID / 标题 / 版本）** | 偏移 `hd_sector_size`（通常 `0x200`）起 256 字节，**未压缩** |
| 原始 ISO 的哈希 | ❌ 头部没有；**且即使完整读出也对不上 Redump（有损）** |

### 2.5.5 多文件 WBFS

`OpenAdditionalFiles()` 会把 `xxx.wbfs` 之后的 `xxx.wbf1` … `xxx.wbf9`（最多 10 个）串起来（源码注释：`// Replace last character with index (e.g. wbfs = wbf1)`）。**成型规则要把 `.wbfs` + `.wbf1..9` 视为一个变体，主文件是 `.wbfs`。**

---

## 2.6 NKit

**它是什么**：GameCube / Wii（NKit 2 起扩展到 14 个系统）的镜像处理与去重工具，作者 Nanook。
**官方仓库**：<https://github.com/Nanook/NKit>

### 2.6.1 现状与许可

NKit 2 README 原文：
> "**The source is currently in a private repository** while a final refactor is completed. It will be made public soon."
> "**License will be announced with the source code release.**"

> ⚠ **NKit 2 目前是闭源、许可未定。** 不能作为本项目的库依赖，只能作为外部工具调用（且许可未明）。

**格式支持（README 原文）**：
> **Reads:** ISO, RVZ, WIA, WBFS, CISO, WUD, WUX, GCZ, CHD, CSO, ZSO, DAX, JSO, CUE+BIN, GDI, **NKit (legacy)**, CDN/APP, XISO
> **Writes:** ISO, RVZ (Zstandard/LZMA), WBFS, CISO, WUX, CSO, ZSO, APP, CUE, GDI

注意 **写入列表里没有 nkit** —— README FAQ 原文："NKit 2 reads the legacy nkit.iso/nkit.gcz format but **no longer writes it**. **RVZ replaced it as the recommended compressed format.**"

**它的归档支持很值得参考（README 原文，"Archive Support (Forward-Reading)"）**：
> "Process images directly from archives — no temp files, no extraction to disk:
> - **RAR** (1–5) — split, multi-volume, **solid**
> - **ZIP / ZipX** — split, multi-volume, Zstandard/XZ compression
> - **7-Zip** — split, **solid**, Zstandard
> - **GZip** — split"

> **【推断】** "Forward-Reading" 正是本文 1.6.4 推荐的策略：顺序单趟流式读取，天然规避 solid 的重复解压。

### 2.6.2 `.nkit.iso` 的头部结构

**【未找到权威来源】** —— NKit 的格式没有公开规范文档，源码闭源。

**但检测方法有权威来源**：Dolphin `Source/Core/DiscIO/VolumeDisc.cpp`

```c
bool VolumeDisc::IsNKit() const
{
  constexpr u32 NKIT_MAGIC = 0x4E4B4954;  // "NKIT"
  return ReadSwapped<u32>(0x200, PARTITION_NONE) == NKIT_MAGIC;
}
```

**即：光盘偏移 `0x200` 处的 4 字节为 `"NKIT"`。** 注意这是**光盘逻辑偏移**，所以 `.nkit.iso` / `.nkit.gcz` / `.nkit.rvz` 都能用同一个判据（先按各自容器打开，再读逻辑偏移 `0x200`）。

### 2.6.3 ⭐⭐ NKit 是识别管线的头号误报源

Dolphin `Source/Core/DiscIO/VolumeVerifier.cpp` 的原文警告：

```c
if (m_volume.IsNKit())
{
  AddProblem(Severity::Low,
      Common::GetStringT("This disc image is in the NKit format. It is not a good dump in its "
                         "current form, but it might become a good dump if converted back. "
                         "The CRC32 of this file might match the CRC32 of a good dump even "
                         "though the files are not identical."));
}
```

**「这个文件的 CRC32 可能和好 dump 的 CRC32 相同，即使两个文件并不完全一样。」**

> **对本项目的直接影响**：
> - 如果第一层命中用的是 **CRC-32**（正如本文 1.4 推荐的），**NKit 处理过的 GC/Wii 镜像会「假命中」原版 DAT 记录**。
> - **必须在 CRC 匹配之前插一个 NKit 检测**：读光盘逻辑偏移 `0x200` 的 4 字节，若为 `"NKIT"`，则把该变体标记为「NKit 处理过、非原始 dump」，命中降级为「需人工裁决」或「需先转回 ISO 再重算」。
> - Dolphin 说它 "might become a good dump if converted back"，所以正确处理是用 NKit 工具转回 ISO 后再识别。

---

## 2.7 Rust 生态（压缩镜像）

| 格式 | crate | 版本 / 下载量 / 更新 | 许可 | 能力 |
|---|---|---|---|---|
| **CHD** | **`chd`**（即 chd-rs） <https://github.com/SnowflakePowered/chd-rs> | 0.3.4 / 6.4 万 / 2026-03 | **BSD-3-Clause** | ⭐ **纯 Rust**，README 原文："Pure Rust implementation of the Compressed Hunks of Data format, **drop-in compatible with libchdr**"。API：`Chd::open()` → `chd.header()`（零解压拿全部头部字段含 `rawsha1`/`sha1`）→ `chd.hunk(n)` **hunk 级随机访问**。支持 V1–V5（README 注明 "V1-4 support has not been as rigorously tested as V5"）。V5 编解码器：None / LZMA / Deflate / FLAC / Huffman / Zstandard / CD LZMA / CD Deflate / CD FLAC / CD Zstandard / AV Huffman。另提供 `chd::read` 的 `Read + Seek` 适配器 |
| CSO / ZSO | — | | | **【未找到】** crates.io 上没有找到成熟的 CSO/ZSO 读取库 |
| DAX | — | | | **【未找到】** |
| PBP | — | | | **【未找到】** |
| PARAM.SFO | — | | | **【未找到】**（`psf` crate 是 PC Screen Font，与 PS 的 PARAM.SFO 无关） |
| RVZ / WIA / WBFS / GCZ | — | | | **【未找到】** |

**【推断】** 好消息是，**上面这些「没有 crate」的格式，恰恰是本文证明「只读头部就够用」的那些**：

| 格式 | 自实现「零解压识别」的工作量 |
|---|---|
| CSO / ZSO / DAX | 极小：读一个 24/32 字节的定长头（本文 2.2 已给出完整 struct）+ 索引表；块解压用 `flate2` / `lz4_flex` |
| PBP | 极小：读 `0x28` 字节头 + `fseek` 到 `param_sfo_offset` 解析 SFO（本文 2.3 已给出完整 struct）。**PARAM.SFO 解析器约 100 行** |
| WIA / RVZ | 极小：读 `0x58 + 0x80 = 216` 字节即可拿 Game ID |
| WBFS | 极小：读 `0x200 + 256` 字节即可拿 Game ID |
| CHD | 用 `chd` crate |

**需要完整解压时的替代方案**（这些才需要重型依赖）：

| 需求 | 方案 | 许可 |
|---|---|---|
| CHD ↔ CD/DVD 镜像互转 | 外部调用 `chdman`（MAME 工具） | MAME 是 **GPL-2.0-or-later**（外部进程调用无传染） |
| CSO/ZSO/DAX ↔ ISO | 外部调用 `maxcso` | maxcso 自身 **ISC**（README 原文），但链接了 LGPL 的 7-zip/p7zip |
| RVZ/WIA/GCZ/WBFS ↔ ISO | 外部调用 **DolphinTool**（Dolphin 自带 CLI，`dolphin-tool convert` / `verify`） | Dolphin 是 GPL-2.0-or-later |
| 多格式互转 + Redump 校验 | 外部调用 **NKit 2** | ⚠ **闭源、许可未定** |

---

# 第 3 部分 · 模拟器与前端的容器支持矩阵

## 3.1 RetroArch（含 libretro 核心）

### 3.1.1 ⭐ 支持哪些归档：源码级确认

**唯一的分派函数** —— `libretro-common/file/archive_file.c`：

```c
const struct file_archive_file_backend* file_archive_get_file_backend(const char *path)
{
#if defined(HAVE_7ZIP) || defined(HAVE_ZLIB) || defined(HAVE_ZSTD) || defined(HAVE_RZSTD) || defined(HAVE_COMPRESSION)
   ...
   file_ext = path_get_extension(newpath);

#ifdef HAVE_7ZIP
   if (string_is_equal_noncase(file_ext, "7z"))
      return &sevenzip_backend;
#endif

#if defined(HAVE_ZLIB) || defined(HAVE_COMPRESSION)
   /* ZIP/APK decode via zlib, or via the built-in inflate when zlib is not compiled in. */
   if (     string_is_equal_noncase(file_ext, "zip")
         || string_is_equal_noncase(file_ext, "apk"))
      return &zlib_backend;
#endif

#if defined(HAVE_ZSTD) || defined(HAVE_RZSTD)
   if (string_is_equal_noncase(file_ext, "zst"))
      return &zstd_backend;
#endif
#endif
   return NULL;
}
```

**并且仓库里根本不存在 `archive_file_rar.c`**（实测 `https://raw.githubusercontent.com/libretro/RetroArch/master/libretro-common/file/archive_file_rar.c` 返回 **HTTP 404**；存在的只有 `archive_file_zlib.c` 与 `archive_file_7z.c`）。

> **✅ 你的理解正确：RetroArch 支持 zip 与 7z，不支持 rar。** 额外还支持 `apk`（走 zlib，因为 APK 就是 ZIP）与 `zst`（单文件 zstd）。
> **✅ 归档支持是全局的**（由 `HAVE_ZLIB` / `HAVE_7ZIP` / `HAVE_ZSTD` 编译开关控制），不是按核心而定 —— 但**加载行为按核心而定**，见下。

### 3.1.2 ⭐ 加载流程：`need_fullpath` 与 `block_extract` 决定一切

`tasks/task_content.c`：

```c
/* If core does not require 'fullpath', load the content into memory */
if (!((content->elems[i].attr.i & BLCK_NEED_FULLPATH) != 0))
{
   content_size = content_file_load_into_memory(content_ctx, p_content, content_path,
                                                content_compressed, i, first_content_type, &content_data);
}
else
{
   /* If this is compressed content and need_fullpath is true, extract it to a temporary file */
   if (content_compressed
       && !((content->elems[i].attr.i & BLCK_BLOCK_EXTRACT) != 0)
       && !content_file_extract_from_archive(content_ctx, p_content, valid_exts, &content_path, err_string))
      return false;
}
```

`content_file_extract_from_archive()` 的日志与注释：
```c
RARCH_LOG("[Content] Core requires uncompressed content - extracting archive to temporary directory...\n");
...
/* The cache directory is the parent when one is configured;
 * otherwise the archive's own directory is ... */
...
/* Add path of extracted file to temporary content list (so it can be deleted when deinitialising the core) */
```

**三种行为**：

| 核心声明 | RetroArch 的行为 |
|---|---|
| `need_fullpath = false`（多数卡带类核心） | **直接在内存里解压**归档内的目标文件，把内存指针喂给核心。不落盘 |
| `need_fullpath = true` + `block_extract = false`（多数光盘类核心） | **解压到临时目录**（配置了 cache 目录就放那里，否则放归档所在目录），核心退出时删除 |
| `block_extract = true`（MAME / FBNeo 等要自己吃 zip 的核心） | **不解压**，把归档路径本身交给核心 |

路径语法是 `<归档路径>#<内部文件>`（`path_get_archive_delim()`），当归档内有多个匹配 `valid_exts` 的文件时，RetroArch 会列出内部文件列表让用户选。

### 3.1.3 ⭐ RetroArch 扫描器：一个可以直接抄的两段式策略

`tasks/task_database.c` 的 `task_database_iterate_playlist()`：

```c
switch (extension_to_file_type(path_get_extension(name)))
{
  case FILE_TYPE_COMPRESSED:            // zip / 7z / apk / zst
     db->type = DATABASE_TYPE_CRC_LOOKUP;
     /* first check crc of archive itself */
     return intfstream_file_get_crc_and_size(name, 0, INT64_MAX,
                                             &db_state->archive_crc, &db_state->archive_size);
  case FILE_TYPE_CUE:  ... serial 优先，失败回退 CRC
  case FILE_TYPE_GDI:  ... serial 优先，失败回退 CRC
  case FILE_TYPE_WBFS:
  case FILE_TYPE_RVZ:
  case FILE_TYPE_WIA:                   // "Consider WBFS, RVZ and WIA files similar to ISO files."
     intfstream_file_get_serial(...);   // ★ 只查 serial，不算哈希
     db->type = DATABASE_TYPE_SERIAL_LOOKUP;
     break;
  case FILE_TYPE_ISO:
     intfstream_file_get_serial(...);
     db->type = DATABASE_TYPE_SERIAL_LOOKUP_SIZEHINT;
     break;
  case FILE_TYPE_CHD:                   // ★ serial 优先
     if (task_database_chd_get_serial(...)) db->type = DATABASE_TYPE_SERIAL_LOOKUP;
     else { db->type = DATABASE_TYPE_CRC_LOOKUP; return task_database_chd_get_crc_and_size(...); }
  case FILE_TYPE_PBP:                   // ★ serial 优先
     if (task_database_pbp_get_serial(...)) db->type = DATABASE_TYPE_SERIAL_LOOKUP;
     else ...
  default:
     db->type = DATABASE_TYPE_CRC_LOOKUP;
     return intfstream_file_get_crc_and_size(name, 0, INT64_MAX, &db_state->crc, &db_state->size);
}
```

**归档的两段式**（`database_info_list_iterate_end_no_match`）：

```c
/* If this was a compressed file and no match in the database list was found
 * then expand the search list to include the archive's contents. */
if (!path_contains_compressed_file && path_is_compressed_file(path) && _db->task_config->search_archives)
   archive_added = add_files_from_archive(_db, path);
```

`add_files_from_archive()` 用 `file_archive_get_file_list(path, file_exts)` 拿内部文件列表，把 `归档路径#内部文件` 追加进扫描队列。

匹配时两个 CRC 都试（`task_database_iterate_crc_lookup`）：
```c
/* When scanning an archive, "first" file crc32 is also checked. */
if (db_info_entry->crc32)
{
   if (db_state->archive_crc == db_info_entry->crc32) return ...found_match(..., NULL);
   if (db_state->crc         == db_info_entry->crc32) return ...found_match(..., archive_entry);
}
```

> **可直接借鉴的三条**：
> 1. **先试「归档文件自身的 CRC」**（有些 DAT 收录的就是打包后的 zip），未命中再展开内部文件。
> 2. **光盘类格式一律走 serial 而非哈希**（ISO/WBFS/RVZ/WIA/CHD/PBP），因为 serial 零/低成本可得而哈希昂贵。
> 3. **`.cue` 有专门的剪枝**（`task_database_cue_prune`）—— 命中 `.cue` 后把它引用的 `.bin` 从扫描列表里剔除，避免同一变体被计两次。

### 3.1.4 CHD 是谁支持的？

RetroArch **自身内置了 libchdr**：`libretro-common/include/libchdr/chd.h` 与 `libretro-common/streams/chd_stream.c` 均存在（实测 HTTP 200）。扫描器用它读 CHD 的 serial。

**运行时播放**则由核心负责 —— Beetle PSX / SwanStation / Flycast 等各自链接 libchdr。**【推断】** 因此「RetroArch 能不能玩 CHD」取决于所用核心，而不是前端。

---

## 3.2 独立模拟器

> 下表每一行都有源码或官方文档支撑。**没有一手来源的一律标注。**

### 3.2.1 卡带类

| 模拟器 | 归档支持 | 一手来源 |
|---|---|---|
| **Mesen2** | **zip + 7z**。`ArchiveReader::GetReader()` 按前 2 字节 magic 分派：`"PK"` → `ZipReader`，`"7z"` → `SZReader`。**无 rar** | [`Utilities/ArchiveReader.cpp`](https://github.com/SourMesen/Mesen2/blob/master/Utilities/ArchiveReader.cpp) |
| **Snes9x** | **zip（`UNZIP_SUPPORT`，内置 minizip）+ JMA（`JMA_SUPPORT`）+ `.msu1`**。`memmap.cpp` 里 `path.ext_is(".zip")` → `LoadZip()`，`.jma` → `load_jma_file()`。**仓库内没有任何 rar 或 7z 归档代码**（源码树里只有 `unzip/`（minizip）与 `jma/`；`jma/7z.h`、`jma/7zlzma.cpp` 是 JMA 用的 LZMA 解码器，不是 7z 归档支持） | [`memmap.cpp`](https://github.com/snes9xgit/snes9x/blob/master/memmap.cpp)、仓库文件树 |
| **ares**（含 bsnes 后继） | **仅 zip**。`desktop-ui/emulator/emulator.cpp` 里 `string filters{"*.zip:"}`；`nall/nall/decode/zip.hpp` 是唯一的归档解码器 | [ares 仓库](https://github.com/ares-emulator/ares) |
| **mGBA** | **zip + 7z**。README Features 原文：「**Loading from ZIP and 7z files.**」依赖 libzip/zlib（"libzip or zlib: for loading ROMs stored in zip files"）与 LZMA SDK | [mGBA README](https://github.com/mgba-emu/mgba/blob/master/README.md) |
| **DeSmuME**（Windows 前端） | ⭐ **zip + 7z + rar**。内置 blargg 的 **File_Extractor**（`fex/Zip_Extractor`、`Zip7_Extractor`、`Gzip_Extractor`、**`Rar_Extractor`** + 完整的 `unrar/` 目录）。`fex.cpp` 里的扩展名表含 `".7z"`、`".rar"`；`main.cpp` 的打开对话框过滤器含 `*.zip`、`*.7z`、`*.rar` | [DeSmuME 源码树](https://github.com/TASEmulators/desmume) |
| **melonDS** | ⭐ **zip + 7z + rar + tar 全家桶**。走 **libarchive**（`CMakeLists.txt`：`pkg_check_modules(LibArchive REQUIRED IMPORTED_TARGET libarchive)` + `add_compile_definitions(ARCHIVE_SUPPORT_ENABLED)`）。`Window.cpp` 的扩展名白名单：`".zip", ".7z", ".rar", ".tar", ".tar.gz", ".tar.xz", ".tar.bz2", ".tar.lz4", ".tar.zst", …`，MIME 里含 `application/vnd.rar`。还支持 `a.zip|b.nds` 的成员选择语法 | [melonDS 源码](https://github.com/melonDS-emu/melonDS) |

> libarchive 的 RAR 支持有官方限制说明 —— README 原文："**RAR and RAR 5.0 archives (with some limitations due to RAR's proprietary status)**"（<https://github.com/libarchive/libarchive>）。**【推断】** 因此 melonDS 对 solid RAR / RAR5 高级特性的支持可能不完整。

### 3.2.2 光盘类

| 模拟器 | 支持的镜像格式 | 归档（zip/7z/rar） | 一手来源 |
|---|---|---|---|
| **DuckStation**（PS1） | `.cue` / `.bin` / `.img` / `.iso` / **`.ecm`** / **`.chd`** / `.mds` / **`.pbp`** / `.ccd` / `.m3u` | ❌ **无** | [`src/util/cd_image.cpp`](https://github.com/stenzek/duckstation/blob/master/src/util/cd_image.cpp) 的 `CDImage::Open()` 扩展名分派 |
| **PCSX2**（PS2） | **`.chd`** / **`.cso`** / **`.zso`** / `.gz` / `.dump`(blockdump) / 其余走 `FlatFileReader`（`.iso`/`.bin`） | ❌ **无** | [`pcsx2/CDVD/InputIsoFile.cpp`](https://github.com/PCSX2/pcsx2/blob/master/pcsx2/CDVD/InputIsoFile.cpp) 的 `GetFileReader()`；CSO/ZSO 共用 `CsoFileReader`，magic 首字节 `'Z'` → lz4，`'C'` → zlib |
| **PPSSPP**（PSP） | `.iso` / **`.cso`** / **`.chd`** / **`.pbp`**（EBOOT，走 `NPDRMDemoBlockDevice`）/ 目录形态 | ❌ **无** | [`Core/FileSystems/BlockDevices.cpp`](https://github.com/hrydgard/ppsspp/blob/master/Core/FileSystems/BlockDevices.cpp) 的 `constructBlockDevice()`：只认 `"CISO"` / `"\x00PBP"` / `"MComprHD"`；[`Core/Loaders.cpp`](https://github.com/hrydgard/ppsspp/blob/master/Core/Loaders.cpp)：`extension == ".iso" \|\| ".cso" \|\| ".chd"` |
| **Dolphin**（GC/Wii） | 按 magic 分派：**CISO / GCZ / TGC / WBFS / WIA / RVZ / NFS**；否则尝试 DirectoryBlob（解包目录）→ SplitPlainFileReader（分卷 ISO）→ PlainFileReader（`.iso`/`.gcm`） | ❌ **无** | [`Source/Core/DiscIO/Blob.cpp`](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/Blob.cpp) 的 `CreateBlobReader()`；`BlobType` 枚举：`PLAIN / DIRECTORY / GCZ / CISO / WBFS / TGC / WIA / RVZ / MOD_DESCRIPTOR / NFS / SPLIT_PLAIN` |
| **Flycast**（DC） | **chd / gdi / cdi / cue**（+ 编译了 `USE_LIBCDIO` 时的物理光驱） | ❌ **无** | [`core/imgread/common.cpp`](https://github.com/flyinghead/flycast/blob/master/core/imgread/common.cpp) 的 `drivers[] = {chd_parse, gdi_parse, cdi_parse, cue_parse, cdio_parse}` |
| **xemu**（Xbox） | **仅 xiso**。官方文档原文："xemu requires game discs to be in the form of **xiso** images. These are generally saved with a `.iso` extension, but are **not the same as typical ISO images**"；且**与 Redump ISO 不兼容**，需先抽出第二个分区 | <https://xemu.app/docs/disc-images/> |
| **RPCS3**（PS3） | **【未找到权威来源】**（rpcs3.net 对自动抓取返回 403）。**【推断】** 常规形态是解包后的 `PS3_GAME` 目录或 `.pkg` 安装；未见任何归档或压缩镜像支持 | — |
| **Xenia**（X360） / **Cemu**（Wii U） | **【未找到权威来源】**（本次未取得可引用的官方页面） | — |

**关于 Dolphin 与 NKit**：Dolphin 的 `BlobType` 里**没有 NKit 项** —— NKit 处理过的镜像是以 `.nkit.iso` / `.nkit.gcz` / `.nkit.rvz` 形态存在的，Dolphin 按对应的容器读它（PlainFileReader / GCZ / RVZ），只是内容不是原始 dump，于是弹出 2.6.3 引用的那条警告。

### 3.2.3 ⭐ RAR 支持情况汇总

| 支持 rar | 不支持 rar |
|---|---|
| **melonDS**（libarchive） | RetroArch（源码级确认）、Mesen2、Snes9x、ares、mGBA、Dolphin、PCSX2、DuckStation、PPSSPP、Flycast、xemu、ES-DE |
| **DeSmuME**（Windows 前端，内置 unrar） | |

> **结论：rar 在模拟器生态里是「几乎无人支持」。** 库里的 `.rar` 必须转成 `.zip` 或 `.7z` 才能可靠游玩。这不是「建议」，而是「否则大部分平台无法启动」。

---

## 3.3 前端

### 3.3.1 ES-DE

**来源**：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>（437 KB 全文实测检索）

- **归档格式**：系统扩展名列表里出现的归档只有 `.7z .7Z .zip .ZIP`（例：`<extension>.nes .NES .unf .UNF .unif .UNIF .7z .7Z .zip .ZIP</extension>`）。
- **`.rar` 在整份 USERGUIDE 里出现 0 次**（实测 `grep -c -i "\.rar"` = 0）。**ES-DE 不支持 rar。**
- **扫描模型**：归档就是一个普通的「可启动文件类型」，按扩展名匹配即成为一个游戏条目 —— **不看内部**。
- **启动**：把归档路径原样传给模拟器（对 MAME 类系统 USERGUIDE 明确写 "single file archives should be used" / "Single archive or ROM file"）。
- ⭐ **刮削哈希的重要说明（USERGUIDE 原文）**：
  > "if the _Search using file hashes_ scraper option is enabled and you're using ScreenScraper, then a hash value will be calculated from the actual game file… **When using hash searches it's often a good idea to uncompress zip archives for systems with single game files (for instance a .bin file inside a .zip archive) as it's more likely that there's a match for such a file than for a compressed archive.**"

  即 **ES-DE 对归档算的是「归档文件本身」的哈希，不穿透**。这与 RetroArch 的两段式不同，也是本项目要做得比 ES-DE 好的地方。
- **「目录当作文件」功能**：ES-DE 支持把一个目录（哪怕名字带 `.zip`）当成单个游戏条目展示，用于 ROM hack 场景（USERGUIDE 的 "RetroArch ROM hacks" 一节）。

### 3.3.2 Pegasus

**来源**：<https://pegasus-frontend.org/docs/user-guide/meta-files/>、<https://github.com/mmatyas/pegasus-frontend>

- 收集游戏的三种方式：`extensions`（"All files with these extensions (including those in subdirectories) will be included."）、`files`（显式列出）、`regex`。
- 官方示例的 `extensions` 里就包含归档：`extensions: 7z, bin, smc, sfc, fig, swc, mgd, zip, bin`。
- 启动命令用 `{file.path}` / `{file.name}` 变量，文档原文："replaced as-is, without additional formatting"。
- **文档没有任何「Pegasus 会解压或读取归档内部」的说明** → **归档是一个不透明的可启动文件**，解压与否完全交给模拟器。
- **【未找到权威来源】** Pegasus 是否对 `.rar` 有特殊处理 —— 文档未提及；**【推断】** 由于它只做扩展名匹配、不看内部，写进 `extensions` 的任何后缀都会被收集，能不能玩取决于模拟器。

### 3.3.3 LaunchBox / Playnite

**【未找到权威来源】** —— 本次未取得可引用的官方文档。

---

# 第 4 部分 · 两张总表

## 表 1 · 零解压 / 部分解压 / 必须完整解压

| 容器 / 镜像格式 | 零解压可得信息 | 部分解压可得信息 | 必须完整解压才可得 | 主要风险 |
|---|---|---|---|---|
| **ZIP** | 内部文件名、未压缩大小、压缩后大小、压缩方法、**CRC-32**、条目数据起点偏移 | 内部文件前 N 字节（stored 条目可直接 seek，O(1)；deflate 流式解到 N） | 内部文件的 **MD5 / SHA-1** | ① 必须读 central directory：flag bit 3 时 local header 的 CRC/大小全为 0 ② ZIP64（>4 GB）哨兵值 `0xFFFFFFFF` 必须解析 extra field ③ `.z01…​.zip` split 的入口是最后一段 |
| **7z** | 内部文件名、未压缩大小、**CRC-32**（`kCRC`）、**每个文件属于哪个 folder**、folder 解压后大小 | 非 solid folder：前 N 字节，代价 ∝ N；**solid folder：代价 ∝「文件在块内偏移 + N」** | 内部文件的 **MD5 / SHA-1** | ⭐ **solid block**：朴素实现可放大 100×（见 1.6.2）② header 本身可能被压缩（`kEncodedHeader`），要多解一次 ③ `kCRC` 规范上可选 ④ 头部可加密（`-mhe=on`）则连文件名都读不到 |
| **RAR4** | 内部文件名、未压缩大小、**CRC-32（无条件存在）**、solid 标志、分卷续接标志 | 非 solid：前 N 字节 ∝ N；**solid：必须解完组内所有前置文件** | 内部文件的 **MD5 / SHA-1** | ⭐ solid ② 列目录要**顺序扫描全文** ③ `-hp` 头部加密时文件名不可见 ④ 分卷 CRC 语义未见规范（**【未找到权威来源】**） |
| **RAR5** | 内部文件名（UTF-8）、未压缩大小、**CRC-32（可选，默认有）**、**BLAKE2sp（可选）**、solid 标志、分卷标志、（可选）Locator 快速定位 | 同 RAR4 | 内部文件的 **MD5 / SHA-1**；若只写了 BLAKE2sp 而无 CRC32，则连 CRC 也要解压才有 | ⭐ solid ② **分卷时非末段的 CRC 是「打包后数据」的 CRC，不能比 DAT** ③ 只有 BLAKE2sp 的归档退化为「必须完整解压」 ④ UnRAR 许可 |
| **`.001` 分卷** | 无（它本身没有头部） | — | — | 只需 `cat` 拼接；识别管线用「多文件拼接读取器」当成一个逻辑文件即可 |
| **CHD** | ⭐ **`rawsha1` / `sha1`（combined） / `parentsha1`**、`logicalbytes`、`hunkbytes`、压缩器；轨道表（读 metadata 链） | 任意扇区：**先解一次 hunk map（一次性），之后每次解 1 个 hunk（≤512 KB）** → `SYSTEM.CNF` / PVD / IP.BIN 等 | 与 Redump 可比的**逐轨 `.bin` 哈希**（`chdman extractcd --splitbin`） | ⭐ **头里的两个 SHA-1 都对不上 Redump**（粒度与扇区布局都不同，见 2.1.2）② 压缩 CHD 要先解 map ③ 差分 CHD（`parentsha1 != 0`）缺父文件就读不了 |
| **CSO / ZSO** | magic（判 deflate vs lz4）、**原始 ISO 总大小**、块大小、完整块索引表 | ⭐ **ISO 任意位置，O(1) 定位**：读前 64 KB 只需解 32 个 2048 块 → `SYSTEM.CNF` / `PARAM.SFO` / `UMD_DATA.BIN` | 原始 ISO 的 **CRC32 / MD5 / SHA-1** | ① 头部**无任何哈希** ② CSO v2 与 ZSO 是**实验性格式**，PPSSPP 明确不支持（见 3.2.2） |
| **DAX** | magic、原始 ISO 大小、version、`nc_areas`、块索引（8 KB 固定块） | 同 CSO | 原始 ISO 的哈希 | ① 头部无哈希 ② maxcso 官方建议 "**Avoid DAX**" ③ 无独立规范文档 |
| **PBP** | ⭐ **`PARAM.SFO` 全部键值（`DISC_ID` / `TITLE` / `CATEGORY` / `DISC_NUMBER`）—— 未压缩明文，`fseek`+`fread` 即得**；8 个 section 偏移；PSAR 的 TOC 与块表 | ISO 任意位置（PSAR 内 raw-deflate 分块 + 32256 条块表，O(1) 定位） | 原始 ISO 的哈希 | ① 头部无镜像哈希 ② `+0x400` 处若为 `"\0PGD"` 说明**已加密，无法读取** ③ 多盘时 PSAR 起点 magic 是 `PSTITLEIMG000000` 而非 `PSISOIMG0000` ④ 块表里的 `checksum[0x10]` 语义未知（**【未找到权威来源】**） |
| **WIA / RVZ** | ⭐ **光盘头前 0x80 字节（Game ID / 标题 / 版本）在文件偏移 `0x58`，未压缩**；`iso_file_size`；`disc_type`；压缩方式 | 任意 chunk（≥32 KiB）—— 但**要先解压 group 表** | 原始 ISO 的 **CRC32 / MD5 / SHA-1**（RVZ 无损，可重建后比 Redump —— Dolphin 的 `RedumpVerifier` 就这么做） | ① 头里的 `disc_hash` / `file_head_hash` 是**结构体自身**的 SHA-1，**不是镜像内容的** ② group 表压缩存放 ③ WIA 的 PURGE 压缩在 RVZ 中被移除 |
| **WBFS** | ⭐ **Wii 光盘头 256 字节（Game ID / 标题）在文件偏移 `hd_sector_size`（通常 `0x200`），未压缩**；扇区参数；`disc_table` | 任意 wbfs 扇区（wlba 表映射，无压缩，纯 O(1) seek） | — | ⭐ **有损**：丢弃未使用簇，**无法还原出与 Redump 一致的 ISO**（`DataSizeType::UpperBound`） ② 多文件形态 `.wbfs + .wbf1..9` 要一起成型 |
| **NKit (`.nkit.iso` / `.nkit.gcz` / `.nkit.rvz`)** | ⭐ **检测判据：光盘逻辑偏移 `0x200` 处 4 字节 == `"NKIT"`**（Dolphin `VolumeDisc::IsNKit()`） | 按其外层容器（ISO/GCZ/RVZ）的能力 | 真正的原始 ISO —— 需用 NKit 工具「转回去」 | ⭐⭐ **CRC32 可能与好 dump 相同但内容不同**（Dolphin 原话）→ **CRC 层的头号误报源** ② 格式无公开规范，NKit 2 闭源、许可未定 |

## 表 2 · 模拟器支持与「是否需要转换才能玩」

| 格式 | RetroArch 支持 | 主流独立模拟器支持 | 是否需要转换才能玩 |
|---|---|---|---|
| **`.zip`** | ✅ 全局支持（zlib 后端）。`need_fullpath=false` 的核心内存解压；`true` 的解压到临时目录；`block_extract` 核心直接吃 zip | ✅ 几乎全部卡带类：Mesen2、Snes9x、ares、mGBA、DeSmuME、melonDS。❌ 全部光盘类：Dolphin / PCSX2 / DuckStation / PPSSPP / Flycast / xemu | **卡带类：否。光盘类：是**（必须先解出镜像） |
| **`.7z`** | ✅ 全局支持（7zip 后端） | ✅ Mesen2、mGBA、DeSmuME、melonDS。❌ Snes9x、ares。❌ 全部光盘类 | **卡带类：多数否**（Snes9x/ares 需转 zip）。**光盘类：是** |
| **`.rar`** | ❌ **不支持**（无 rar 后端，`archive_file_rar.c` 不存在） | ⚠ 仅 **melonDS**（libarchive）与 **DeSmuME Windows 前端**。其余全不支持 | ⭐ **是，几乎必然需要转换**（转 zip 或 7z） |
| **`.zst`** | ✅（单文件 zstd） | ❌ 未见支持 | 是（RetroArch 之外） |
| **`.chd`** | ✅ 前端内置 libchdr 用于扫描；**播放由核心提供**（Beetle PSX / SwanStation / Flycast 等） | ✅ DuckStation、PCSX2、PPSSPP、Flycast。❌ Dolphin | 视平台。**GC/Wii 的 CHD 需转 RVZ** |
| **`.cso`** | **【推断】** 由核心（PPSSPP-libretro / PCSX2-libretro）提供，前端不处理 | ✅ PPSSPP（v1，`version ≤ 1`）、PCSX2 | 否（PSP/PS2） |
| **`.zso`** | **【推断】** 同上，取决于核心 | ⚠ **PCSX2 ✅ / PPSSPP ❌**（`hdr.ver > 1` 报错 "CSO version too high!"；issue #20890 **closed as not planned**） | ⭐ **PSP 上是：ZSO 必须转 CSO 或 ISO** |
| **`.dax`** | **【未找到权威来源】** | ❌ PPSSPP 现版本源码里无 DAX 分派。maxcso 可读写 | **是**（转 CSO/ISO） |
| **`.pbp`** | ✅ 扫描器有 `FILE_TYPE_PBP` 与 serial 提取；播放由核心（Beetle PSX / SwanStation / PPSSPP-libretro） | ✅ DuckStation（PS1 classics）、PPSSPP（EBOOT） | 否 |
| **`.rvz`** | ✅ 扫描器有 `FILE_TYPE_RVZ`（当 ISO 处理，走 serial）；播放由 Dolphin 核心 | ✅ **Dolphin**（官方推荐格式）。❌ 其他 | 否（GC/Wii） |
| **`.wia`** | ✅ 扫描器有 `FILE_TYPE_WIA` | ✅ Dolphin | 否，但**建议转 RVZ**（压缩率更好） |
| **`.wbfs`** | ✅ 扫描器有 `FILE_TYPE_WBFS` | ✅ Dolphin | 否，但 ⭐ **强烈建议转 RVZ** —— WBFS 有损，无法校验，且 RVZ 更小 |
| **`.gcz`** | **【推断】** 由 Dolphin 核心处理 | ✅ Dolphin（旧格式） | 否，但建议转 RVZ |
| **`.nkit.iso` / `.nkit.gcz` / `.nkit.rvz`** | **【推断】** 按外层容器 | ⚠ Dolphin 能读但**会弹警告**："It is not a good dump in its current form, but it might become a good dump if converted back." | **能玩，但识别上必须先转回 ISO** |
| **`.iso` / `.cue+bin` / `.gdi` 等未压缩形态** | ✅ | ✅ | 否 |

---

# 第 5 部分 · 对识别管线的可执行建议

## 5.1 总原则：三条

1. **永远先只读头部。** 任何文件进入管线的第一步都是「读不超过几 KB，判定格式 + 抽取零解压可得的全部信息」。这一步对 7.5 万条目、10 TB 的成本约等于 7.5 万次随机 I/O，**几分钟量级**。
2. **CRC-32 + size 是第一命中层，不是 SHA-1。** 这是唯一能让归档容器「零解压识别」的路径（见 1.4）。SHA-1 只在 CRC 未命中、需要落进沉淀库、或需要仲裁碰撞时才算。
3. **解压是稀缺资源，必须调度。** 一旦决定解压，就按「块」（7z folder / RAR solid 组 / 整个 zip）一次性顺序流式处理完该块内**所有**需要的文件，绝不重复进入同一个块。

## 5.2 按容器类型分派的处理策略

### L0 · 探测（全部文件，只读头部）

```
read_magic(file, 32 bytes)
  "PK\x03\x04" / "PK\x05\x06"       → ZIP
  "7z\xBC\xAF\x27\x1C"              → 7z
  52 61 72 21 1A 07 00              → RAR 1.5–4.x  (RARFMT15)
  52 61 72 21 1A 07 01 00           → RAR 5.0+     (RARFMT50)
  52 45 7E 5E ("RE~^")              → RAR 1.4      (RARFMT14，古董，可忽略)
  "MComprHD"                        → CHD
  "CISO"                            → CSO   （再读 version：0/1 = v1，2 = v2）
  "ZISO"                            → ZSO
  "DAX\x00"                         → DAX
  "\x00PBP"                         → PBP
  "WIA\x01"                         → WIA
  "RVZ\x01"                         → RVZ
  "WBFS"                            → WBFS
  否则 → 按扩展名 + ISO9660 PVD 探测
文件名匹配 (\.part\d+\.rar|\.r\d\d|\.\d{3}|\.z\d\d)$ → 先做分卷聚合，再回到这一步
```

> ⚠ **magic 不一定在偏移 0** —— 自解压（SFX）归档前面挂着一个可执行模块。
> - **RAR**：rarlab technote 原文："You need to **search for this signature in supposed archive from beginning and up to maximum SFX module size**"，且 §SFX 说 "RAR assumes the maximum SFX module size to **not exceed 1 MB**"。unrar `archive.cpp` 的 `Archive::IsArchive()` 正是先试偏移 0，失败则在前 `MAXSFXSIZE` 字节里扫 `0x52`（`'R'`）逐个试 `IsSignature()`。
> - **ZIP / 7z** 同理存在 SFX 形态。ZIP 因为元数据在末尾，扫 EOCD 天然不受影响；7z 则需要扫 magic。
>
> **【推断】** 本项目里 SFX 归档应当极少，可先按「magic 在偏移 0」处理，找不到时再对**扩展名可疑**的文件做一次前 1 MB 扫描，避免对全部 7.5 万文件付出扫描成本。
>
> 签名字节的一手来源：[unrar `archive.cpp` `IsSignature()`](https://github.com/aawc/unrar/blob/master/archive.cpp) —— `D[1..5] == 61 72 21 1a 07` 且 `D[6]==0` → `RARFMT15`、`D[6]==1` → `RARFMT50`、`D[1..3] == 45 7e 5e` → `RARFMT14`。

**分卷聚合规则**（对应 CONTEXT.md 的「成型规则」）：

| 模式 | 聚合键 | 主文件 |
|---|---|---|
| `X.part1.rar` … `X.partN.rar` | `X` | `X.part1.rar` |
| `X.rar`, `X.r00` … `X.rNN` | `X` | `X.rar` |
| `X.z01` … `X.zip` | `X` | **`X.zip`** |
| `X.001` … `X.NNN` | `X` | `X.001`（用拼接读取器，不落盘合并） |
| `X.wbfs`, `X.wbf1` … | `X` | `X.wbfs` |

### L1 · 零解压抽取

| 类型 | 抽取什么 | 用什么 |
|---|---|---|
| ZIP | 遍历 central directory：`(name, uncompressed_size, crc32, method, local_offset)` | `zip` crate |
| 7z | 遍历 header：`(name, size, crc32)` +（若能拿到）`file→folder` 映射 | `sevenz-rust2`；需 folder 映射时 FFI LZMA SDK |
| RAR4/5 | 顺序扫描 block：`(name, unp_size, crc32 或 blake2sp, solid, split_before/after)` | **自实现头部解析器**（本文 1.3 已给完整字段表）→ 避开 UnRAR license |
| CHD | 读 124 字节头：`rawsha1` / `sha1` / `logicalbytes` / `hunkbytes`；读 metadata 链拿轨道表 | `chd` crate |
| CSO/ZSO/DAX | 读定长头：`uncompressed_size` / `block_size` | 自实现（~50 行） |
| PBP | 读 `0x28` 头 → `fseek(param_sfo_offset)` → 解析 SFO → `DISC_ID` / `TITLE` | 自实现（~150 行） |
| WIA/RVZ | 读文件偏移 `0x58` 起 `0x80` 字节 → GC/Wii Game ID + 标题 | 自实现（~30 行） |
| WBFS | 读 `0x00` 头拿 `hd_sector_shift` → 读 `1<<shift` 起 256 字节 → Game ID | 自实现（~30 行） |
| **任何 GC/Wii 逻辑镜像** | ⭐ **读逻辑偏移 `0x200` 判 `"NKIT"`** | 见 5.4 |

### L2 · 匹配

```
优先级 1：serial / 内部 ID 命中（光盘类）
    PBP.DISC_ID / WIA·RVZ·WBFS 的 Game ID / CHD 轨道结构 + 解 hunk 读 SYSTEM.CNF
    → 查 Redump / No-Intro 的 serial 索引
优先级 2：(crc32, size) 命中（归档内部文件）
    → 查 No-Intro / TOSEC / Redump DAT
    → 若唯一命中：自动通过
    → 若多条命中：进入 L3 仲裁
优先级 3：容器自身的 (crc32, size)（照抄 RetroArch 的「先试归档本身」）
优先级 4：未命中 → L3
```

### L3 · 按块调度的解压（只对 L2 未命中的条目）

```
按「块」分组待解压任务：
  ZIP  → 每个条目一个任务，可任意并行
  7z   → 按 folder 分组；每个 folder 一趟顺序流，
          流过程中同时喂给该 folder 内所有目标文件的 hasher（CRC32 + MD5 + SHA-1 同时算）
  RAR  → 按 solid 组分组；用 unrar crate 的顺序 iterator 一趟走完，
          绝不用「按名字逐个 extract」的循环
  压缩镜像 → 不解压，直接部分读（CSO/PBP/CHD/RVZ 都支持随机访问）；
             只有要比 Redump 全盘哈希时才完整解
并行度设在「块」这一层，不是「文件」这一层。
每块解压前用 folder_unpack_size / solid 组总大小 做内存预算，超预算就流式不缓冲。
```

### L4 · 落库（关键，决定第二次扫描的成本）

对每个「容器内文件」持久化：
```
(容器路径, 容器 size, 容器 mtime, 内部路径, 内部 size, crc32, md5, sha1, 识别结论, 依据)
```
第二次扫描时：容器的 `(size, mtime)` 未变 → **完全跳过解压**，甚至跳过读头部（可选：仍读头部对 CRC 做低成本一致性抽检）。

## 5.3 solid block 陷阱的规避办法（汇总）

| # | 办法 | 效果 |
|---|---|---|
| 1 | **CRC-32 优先命中** —— 命中就完全不解压 | 消灭绝大多数解压需求。这是最有效的一条 |
| 2 | **零解压判定 solid**：7z 看 `NumUnPackStreamsInFolders[i] > 1`；RAR 看 `LHD_SOLID` / CompInfo bit `0x0040` | 让调度器知道自己面对什么 |
| 3 | **按块分组、一块一趟** | 把 O(文件数 × 块大小) 降到 O(总大小)，本项目场景下可达 ~100× |
| 4 | **一趟流里并行喂多个 hasher** | 一个 folder 内所有文件的 CRC32/MD5/SHA-1 在同一次解压中全部算出，无需缓冲整块 |
| 5 | **并行度设在块级** | 避免同一块被多线程重复解压 |
| 6 | **内存护栏**：`SzAr_GetFolderUnpackSize` 零解压可得 | 超预算走流式，不 OOM |
| 7 | ⭐ **重打包（治本）**：已识别条目重打包为 **非 solid 的 7z（`-ms=off`）或 zip** | 之后所有增量扫描 O(1)；代价是一次性全量重压 |
| 8 | **超时 + 降级**：单个块解压超过阈值就中止，把该块内条目降级到「待确认队列」 | 防止一个病态大包拖垮整轮扫描 |
| 9 | **先扫小后扫大**：按容器大小升序调度 | 让用户尽早看到结果 |

## 5.4 必须在管线里显式处理的「坑」清单

| # | 坑 | 处理 |
|---|---|---|
| 1 | **ZIP flag bit 3** → local header 的 CRC/大小为 0 | 只读 central directory，永不读 local header 的这三个字段 |
| 2 | **ZIP64** → 4 GB 以上的大小/偏移在 extra field | 解析 tag `0x0001`；用 `zip` crate 可自动处理 |
| 3 | **RAR5 只写 BLAKE2sp 不写 CRC32** | 检查 file flag `0x0004`；缺 CRC32 时该条目直接进 L3 完整解压 |
| 4 | **分卷 RAR 的中间卷 CRC 是「打包后数据」的 CRC** | 只取 `SPLIT_AFTER == false` 的那一段的 CRC；或干脆对分卷 RAR 一律走 L3 |
| 5 | **7z / RAR 头部加密**（`-mhe=on` / `-hp`） | 连文件名都读不到 → 直接进「待确认队列」，标注「需密码」 |
| 6 | ⭐ **NKit 假命中 CRC32** | **CRC 匹配之前**先读 GC/Wii 逻辑偏移 `0x200` 判 `"NKIT"`。命中 NKit → 不允许自动通过，标注「NKit 处理过，需转回 ISO」 |
| 7 | **WBFS 有损** | 不要尝试用 WBFS 重建 ISO 去比 Redump。只能走 Game ID → serial 匹配 |
| 8 | **CHD 的两个 SHA-1 都不等于 Redump 的任何哈希** | 只用 `sha1`(combined) 做**本地沉淀库主键**与 MAME `<disk>` 校验；对 Redump 一律走轨道结构 + serial，或 `extractcd --splitbin` 完整校验 |
| 9 | **差分 CHD**（`parentsha1 != 0`） | 头部零解压即可判定；缺父文件时标注「缺少父镜像」，不要当成损坏 |
| 10 | **PBP 加密**（`+0x400` == `"\0PGD"`） | 只能读 `PARAM.SFO`，PSAR 内容不可读 → serial 匹配是唯一路径 |
| 11 | **`.cue` 与 `.bin` 双重计数** | 照抄 RetroArch 的 `task_database_cue_prune`：命中 `.cue` 后把它引用的 `.bin` 从扫描列表剔除 |
| 12 | **CSO v2 / ZSO / DAX 的可玩性** | 识别没问题（头部结构清楚），但**导出到子库时要按目标模拟器转换**（见 5.5） |
| 13 | **CRC-32 碰撞** | 始终用 `(crc32, size)` 双约束；多条命中时进 L3 用 SHA-1 仲裁 |
| 14 | **UnRAR license** | L1 用自实现头部解析器；L3 若用 `unrar` crate，产品许可声明必须附带 UnRAR license 第 2 段全文 |

## 5.5 与「子库 / 同步」的联动（格式转换规则）

CONTEXT.md 里子库带「格式转换规则」。基于本文的支持矩阵，给出一份可直接落成默认规则的建议：

| 源格式 | 目标（通用/掌机子库） | 理由 |
|---|---|---|
| `.rar`（卡带类） | **→ `.zip`** | rar 几乎无人支持；zip 支持面最广、且解压最快 |
| `.rar`（光盘类） | **→ 解出镜像，再按下面各行处理** | 光盘类模拟器一律不吃归档 |
| solid `.7z`（卡带类） | **→ 非 solid `.7z`（`-ms=off`）或 `.zip`** | 消除后续扫描的 solid 代价；zip 兼容 Snes9x/ares |
| `.zip` / `.7z`（光盘类，内含 iso/cue+bin） | **→ 解包后按平台转 `.chd` / `.rvz` / `.cso`** | 光盘类模拟器不吃归档；且压缩镜像本身就更省空间 |
| `.wbfs` / `.gcz` / `.wia` | **→ `.rvz`** | RVZ 无损、压缩率最好、Dolphin 官方推荐；WBFS 有损且无法校验 |
| `.nkit.*` | **→ 先转回 `.iso`，重新识别，再转 `.rvz`** | 消除 CRC 假命中 |
| `.zso` / `.dax`（PSP） | **→ `.cso`（v1）** | PPSSPP 不支持 ZSO/CSO v2/DAX |
| `.cso`（PS2） | 保留，或 → `.chd` | PCSX2 两者都支持；CHD 在 PS2 上通常更小 |
| `.cue + .bin`（PS1/Saturn/DC/PCE-CD） | **→ `.chd`** | DuckStation / Flycast / Beetle 全支持，体积显著更小 |
| `.iso`（PSP） | → `.cso` | 省空间且 PPSSPP 原生支持 |

> ⚠ **转换会改变文件哈希**，因此：
> - **转换必须发生在「识别」之后**，且中立库里记录的是**原始变体的哈希**；
> - 子库里的派生文件在「清单」里要记录 `(源变体 ID, 目标格式, 目标哈希)`，同步比对时用源变体 ID 而不是哈希；
> - 主库应当保留原始形态（CONTEXT.md：「主库保存原始形态的全部资源」）。

## 5.6 性能可行性结论

**【推断】**（基于本文全部事实的综合推理，未实测）：

| 阶段 | 覆盖范围 | 主要成本 | 量级 |
|---|---|---|---|
| L0 探测 | 7.5 万条目全部 | 每文件读 ≤ 32 字节 | 分钟级（受随机 I/O 限制） |
| L1 零解压抽取 | 7.5 万条目全部 | ZIP/RAR 读头部；7z 多解一次小 header；CHD 读 124B+metadata；镜像读 ≤ 1 KB | **十分钟级** |
| L2 匹配 | 7.5 万条目全部 | 纯内存哈希表查找 | 秒级 |
| L3 解压 | **只有 L2 未命中的部分**（主要是汉化版/魔改版/未收录） | 顺序解压 + 多 hasher | **取决于未命中率**。若未命中率 10%（1 TB），单机顺序解压约数小时 |
| L4 落库 | — | — | — |

**关键判断：项目性能可行，前提是第一命中层用 CRC-32 而非 SHA-1，且 solid block 按块调度。** 如果这两条任一不满足，L3 的工作量会从「1 TB」膨胀到「10 TB 甚至 100 TB」，项目就不可行了。

---

# 附录 · 一手来源清单

## 归档格式规范

| 内容 | URL |
|---|---|
| PKWARE APPNOTE.TXT 6.3.10（ZIP 官方规范） | <https://pkwaredownloads.blob.core.windows.net/pkware-general/Documentation/APPNOTE-6.3.10.TXT> |
| 7-Zip 官方 `DOC/7zFormat.txt` | <https://github.com/ip7z/7zip/blob/main/DOC/7zFormat.txt> |
| 7-Zip 官方 7z 格式介绍页 | <https://7-zip.org/7z.html> |
| 7-Zip FAQ | <https://7-zip.org/faq.html> |
| LZMA SDK `C/7z.h`（`SzArEx_Extract` / `CSzArEx` / solid block 注释，public domain） | <https://github.com/ip7z/7zip/blob/main/C/7z.h> |
| 7-Zip `SplitHandler.cpp`（`.001` 分卷的实现） | <https://github.com/ip7z/7zip/blob/main/CPP/7zip/Archive/SplitHandler.cpp> |
| RARLAB 官方 RAR 5.0 格式规范（technote） | <https://www.rarlab.com/technote.htm> |
| unrar 源码 `headers.hpp`（RAR4 旗标与结构） | <https://github.com/aawc/unrar/blob/master/headers.hpp> |
| unrar 源码 `headers5.hpp`（RAR5 旗标） | <https://github.com/aawc/unrar/blob/master/headers5.hpp> |
| unrar 源码 `arcread.cpp`（实际解析顺序） | <https://github.com/aawc/unrar/blob/master/arcread.cpp> |
| unrar 源码 `hash.hpp`（HASH_TYPE / blake2sp） | <https://github.com/aawc/unrar/blob/master/hash.hpp> |
| unrar 源码 `pathfn.cpp`（`NextVolumeName` 分卷命名） | <https://github.com/aawc/unrar/blob/master/pathfn.cpp> |
| **UnRAR license 全文** | <https://github.com/aawc/unrar/blob/master/license.txt> |
| Debian `unrar-nonfree`（non-free 归类） | <https://tracker.debian.org/pkg/unrar-nonfree> |
| libarchive README（RAR 支持声明） | <https://github.com/libarchive/libarchive/blob/master/README.md> |

## 压缩光盘镜像

| 内容 | URL |
|---|---|
| MAME `src/lib/util/chd.h`（CHD V1–V5 头部与 map 格式） | <https://github.com/mamedev/mame/blob/master/src/lib/util/chd.h> |
| libchdr | <https://github.com/rtissera/libchdr> |
| chdman 官方文档 | <https://docs.mamedev.org/tools/chdman.html> |
| maxcso `README_CSO.md`（CSO v1/v2 规范） | <https://github.com/unknownbrackets/maxcso/blob/master/README_CSO.md> |
| maxcso `README_ZSO.md`（ZSO 规范） | <https://github.com/unknownbrackets/maxcso/blob/master/README_ZSO.md> |
| maxcso `src/dax.h`（DAX 头部结构） | <https://github.com/unknownbrackets/maxcso/blob/master/src/dax.h> |
| maxcso README（工具能力与许可） | <https://github.com/unknownbrackets/maxcso/blob/master/README.md> |
| PPSSPP `Core/FileSystems/BlockDevices.cpp`（CISO_H struct、格式分派） | <https://github.com/hrydgard/ppsspp/blob/master/Core/FileSystems/BlockDevices.cpp> |
| PPSSPP `Core/Loaders.cpp`（扩展名识别） | <https://github.com/hrydgard/ppsspp/blob/master/Core/Loaders.cpp> |
| PPSSPP issue #20890（ZSO / CSO v2 not planned） | <https://github.com/hrydgard/ppsspp/issues/20890> |
| DuckStation `src/util/cd_image_pbp.cpp`（PBP + SFO + PSAR） | <https://github.com/stenzek/duckstation/blob/master/src/util/cd_image_pbp.cpp> |
| DuckStation `src/util/cd_image.cpp`（支持的镜像格式） | <https://github.com/stenzek/duckstation/blob/master/src/util/cd_image.cpp> |
| PSDevWiki PARAM.SFO | <https://www.psdevwiki.com/ps3/PARAM.SFO> |
| **Dolphin `docs/WiaAndRvz.md`（WIA/RVZ 完整格式文档）** | <https://github.com/dolphin-emu/dolphin/blob/master/docs/WiaAndRvz.md> |
| Dolphin `Source/Core/DiscIO/WIABlob.h`（结构体与 static_assert） | <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/WIABlob.h> |
| Dolphin `Source/Core/DiscIO/Blob.cpp`（格式分派 / BlobType） | <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/Blob.cpp> |
| Dolphin `Source/Core/DiscIO/WbfsBlob.cpp`（WBFS 头部与读取） | <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/WbfsBlob.cpp> |
| Dolphin `Source/Core/DiscIO/VolumeDisc.cpp`（`IsNKit()` 检测） | <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/VolumeDisc.cpp> |
| Dolphin `Source/Core/DiscIO/VolumeVerifier.cpp`（NKit 警告、RedumpVerifier） | <https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/DiscIO/VolumeVerifier.cpp> |
| NKit / NKDS 官方仓库 | <https://github.com/Nanook/NKit> |

## 模拟器与前端

| 内容 | URL |
|---|---|
| RetroArch `libretro-common/file/archive_file.c`（后端分派 —— **无 rar**） | <https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file.c> |
| RetroArch `archive_file_zlib.c`（central directory 解析） | <https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file_zlib.c> |
| RetroArch `archive_file_7z.c` | <https://github.com/libretro/RetroArch/blob/master/libretro-common/file/archive_file_7z.c> |
| RetroArch `tasks/task_content.c`（need_fullpath / block_extract / 临时解压） | <https://github.com/libretro/RetroArch/blob/master/tasks/task_content.c> |
| RetroArch `tasks/task_database.c`（扫描器两段式策略） | <https://github.com/libretro/RetroArch/blob/master/tasks/task_database.c> |
| RetroArch 内置 libchdr | <https://github.com/libretro/RetroArch/blob/master/libretro-common/streams/chd_stream.c> |
| libretro 核心开发文档（need_fullpath） | <https://docs.libretro.com/development/cores/developing-cores/> |
| Mesen2 `Utilities/ArchiveReader.cpp` | <https://github.com/SourMesen/Mesen2/blob/master/Utilities/ArchiveReader.cpp> |
| Snes9x `memmap.cpp`（zip / JMA） | <https://github.com/snes9xgit/snes9x/blob/master/memmap.cpp> |
| ares `desktop-ui/emulator/emulator.cpp`（`*.zip` 过滤器） | <https://github.com/ares-emulator/ares> |
| mGBA README（"Loading from ZIP and 7z files"） | <https://github.com/mgba-emu/mgba/blob/master/README.md> |
| DeSmuME File_Extractor（zip/7z/rar） | <https://github.com/TASEmulators/desmume/tree/master/desmume/src/frontend/windows/File_Extractor> |
| melonDS `ArchiveUtil.cpp` + `CMakeLists.txt`（libarchive） | <https://github.com/melonDS-emu/melonDS/blob/master/src/frontend/qt_sdl/ArchiveUtil.cpp> |
| PCSX2 `pcsx2/CDVD/InputIsoFile.cpp`（chd/cso/zso/gz 分派） | <https://github.com/PCSX2/pcsx2/blob/master/pcsx2/CDVD/InputIsoFile.cpp> |
| PCSX2 `pcsx2/CDVD/CsoFileReader.cpp`（CISO/ZISO magic） | <https://github.com/PCSX2/pcsx2/blob/master/pcsx2/CDVD/CsoFileReader.cpp> |
| Flycast `core/imgread/common.cpp`（chd/gdi/cdi/cue） | <https://github.com/flyinghead/flycast/blob/master/core/imgread/common.cpp> |
| xemu 官方 disc images 文档 | <https://xemu.app/docs/disc-images/> |
| ES-DE USERGUIDE | <https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md> |
| Pegasus 元数据文件文档 | <https://pegasus-frontend.org/docs/user-guide/meta-files/> |

## Rust 生态

| crate | URL |
|---|---|
| `zip`（zip-rs/zip2） | <https://github.com/zip-rs/zip2> · <https://docs.rs/zip/latest/zip/read/struct.ZipFile.html> |
| `rc-zip` / `rc-zip-sync` | <https://github.com/bearcove/rc-zip> |
| `sevenz-rust2` | <https://github.com/hasenbanck/sevenz-rust> · <https://docs.rs/sevenz-rust2/latest/sevenz_rust2/struct.ArchiveEntry.html> |
| `unrar` / `unrar_sys` | <https://github.com/muja/unrar.rs> · <https://docs.rs/unrar/latest/unrar/struct.FileHeader.html> |
| `unrar-ng` | <https://github.com/ttys3/unrar.rs> |
| `rars`（纯 Rust，很新） | <https://github.com/bitplane/rars> |
| `nzbdav-rar`（纯 Rust RAR4/5 头部解析） | <https://github.com/TheDancingDeveloper-org/nzbdav-rs> |
| `unarc-rs` | <https://github.com/mkrueger/unarc-rs> |
| **`chd`（chd-rs）** | <https://github.com/SnowflakePowered/chd-rs> |
| `lzma-rs` / `ppmd-rust` / `flate2` | <https://github.com/gendx/lzma-rs> · <https://github.com/hasenbanck/ppmd-rust> · <https://github.com/rust-lang/flate2-rs> |
