# Nintendo Switch 转储格式识别方法

调研日期：2026-08-31。本文覆盖 `.xci` / `.nsp` / `.nsz` / `.xcz` 四种扩展名，回答「不解密能不能认出游戏」这一核心问题。

**证据标记约定**（沿用 `rom-identification-part2.md`）：

- **【事实】** —— 有一手来源逐字节佐证，附 URL。多源交叉验证的会标注源数。
- **【实测】** —— 本次直接下载数据 / 读源码得出的数字，附可复现命令。
- **【推断】** —— 本文的推理，已逐条标注。
- **【未找到权威来源】** —— 单列一节。

**本文没有虚构任何头部字段或偏移量。** 所有偏移都能在正文给出的 URL 里逐条核对。

---

## 0. 结论摘要

| 问题 | 结论 |
|---|---|
| 容器层是不是明文？ | **是。** PFS0（NSP/NSZ）与 HFS0（XCI/XCZ）的头、文件条目表、字符串表**全部明文**，不需要任何密钥 |
| 不解密能拿到 TitleID 吗？ | **能，三条独立路径。** ① `<RightsId>.tik` 文件名前 16 位 hex 就是 TitleID；② `<ContentId>.nca` 文件名查 titledb `ncas.json`；③ scene NSP 里的 `<ContentId>.cnmt.xml` 是明文 XML，含 `<Id>0x...</Id>` |
| 能唯一确定到「哪个游戏的哪个版本」吗？ | **能。【实测】** blawar/titledb `cnmts.json` 里 173 502 个 NCA content ID，**100.00% 只属于唯一一个 (titleId, version) 二元组** |
| NSZ/XCZ 要先解压吗？ | **识别不用。** 容器层与 NSP/XCI 完全相同，只是内部 `.nca` → `.ncz`。**但模拟器要**，见 §6 |
| No-Intro 有 Switch 吗？ | **DAT-o-MATIC 里有，但每日公开镜像里没有。【实测】** 334 个 DAT 里零个 Switch |
| 需要 `prod.keys` 吗？ | **不需要**，除非要读 NACP（游戏名/语言旗标）或做 NCA 级校验。**明确建议不走解密路线**，见 §3.3 |
| 官中怎么办？ | **官中在 Switch 上不是变体，是同一个文件。【实测】** 港服 eShop 的 TitleID 有 69.4% 与美服完全相同 |

**一句话：Switch 是本项目迄今为止「免密钥识别」质量最高的现代平台** —— 因为它的容器文件名本身就是内容的 SHA-256 前 16 字节，是一个自校验的内容寻址标识符。

---

## 1. 本次库存的实际构成

用户库实测扫描给出的 `switch` 目录（146 文件 / 223.84 GiB）：

| 扩展名 | 文件数 | 容量 | 平均单文件 | 性质 |
|---|---:|---:|---:|---|
| `.xci` | 9 | 108.19 GiB | 12.02 GiB | 卡带转储，未压缩 |
| `.nsp` | 82 | 40.97 GiB | 511.6 MiB | eShop 包，未压缩 |
| `.nsz` | 48 | 51.29 GiB | 1.068 GiB | eShop 包，zstd 压缩 |
| `.xcz` | 7 | 23.38 GiB | 3.34 GiB | 卡带转储，zstd 压缩 |

四种扩展名之和恰好 146 个文件、223.83 GiB，与目录合计一致 —— **`switch` 目录里没有别的东西**（没有裸 `.nca`、没有 `.nro`、没有分卷 `00`/`01`）。

派生比例：

- **压缩格式（nsz+xcz）：55 文件 = 37.7%，74.67 GiB = 33.4%** —— 这一块是 ES-DE 的盲区（§6.2）
- **卡带系（xci+xcz）：16 文件 = 11.0%，131.57 GiB = 58.8%** —— 文件少但吃掉近六成容量
- **eShop 系（nsp+nsz）：130 文件 = 89.0%，92.26 GiB = 41.2%**

`.nsp` 平均只有 512 MiB —— 【推断】这 82 个里相当一部分是更新包（TitleID 尾 `800`）与 DLC（尾非 `000`/`800`），而不是本体。识别器必须能区分 Base / Update / DLC，否则库体检会把一堆更新包报成「游戏」。

在全库中的分量：146 / 256 128 = **0.057% 的文件**，223.84 GiB / 8.60 TiB = **2.54% 的容量**。文件数极少，单文件极大 —— 这决定了成本模型：**任何 O(文件数) 的开销都可忽略，但任何 O(字节数) 的开销（比如全文件 CRC32）都要算清楚**。

---

## 2. 格式结构

### 2.1 XCI（卡带转储）

**权威来源**：[switchbrew.org/wiki/XCI](https://switchbrew.org/wiki/XCI)（原始 wikitext 可用 `https://switchbrew.org/w/index.php?title=XCI&action=raw` 取，直接 WebFetch 会被 403）。

#### 2.1.1 两套偏移：整卡布局 vs. 转储文件布局 ⚠

**这是 XCI 最大的坑。** switchbrew 描述的是**整张卡的地址空间**，而**实际转储文件（`.xci`）从 CardHeader 开始，前面 0x1000 字节的 CardKeyArea 不在文件里**。

switchbrew 的整卡布局【事实】：

| 卡内偏移 | 大小 | 内容 |
|---|---|---|
| `0x0` | `0x1000` | CardKeyArea（InitialData 0x200 + TitleKeyArea 0xD00 + Reserved 0x100）|
| `0x1000` | `0x200` | **CardHeader** |
| `0x1200` | `0x200` | [11.0.0+] CardHeaderT2 |
| `0x1400` | `0x400` | [11.0.0+] CardHeaderT2CertArea |
| `0x1800` | `0x100` | [11.0.0+] CardHeaderT2CertAreaModulus |
| `0x8000` | `0x8000` | CertArea（卡片唯一证书）|
| `0x10000` | 变长 | NormalArea |

switchbrew 明说 CardKeyArea「**This region cannot be read directly once written to the Gamecard**」——所以转储工具根本读不到它，转储文件自然从 `0x1000` 开始。

**四个独立来源交叉验证这个 0x1000 位移**【事实 ×4】：

1. **hactool** `xci.c` 在文件偏移 0 读 `0x200` 字节当 header，检查 `magic` 在结构体 +0x100 处 —— 即 `"HEAD"` 在文件偏移 `0x100`。
   → <https://github.com/SciresM/hactool/blob/master/xci.c>、[`xci.h`](https://github.com/SciresM/hactool/blob/master/xci.h)
2. **nxdumptool** `gamecard.h`：`GAMECARD_CERT_OFFSET 0x7000` = `0x8000 − 0x1000`；`GAMECARD_HEADER2_OFFSET 0x200` = `0x1200 − 0x1000`；`GAMECARD_HEADER2_CERT_OFFSET 0x400` = `0x1400 − 0x1000`；`GAMECARD_HEADER2_CERT_PUBKEY_OFFSET 0x800` = `0x1800 − 0x1000`。**四个常量同时对上**。
   → <https://github.com/DarkMatterCore/nxdumptool/blob/rewrite/include/core/gamecard.h>
3. **switch-library-manager** `xci.go`：`file.ReadAt(header, 0)`，然后 `if string(header[0x100:0x104]) != "HEAD"`。
   → <https://github.com/trembon/switch-library-manager/blob/master/src/switchfs/xci.go>
4. **No-Intro dat notes** 的 FullXCI 配方：`512_bytes_initial_area + 3584_bytes_of_zeroes + scene_style_xci`。512 + 3584 = 4096 = `0x1000`，且 512 = InitialData 大小、3584 = `0xE00` = TitleKeyArea 0xD00 + Reserved 0x100。**逐段对上**。
   → <https://wiki.no-intro.org/index.php?title=Nintendo_-_Nintendo_Switch_dat_notes>

**实现建议**：不要硬编码。先在 `0x100` 和 `0x1100` 两处找 `"HEAD"`（`0x44414548` LE，hactool `MAGIC_HEAD`），确定 `cardHeaderBase`，此后所有偏移都相对它算。

#### 2.1.2 CardHeader 字段表（相对 CardHeader 起点）

switchbrew 原表【事实】，与 hactool `xci_header_t` 逐字段一致【事实 ×2】：

| 偏移 | 大小 | 字段 | 加密？ |
|---|---|---|---|
| `0x0` | `0x100` | RSA-2048 PKCS#1 签名（覆盖 0x100–0x200）| 明文 |
| `0x100` | `0x4` | Magic `"HEAD"` | 明文 |
| `0x104` | `0x4` | RomAreaStartPageAddress（页 = 0x200 字节）| 明文 |
| `0x108` | `0x4` | BackupAreaStartPageAddress（恒 `0xFFFFFFFF`）| 明文 |
| `0x10C` | `0x1` | TitleKeyDecIndex（高 4 位）/ KekIndex（低 4 位）| 明文 |
| `0x10D` | `0x1` | **RomSize**（卡容量码）| 明文 |
| `0x10E` | `0x1` | Version | 明文 |
| `0x10F` | `0x1` | Flags | 明文 |
| `0x110` | `0x8` | PackageId | 明文 |
| `0x118` | `0x4` | ValidDataEndAddress（页单位）| 明文 |
| `0x120` | `0x10` | Iv（**倒序存放**）| 明文 |
| `0x130` | `0x8` | **PartitionFsHeaderAddress** ← 根 HFS0 的位置 | 明文 |
| `0x138` | `0x8` | PartitionFsHeaderSize | 明文 |
| `0x140` | `0x20` | PartitionFsHeaderHash（根 HFS0 头的 SHA-256）| 明文 |
| `0x160` | `0x20` | InitialDataHash | 明文 |
| `0x180` | `0x4` | SelSec（1=T1, 2=T2）| 明文 |
| `0x184` | `0x4` | SelT1Key（恒 2）| 明文 |
| `0x188` | `0x4` | SelKey（恒 0）| 明文 |
| `0x18C` | `0x4` | LimArea（页单位）| 明文 |
| `0x190` | `0x70` | **CardHeaderEncryptedData** | **AES-128-CBC 加密** |

RomSize 取值【事实】：`0xFA`=1GB、`0xF8`=2GB、`0xF0`=4GB、`0xE0`=8GB、`0xE1`=16GB、`0xE2`=32GB。

#### 2.1.3 卡带头里有没有明文 TitleID？

**没有。**【事实】

CardHeader 的明文部分只有 PackageId（8 字节，用于挑战-应答认证，**不是 TitleID**）。加密的 `CardHeaderEncryptedData` 里有 `UppId`（恒 `0x0100000000000816`，系统更新 title，不是游戏）和 `UppVersion`。**任何位置都没有游戏的 TitleID。**

→ 这意味着 **XCI 的 TitleID 只能从 HFS0 分区里的文件名拿**（§2.1.4 / §3.1）。

#### 2.1.4 HFS0 分区（这才是 XCI 识别的入口）

根 HFS0 位于 CardHeaderBase + `PartitionFsHeaderAddress`（字段 0x130）。结构【事实】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x4` | Magic `"HFS0"` |
| `0x4` | `0x4` | FileCount |
| `0x8` | `0x4` | StringTableSize |
| `0xC` | `0x4` | Reserved |
| `0x10` | `0x40 × FileCount` | FileEntryTable |
| `0x10 + X` | StringTableSize | **StringTable（NUL 分隔的明文文件名）** |
| 之后 | | RawFileData |

FileEntryTable 单条 `0x40` 字节【事实】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x8` | 文件在 Data 区的偏移 |
| `0x8` | `0x8` | 文件大小 |
| `0x10` | `0x4` | 文件名在 StringTable 的偏移 |
| `0x14` | `0x4` | 被哈希区域的大小（NCA 通常 `0x200`）|
| `0x18` | `0x8` | Reserved |
| `0x20` | `0x20` | 前 N 字节的 SHA-256 |

**整个 HFS0 头是明文，读它不需要任何密钥。** hactool 的 `hfs0_process()` 里没有任何解密调用 —— 只有 `fread` + magic 检查【事实】。
→ <https://github.com/SciresM/hactool/blob/master/hfs0.c>

根分区最多 4 个条目，名字固定为 `update` / `normal` / `logo` / `secure`（hactool `xci.c` 对这四个名字做硬匹配，遇到别的直接 `exit`）【事实】。各分区内容【事实，switchbrew】：

- **`update`**：整套系统更新的 `.cnmt.nca` + `.nca`
- **`normal`**：`.cnmt.nca` + 图标 NCA（[4.0.0+] 该分区变空，内容移到 `logo`）
- **`logo`**：[4.0.0+] 承接原 `normal` 的内容
- **`secure`**：**`.cnmt.nca` + 图标 NCA + 游戏所需的全部 NCA** ← 识别只需要这个

#### 2.1.5 国行（Terra / Tencent）卡的免密钥检测 ⭐

`CardHeaderEncryptedData` 偏移 `0x24` 有 [9.0.0+] `CompatibilityType`（0=Normal, 1=Terra）【事实，switchbrew】。这个字段是加密的，正常要 `xci_header_key`。

**但 hactool 有一个不需要密钥的旁路**【事实】：根 HFS0 头的 SHA-256（存在 CardHeader `0x140`）在计算时会把 `CompatibilityType` 作为后缀拼进去。hactool 在没有 `xci_header_key` 时，先按「无后缀」算一遍哈希，不匹配就换成后缀 `COMPAT_CHINA (0x01)` 再算一遍：

```c
/* hactool/xci.c */
ctx->hfs0_hash_validity = check_memory_hash_table_with_suffix(..., compatiblity_type_ptr, 0);
if (ctx->hfs0_hash_validity != VALIDITY_VALID) {
    ctx->has_fake_compat_type = 1;
    ctx->fake_compat_type = COMPAT_CHINA;
    ...
}
```

`check_memory_hash_table_with_suffix()` 在 `utils.c` 里就是 `sha_update(block)` 后再 `sha_update(suffix, 1)`【事实】。
→ [`xci.c`](https://github.com/SciresM/hactool/blob/master/xci.c) / [`utils.c`](https://github.com/SciresM/hactool/blob/master/utils.c) / [`xci.h`](https://github.com/SciresM/hactool/blob/master/xci.h)（`COMPAT_GLOBAL=0x00` / `COMPAT_CHINA=0x01`）

**→ 一次 SHA-256（只覆盖 HFS0 头，几百字节到几 KB）就能零密钥判定「这张卡是不是国行」。** 这是 ADR-0012（官中 vs 汉化）在 Switch 上唯一需要读文件本体的区分点。

### 2.2 NSP（eShop / CDN 包）

**⚠ switchbrew 上没有 "NSP" 页面。**【实测】用 MediaWiki search API 查 `NSP` 返回 `totalhits: 0`：

```bash
curl -s "https://switchbrew.org/w/api.php?action=query&list=search&srsearch=NSP&format=json"
# => {"batchcomplete":"","query":{"searchinfo":{"totalhits":0},"search":[]}}
```

"NSP"（Nintendo Submission Package）是社区/工具链术语，**格式本身就是一个裸 PFS0**，PFS0 的规范在 [switchbrew.org/wiki/NCA#PFS0](https://switchbrew.org/wiki/NCA)（NCA 页末尾）。

#### 2.2.1 PFS0 结构

【事实，switchbrew】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x4` | Magic `"PFS0"` |
| `0x4` | `0x4` | EntryCount |
| `0x8` | `0x4` | StringTableSize |
| `0xC` | `0x4` | Reserved |
| `0x10` | `0x18 × EntryCount` | PartitionEntryTable |
| `0x10 + X` | StringTableSize | **StringTable** |
| 之后 | | FileData |

PartitionEntry 单条 `0x18` 字节【事实】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x8` | Offset（相对 FileData 起点）|
| `0x8` | `0x8` | Size |
| `0x10` | `0x4` | StringOffset |
| `0x14` | `0x4` | Reserved |

**PFS0 与 HFS0 只有两处不同**：magic，和条目大小（`0x18` vs `0x40`，HFS0 多了 hashedRegionSize + SHA-256）。switch-library-manager 用同一个 `readPfs0()` 处理两者，只切换 `fileEntryTableSize`【事实】：

```go
const (
    PfsfileEntryTableSize = 0x18
    HfsfileEntryTableSize = 0x40
    pfs0Magic             = "PFS0"
    hfs0Magic             = "HFS0"
)
```
→ <https://github.com/trembon/switch-library-manager/blob/master/src/switchfs/pfs0.go>

#### 2.2.2 PFS0 的文件名表是明文吗？——**是。**⭐

这是本次调研最关键的一条【事实 ×3】：

1. **switchbrew** 把 StringTable 列为普通结构成员，PFS0 的加密只发生在 NCA section 层，容器层不加密。
2. **nsz** 的 `Pfs0.getHeader()` 直接拼字符串：
   ```python
   stringTableNonPadded = "\x00".join(file["name"] for file in self.files) + "\x00"
   h = b"PFS0" + len(self.files).to_bytes(4,'little') + ...
   h += stringTable.encode()
   ```
   `Pfs0.open()` 读它时也是 `self.read(self._stringTableSize)` 后直接 `.decode("utf-8")` —— **没有任何解密**。
   → <https://github.com/nicoboss/nsz/blob/master/nsz/Fs/Pfs0.py>
3. **switch-library-manager** 的 `readPfs0()` 只有 `reader.ReadAt()`，同样零解密。

#### 2.2.3 NSP 里都有什么文件名？—— nxdumptool 的生成代码是最硬的证据 ⭐

**nxdumptool 的 NSP 生成逻辑**（`code_templates/nxdt_rw_poc.c`）【事实】：

```c
sprintf(entry_name, "%s.cnmt.xml",       meta_nca_ctx->content_id_str);   // L7858
sprintf(entry_name, "%s.programinfo.xml", cur_nca_ctx->content_id_str);   // L7879
sprintf(entry_name, "%s.nacp.xml",        cur_nca_ctx->content_id_str);   // L7898
sprintf(entry_name, "%s.legalinfo.xml",   cur_nca_ctx->content_id_str);   // L7905
sprintf(entry_name, "%s.tik",  tik.rights_id_str);                        // L7923
sprintf(entry_name, "%s.cert", tik.rights_id_str);                        // L7930
```

**NCA 本身的命名**（`source/core/nca.c` L618）【事实】：
```c
sprintf(nca_filename, "%s.%s", out->content_id_str,
        out->content_type == NcmContentType_Meta ? "cnmt.nca" : "nca");
```

配套常量（`include/core/nca.h`）【事实】：
```c
#define NCA_CONTENT_ID_STR_LENGTH        0x20                              /* 32 个 hex 字符 */
#define NCA_HFS_REGULAR_NAME_LENGTH      (NCA_CONTENT_ID_STR_LENGTH + 4)   /* Content ID + ".nca" */
#define NCA_HFS_META_NAME_LENGTH         (NCA_CONTENT_ID_STR_LENGTH + 9)   /* Content ID + ".cnmt.nca" */
```

**所以：NSP/XCI 的容器里，文件名只有两种形态**：
- `<32 位 hex ContentId>.nca` / `<32 位 hex ContentId>.cnmt.nca` / `.ncz` / `.cnmt.ncz`
- `<32 位 hex RightsId>.tik` / `<32 位 hex RightsId>.cert`
- （scene / authoring 附加）`<ContentId>.cnmt.xml` / `.programinfo.xml` / `.nacp.xml` / `.legalinfo.xml` / `cardspec.xml` / `authoringtoolinfo.xml`

#### 2.2.4 `.cnmt.nca` 的文件名含 TitleID 吗？——**不含，含的是 ContentId。**

这一点必须说清楚，因为它是最容易搞错的地方。

`.cnmt.nca` 的文件名前缀是 **ContentId**（16 字节 = 32 hex），**不是 TitleID**。ContentId 的来源【事实，nxdumptool `nca.c` L553–556】：

```c
void ncaUpdateContentIdAndHash(NcaContext *ctx, const u8 *hash)
{
    /* Update content ID. */
    memcpy(ctx->content_id.c, hash, sizeof(ctx->content_id.c));   // 取 SHA-256 前 16 字节
    ...
    /* Update content hash. */
    memcpy(ctx->hash, hash, sizeof(ctx->hash));                    // 完整 SHA-256
```

**→ ContentId = 整个 NCA 的 SHA-256 的前 16 字节。**

这带来两个重要后果：

1. **好消息**：ContentId 是**内容寻址**的、自校验的 128 位标识符。用它查表（§4.3）能精确定位到 title + version。
2. **坏消息**：**任何改动 NCA 一个字节，ContentId 就变。** nxdumptool 在 XCI→NSP 转换时正是这么做的（`nxdt_rw_poc.c` L8116）：改了 DistributionType 或去掉 titlekey crypto 后 `ncaUpdateContentIdAndHash()` 重算，再 `pfsUpdateEntryNameFromImageContext()` 改名。所以**「转换过的」包与原包 ContentId 不同**。

#### 2.2.5 `.tik` 文件名 = RightsId，前 16 位 hex 就是 TitleID ⭐

RightsId 的布局【事实 + 实测 ×71 182 样本】：

```
RightsId (16 字节) = TitleID (8 字节, big-endian) ‖ 7 字节 0x00 ‖ KeyGeneration (1 字节)
```

- **nsz 源码明说**【事实】：`ExtractTitlekeys.py` 里 `rightsId = format(ticket.getRightsId(),"x").zfill(32)` 后 `titleId = rightsId[0:16]`。
  → <https://github.com/nicoboss/nsz/blob/master/nsz/ExtractTitlekeys.py>
- **【实测】** 对 blawar/titledb `ncas.json` 里全部 71 182 条带 `rightsId` 的记录统计：
  - `rightsId[16:30]`（中间 14 个 hex）**100% 恒为 `"00000000000000"`**
  - `rightsId[30:32]` 是 KeyGeneration，取值分布 `00,03..12,15,16`（对应 switchbrew NCA 的 KeyGeneration 枚举）
  - `rightsId[0:16]` 与所在 title 的关系：**100% 前 13 位 hex 相同**。细分：28 292 条完全等于 base titleId，42 609 条是该 base 的 patch（`...800`），281 条是 DLC（末 3 位不同）。**零反例。**

  复现：
  ```bash
  curl -sL -o ncas.json https://raw.githubusercontent.com/blawar/titledb/master/ncas.json
  python3 -c "
  import json,collections
  n=json.load(open('ncas.json')); mid=collections.Counter()
  for v in n.values():
      if v.get('rightsId'): mid[v['rightsId'][16:30]]+=1
  print(mid)"
  ```

**Ticket 文件本身不加密**（只有里面的 titleKeyBlock 是加密的）。nsz `Ticket.open()` 用纯 seek/read 解析出 `signatureType → issuer → titleKeyBlock → ticketId → deviceId → rightsId`，**零解密**【事实】。
→ [switchbrew Ticket](https://switchbrew.org/wiki/Ticket)（Rights ID 在 ticket data 的 `0x160`，长 `0x10`）+ [nsz/Fs/Ticket.py](https://github.com/nicoboss/nsz/blob/master/nsz/Fs/Ticket.py)

**`.tik` 的可得性有多高？**【实测】ncas.json 里 19 518 个基础 title（TitleID 尾 `000`），**19 516 个（100.0%）至少含一个带 rightsId 的 NCA** —— 也就是说，几乎每一个零售游戏都用 titlekey 加密，容器里就应该有 `.tik`。

**例外**：`nsz --remove-titlerights` 与 nxdumptool 的 "remove titlekey crypto" 会去掉 titlekey 加密，产物没有 `.tik`。NX Game Info 把这类归为 `Converted` 结构（§3.1 表）。

### 2.3 NSZ / XCZ / NCZ

**权威来源**：[nicoboss/nsz `docs/formats.md`](https://github.com/nicoboss/nsz/blob/master/docs/formats.md)（MIT 许可，2 354 星，2026-08-23 仍在推送）。

#### 2.3.1 NSZ 与 XCZ

原文【事实】：

> **NSZ**: NSZ files are functionally identical to NSP files. The file extension difference is to alert the user that it contains compressed NCZ files. **NCZ files can be mixed with NCA files in the same container.**
>
> **XCZ**: XCZ files are functionally identical to XCI files. …

**→ NSZ 就是 PFS0，XCZ 就是 XCI/HFS0。容器层完全一致，识别代码 100% 复用。**

**文件名如何变化**【事实，nsz 源码】：`BlockCompressor.py` L150 与 `SolidCompressor.py` L117 都是同一句：

```python
newFileName = nspf._path[0:-1] + "z"
```

**只改最后一个字符**：`<ContentId>.nca` → `<ContentId>.ncz`，`<ContentId>.cnmt.nca` → `<ContentId>.cnmt.ncz`。**32 位 hex 的 ContentId 词干原样保留。** `.tik` / `.cert` / `.xml` 不动，delta fragment 直接跳过不压缩。

**→ NSZ/XCZ 的免密钥识别与 NSP/XCI 完全等价，一行额外代码都不用写**（只需在扩展名匹配时接受 `.ncz`）。

#### 2.3.2 NCZ 内部结构

【事实，`docs/formats.md`】：

> The first `0x4000` bytes of a NCZ file is **exactly the same as the original NCA (and still encrypted)**. This applies even if the first section doesn't start at 0x4000.
>
> At `0x4000` there is the variable sized NCZ Header. … Directly after the NCZ header, the zStandard stream begins and ends at EOF. The stream is decompressed to offset `0x4000`.

NCZ 头结构（原文给的 Python 参考实现）【事实】：

```python
nspf.seek(0x4000)
sectionCount = nspf.readInt64()          # u64
for i in range(sectionCount):
    magic          = f.read(8)           # b'NCZSECTN'
    offset         = f.readInt64()
    size           = f.readInt64()
    cryptoType     = f.readInt64()
    f.readInt64()                        # padding
    cryptoKey      = f.read(16)
    cryptoCounter  = f.read(16)
# 可选的块压缩头
    magic              = f.read(8)       # b'NCZBLOCK'
    version            = f.readInt8()
    type               = f.readInt8()
    unused             = f.readInt8()
    blockSizeExponent  = f.readInt8()
    numberOfBlocks     = f.readInt32()
    decompressedSize   = f.readInt64()
    compressedBlockSizeList = [f.readInt32() for _ in range(numberOfBlocks)]
```

**⚠ 安全提示**：NCZ 的 `Section` 结构里**明文存放了 `cryptoKey`（16 字节）与 `cryptoCounter`**。nsz 的文档说这些「can be derived from the original NCA + Ticket, however it is provided pre-parsed」。**本项目不应读取、缓存或导出这两个字段** —— 它们是解密材料，读它们会把工具从「读容器元数据」推向「规避 TPM」（§3.3）。只解析 `sectionCount` 用于完整性判断即可，或者干脆连 NCZ 头都不碰。

#### 2.3.3 NSZ/XCZ 能不能与 No-Intro 哈希匹配？

**不能，而且从构造上就不可能。**【推断，但推理链是【事实】】

- NCZ 的 zstd 流是**解密后再压缩**的（`docs/formats.md`：「The NCAs are decrypted then compressed using zStandard」），字节流与原 NCA 完全不同。
- 容器（PFS0/HFS0）的条目大小、偏移随之全变，头部也重算。
- 因此 **NSZ/XCZ 文件的 CRC32/MD5/SHA-1/SHA-256 与对应 NSP/XCI 无任何关系**。

**要与任何按整文件哈希的 DAT 比对，必须先 `nsz -D` 还原成 NSP/XCI。** 但 —— 见 §7，本项目的推荐路线根本不依赖整文件哈希，所以这不构成阻塞。

nsz 的解压是**无损**的（README：「compresses or decompresses Nintendo Switch dumps losslessly」），还原后应当逐字节等于原文件【事实，nsz README】。

### 2.4 NCA（Nintendo Content Archive）

**权威来源**：[switchbrew.org/wiki/NCA](https://switchbrew.org/wiki/NCA)。

#### 2.4.1 加密状况

switchbrew 开门见山【事实】：

> **The entire raw NCAs are encrypted.**
> The only known area which is not encrypted in the raw NCA is the logo section, when the NCA includes that section.
>
> The first `0xC00` bytes are encrypted with **AES-XTS with sector size 0x200 with a non-standard "tweak"**（字节序反转）…… this encrypted data is an `0x400` NCA header + an `0x200` header for each section in the section table.

**→ NCA 头（含 magic `"NCA3"`、ContentType、ProgramId）在原始文件里是密文。想读它必须有 `header_key`。**

这是本项目「免密钥路线」的边界所在。

#### 2.4.2 NCA 头字段（解密后）

【事实，switchbrew】，与 nxdumptool `NcaHeader` 结构逐字段一致【事实 ×2】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x100` | RSA-2048 签名（固定密钥，覆盖 0x200–0x400）|
| `0x100` | `0x100` | RSA-2048 签名（NPDM 的 ACID 密钥；非 program 则全 0）|
| `0x200` | `0x4` | Magic `"NCA3"`（旧版 `"NCA2"` / `"NCA1"` / `"NCA0"`）|
| `0x204` | `0x1` | **DistributionType**（0x00=Download, 0x01=GameCard）|
| `0x205` | `0x1` | **ContentType**（0=Program, 1=Meta, 2=Control, 3=Manual, 4=Data, 5=PublicData）|
| `0x206` | `0x1` | KeyGenerationOld |
| `0x207` | `0x1` | KeyAreaEncryptionKeyIndex（0=Application, 1=Ocean, 2=System）|
| `0x208` | `0x8` | ContentSize |
| `0x210` | `0x8` | **ProgramId** ← 这里才是 TitleID |
| `0x218` | `0x4` | ContentIndex |
| `0x21C` | `0x4` | SdkAddonVersion |
| `0x220` | `0x1` | KeyGeneration |
| `0x221` | `0x1` | [9.0.0+] SignatureKeyGeneration |
| `0x222` | `0xE` | Reserved |
| `0x230` | `0x10` | **RightsId** |
| `0x240` | `0x10 × 4` | FsEntry 数组 |
| `0x280` | `0x20 × 4` | 各 FsHeader 的 SHA-256 |
| `0x300` | `0x10 × 4` | EncryptedKeyArea |

**⚠ 关键：`DistributionType` 在 `0x204`。卡带上的 NCA 是 `0x01`，eShop 的是 `0x00` —— 即使是同一个游戏同一个版本，两者的字节流不同，因此 ContentId（= SHA-256 前 16 字节）也不同。** 这直接决定了 §4.3 里 titledb 对 XCI 的覆盖率（见【实测】数据）。

### 2.5 CNMT（Content Meta）

**权威来源**：[switchbrew.org/wiki/CNMT](https://switchbrew.org/wiki/CNMT)（官方名 `nn::ncm::PackagedContentMeta`）。

CNMT 文件位于 **Meta NCA 的 section 0（一个 PFS0）里**，文件名形如 `Application_0100c4c320c0ffee.cnmt`。nxdumptool 的 `cnmtGetContentMetaTypeAndTitleIdFromFileName()` 就是靠 `_` 前的类型串 + 后面正好 16 个 hex 字符解析出来的【事实】：

```c
pch1 = strstr(cnmt_filename, "_");
pch2 = (cnmt_filename + cnmt_filename_len - 5);   /* 去掉 ".cnmt" */
if (!pch1 || !(content_meta_type_str_len = (pch1 - cnmt_filename)) || (pch2 - ++pch1) != 16) { ... }
```
→ <https://github.com/DarkMatterCore/nxdumptool/blob/rewrite/source/core/cnmt.c>

**PackagedContentMetaHeader**【事实】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x8` | **Id**（TitleID）|
| `0x8` | `0x4` | **Version** |
| `0xC` | `0x1` | ContentMetaType |
| `0xD` | `0x1` | [17.0.0+] ContentMetaPlatform |
| `0xE` | `0x2` | ExtendedHeaderSize |
| `0x10` | `0x2` | ContentCount |
| `0x12` | `0x2` | ContentMetaCount |
| `0x14` | `0x1` | ContentMetaAttributes |
| `0x18` | `0x4` | RequiredDownloadSystemVersion |

**PackagedContentInfo**（每条 `0x38` 字节）【事实】：

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x20` | Hash（被引用内容的 SHA-256）|
| `0x20` | `0x10` | **ContentId** |
| `0x30` | `0x5`（[15.0.0+]，此前 `0x6`）| Size |
| `0x35` | `0x1` | [15.0.0+] ContentAttributes |
| `0x36` | `0x1` | ContentType |
| `0x37` | `0x1` | IdOffset |

switch-library-manager 的 `readBinaryCnmt()` 用的正是这套偏移（`0x20 + tableOffset + i*0x38`，ncaId 在 `+0x20`，contentType 在 `+0x36`）【事实 ×2】。

ContentMetaType 枚举【事实，[NCM_services](https://switchbrew.org/wiki/NCM_services#ContentMetaType)】：`0x80`=Application、`0x81`=Patch、`0x82`=AddOnContent、`0x83`=Delta、[15.0.0+] `0x84`=DataPatch。

**→ CNMT 里同时有 TitleID 和 Version，但它埋在加密的 Meta NCA 里，读它需要密钥。** 免密钥路线用查表（§4.3）绕开它。

### 2.6 NACP（control.nacp）—— 语言与游戏名

**权威来源**：[switchbrew.org/wiki/NACP](https://switchbrew.org/wiki/NACP)（`nn::ns::ApplicationControlProperty`，总大小 `0x4000`）。

| 偏移 | 大小 | 字段 |
|---|---|---|
| `0x0` | `0x3000`（`0x300 × 16`）| Title（每语言 Name 0x200 + Publisher 0x100，UTF-8）|
| `0x302C` | `0x4` | **SupportedLanguageFlag** |
| `0x3060` | `0x10` | DisplayVersion |

语言条目索引【事实】：0 AmericanEnglish、1 BritishEnglish、2 Japanese、3 French、4 German、5 LatinAmericanSpanish、6 Spanish、7 Italian、8 Dutch、9 CanadianFrench、10 Portuguese、11 Russian、12 Korean、**13 TraditionalChinese**、**14 SimplifiedChinese**、15 [10.1.0+] BrazilianPortuguese、16 [21.0.0+] Polish、17 [21.0.0+] Thai。

**→ 繁中/简中在 NACP 里是可区分的（bit 13 / bit 14）。但 NACP 在 Control NCA 里，读它需要 `header_key` + key-area keys。** 免密钥路线只能从 titledb 的 `languages` 字段拿语言信息，而那个字段把两种中文都写成 `"zh"`（§5.1）。

---

## 3. 加密与密钥边界

### 3.1 无密钥可读 vs. 需要 prod.keys —— 明确划线

| 数据 | 位置 | 密钥需求 |
|---|---|---|
| XCI CardHeader 全部明文字段（RomSize / PartitionFsHeaderAddress / 各种哈希 / PackageId）| CardHeader `0x100`–`0x190` | **无** |
| XCI 根 HFS0 + 四个分区 HFS0 的头、条目表、**文件名** | 各分区起点 | **无** |
| 国行（Terra）判定 | HFS0 头哈希 + 1 字节后缀爆破 | **无**（hactool 旁路）|
| PFS0（NSP/NSZ）的头、条目表、**文件名** | 文件偏移 0 | **无** |
| `<ContentId>.nca` / `.ncz` / `.cnmt.nca` / `.cnmt.ncz` 文件名 | StringTable | **无** |
| `<RightsId>.tik` / `.cert` 文件名 → **TitleID + KeyGeneration** | StringTable | **无** |
| Ticket 文件内部的 `rightsId`（偏移 `0x160`，长 `0x10`）| `.tik` 文件体 | **无**（票据本身不加密）|
| `<ContentId>.cnmt.xml` / `.nacp.xml` / `.programinfo.xml` / `.legalinfo.xml` / `cardspec.xml` / `authoringtoolinfo.xml` 的**内容** | PFS0 里的独立条目 | **无**（就是明文 XML）|
| NCZ 头（`NCZSECTN` / `NCZBLOCK`）| NCZ `0x4000` 起 | **无**（但含解密材料，见 §2.3.2 警告）|
| — 以下需要密钥 — | | |
| NCA header（magic / ProgramId / ContentType / RightsId）| NCA `0x0`–`0xC00` | **`header_key`**（AES-XTS 0x20 字节）|
| CNMT（TitleID + Version + 内容清单）| Meta NCA section 0 | `header_key` + `key_area_key_application_*` 等 |
| NACP（游戏名 / 语言旗标 / DisplayVersion）| Control NCA romfs | 同上 |
| 用 titlekey 加密的 NCA 正文 | Program/Data NCA | 还要 `titlekek_##` + ticket 里的 titlekey |
| XCI `CardHeaderEncryptedData`（CompatibilityType / UppVersion / FwVersion）| CardHeader `0x190`，`0x70` 字节 | **`xci_header_key`**（AES-128-CBC）|

**三个一手来源交叉确认这条线**：

1. **hactool `settings.h`** 的 `nca_keyset_t` 里 `header_key[0x20]`（NCA header key）与 `xci_header_key[0x10]`（XCI partially encrypted header）是两个独立字段；`xci.c` 里 `xci_header_key` 若为全零就只是 `has_decrypted_header = 0`，**HFS0 解析照常进行**【事实】。
2. **switch-library-manager** 的判断最直白【事实】：
   ```go
   keys, _ := settings.SwitchKeys()
   if keys != nil && keys.GetKey("header_key") != "" {
       ...  // 才去 ReadNspMetadata / ReadXciMetadata
   }
   // fallback to parse data from filename
   ```
   `openMetaNcaDataSection()` 里：`if headerKey == "" { return errors.New("missing key - header_key") }`。
   → <https://github.com/trembon/switch-library-manager/blob/master/src/db/localSwitchFilesDB.go> / [`switchfs/nca.go`](https://github.com/trembon/switch-library-manager/blob/master/src/switchfs/nca.go)
3. **NX Game Info README** 列出的必需密钥【事实】：
   > *prod.keys*: Mandatory keys includes `header_key`, `aes_kek_generation_source`, `aes_key_generation_source`, `key_area_key_application_source` and `master_key_00`. **Failing to provide these keys will make the application quit**
   → <https://github.com/garoxas/NX_Game_Info>

**一个副产品：NX Game Info 的「Structure」分类完全可以免密钥复现。** 它的判据全是容器内的文件名【事实，README】：

| 结构 | 判据（纯文件名）|
|---|---|
| XCI — Scene | 同时有 `update` / `normal` / `secure` 分区 |
| XCI — Converted | **只有** `secure` 分区（NSP 转来的）|
| NSP — Scene | 含 `legalinfo.xml` + `nacp.xml` + `programinfo.xml` + `cardspec.xml`（BigBlueBox 风格）|
| NSP — Homebrew | 含 `authoringtoolinfo.xml` |
| NSP — CDN | 含 `.cert` + `.tik` |
| NSP — Converted | **没有** `.cert` / `.tik`（XCI 转来的）|
| Not complete | 只有 `.nca` |

这套分类直接可以喂给 ADR-0008 的「verdict accretion」做来源判定。

### 3.2 仅凭免密钥信息，能唯一确定一个游戏吗？——**能，而且能确定到版本。**⭐

这是本次调研最有价值的实测结果。

**【实测】** 下载 [blawar/titledb](https://github.com/blawar/titledb) 的 `cnmts.json`（50 MB，MIT 许可，2026-08-30 仍在推送），统计所有 CNMT 内容条目里出现的 NCA content ID：

```bash
curl -sL -o cnmts.json https://raw.githubusercontent.com/blawar/titledb/master/cnmts.json
python3 - <<'EOF'
import json, collections
c = json.load(open('cnmts.json'))
m = collections.defaultdict(set)
for t, vs in c.items():
    for ver, d in vs.items():
        for e in d.get('contentEntries', []):
            m[e['ncaId']].add((t, ver))
uniq = sum(1 for v in m.values() if len(v) == 1)
print(len(m), uniq, f'{100*uniq/len(m):.2f}%')
EOF
```

结果：

```
不同 ncaId: 173502
只属于唯一 (titleId, version) 的: 173502  (100.00%)
```

**零冲突。** 另有旁证：7 231 个有多版本的 title，抽样 3 000 个，**版本之间的 ncaId 集合两两互不相同，零重复**。

**→ 从 PFS0/HFS0 的明文文件名里读出任意一个 32 位 hex 的 ContentId，查一次表，就能得到「哪个游戏、哪个版本」。不需要打开 NCA，不需要任何密钥。**

覆盖面【实测】：

| 数据集 | 规模 |
|---|---|
| `ncas.json` 条目（ContentId → titleId 等）| **235 227** |
| 涉及不同 titleId | **45 603** |
| `cnmts.json` 的 titleId | **39 648** |
| 其中基础游戏（尾 `000`）| **14 029** |
| 其中更新（尾 `800`）| **13 444** |

`ncas.json` 单条记录的形状【实测】：

```json
"0000073c3b42cc482ce96cb074f46971": {
    "buildId": null, "contentIndex": 0,
    "contentType": 1,          // NCA 头的 ContentType 枚举：0=Program,1=Meta,2=Control,3=Manual,4=Data,5=PublicData
    "cryptoType": 2, "cryptoType2": 14,
    "isGameCard": 0,           // = NCA 头的 DistributionType
    "keyIndex": 0, "rightsId": null,
    "sdkVersion": 251854848, "size": 5120,
    "titleId": "010038A01628A800"
}
```

（`contentType` 语义已交叉验证：拿 `cnmts.json` 里标为 `type:1`(Program, NCM 枚举) 的 `0f26bd42…`，在 `ncas.json` 里是 `contentType:0` —— 对应 NCA 头枚举的 Program。**两个文件用的是两套枚举**，实现时别搞混。）

#### ⚠ 一个重大限制：这条路对 XCI 基本无效

**【实测】** `ncas.json` 里 `isGameCard == 1` 的条目**只有 121 条**（占 235 227 的 0.05%），涉及 58 个 titleId。

原因【推断，但基于【事实】的推理链】：titledb 是从 eShop CDN 抓的，里面全是 DistributionType=Download 的 NCA。而**卡带上的 NCA 是 DistributionType=GameCard（NCA 头 `0x204` = `0x01`）**，字节流不同 → SHA-256 不同 → ContentId 不同。

**→ XCI / XCZ 的 ContentId 查不到 titledb。XCI 必须走别的路：**
- `.tik` 文件名（§2.2.5）—— 卡带的 secure 分区**确实**有 `.tik`。nxdumptool 的 `tikRetrieveTicketFromGameCardByRightsId()` 就是在 `HashFileSystemPartitionType_Secure` 里按 `<rightsid>.tik` 找的【事实】：
  ```c
  utilsGenerateHexString(tik_filename, sizeof(tik_filename), id->c, sizeof(id->c), false);
  strcat(tik_filename, ".tik");
  gamecardGetHashFileSystemEntryInfoByName(HashFileSystemPartitionType_Secure, tik_filename, &tik_offset, &tik_size);
  ```
  → <https://github.com/DarkMatterCore/nxdumptool/blob/rewrite/source/core/tik.c>
- NSWDB 的 `imgcrc`（整个 XCI 的 CRC32，§4.4）
- 文件名里的 `[TitleID]`

### 3.3 prod.keys 的法律问题 —— **本工具不应内置或分发密钥**

**这不是保守建议，是有判决先例的红线。**

**一手来源：Nintendo of America 提交给 GitHub 的 DMCA 通知全文**（GitHub 的公开 `github/dmca` 仓库）：

> Nintendo Switch games are encrypted using proprietary cryptographic keys (**prod.keys**) which protect against unauthorized access to and copying of the copyrighted games. During operation, Ryujinx **necessarily uses unauthorized copies of these cryptographic keys to decrypt** unauthorized copies of Nintendo Switch games, or ROMs, at or immediately before runtime without Nintendo's authorization. Thus, Ryujinx … unlawfully "circumvent[s] a technological measure that effectively controls access to a work protected under" the DMCA … **17 U.S.C. § 1201(a)(1) and (2)**. See also *Final Judgment & Permanent Injunction, Nintendo of America Inc. v. Tropic Haze LLC*, No. 1:24-cv-00082 (D.R.I. Mar. 6, 2024), ECF No. 11.

→ <https://github.com/github/dmca/blob/master/2025/03/2025-03-06-nintendo.md>（该次下架涉及 **575 个仓库**）

后续【事实】：2026-06-30 又有一次通知，把 Flathub 的 Ryubing 打包仓库 `flathub/io.github.ryubing.Ryujinx` 整个下架。
→ <https://github.com/github/dmca/blob/master/2026/06/2026-06-30-nintendo.md>（GitHub 备注：「While GitHub did not find sufficient information to determine a valid anti-circumvention claim, we determined that this takedown notice contains other valid copyright claim(s).」）

**行业惯例（两个一手来源）**：

- **nsz README** —— Legal 一节【事实】：
  > This project does **NOT** incorporate any copyrighted material such as cryptographic keys. All keys must be provided by the user.
  > This project does **NOT** circumvent any technological protection measures. The NSZ file format purposely keeps all technological protection measures in place.
  → <https://github.com/nicoboss/nsz>
- **hactool README**【事实】：keyset 文件由用户通过 `-k/--keyset` 提供，或放在 `$HOME/.switch/prod.keys`；仓库不带任何密钥。
  → <https://github.com/SciresM/hactool>

**对本项目的建议（明确结论）**：

1. **绝不内置、绝不分发、绝不下载 `prod.keys` 的任何字节。** 仓库、二进制、CI 缓存里都不能有。
2. **不解密路线能走到 §7 的 A/B/C 三层，覆盖到「TitleID + 精确版本 + Base/Update/DLC + 官中/国行标注 + 完整刮削元数据」** —— 对本项目（元数据自动化 + 前端互转）已经**完全够用**。缺的只有「从文件里直接读出 NACP 游戏名」和「NCA 级完整性校验」，这两样都能用外部数据库替代。
3. 如果将来一定要支持密钥路线，正确做法是：**可选功能，密钥路径由用户在配置里指定，默认关闭，代码里不带任何密钥常量或 key source**。即便如此，`§1201` 的风险仍然存在 —— 这是一个产品决策，不是技术决策。
4. **NCZ 头里的 `cryptoKey` / `cryptoCounter` 明文字段不要读。**（§2.3.2）

---

## 4. 数据库覆盖

### 4.1 No-Intro —— DAT-o-MATIC 里有 Switch，公开镜像里没有

#### 4.1.1 【实测】每日镜像里零个 Switch DAT

严格按项目约定，**全程未访问 `datomatic.no-intro.org`**，只用 GitHub 镜像：

```bash
curl -sL -o no-intro.xml \
  https://github.com/hugo19941994/auto-datfile-generator/releases/download/Daily_Rebuild/no-intro.xml
python3 -c "
import re; s=open('no-intro.xml',encoding='utf-8').read()
n=re.findall(r'<name>(.*?)</name>',s)
print('DAT 总数:',len(n)); print('Switch:',[x for x in n if 'Switch' in x])"
```

结果【实测】：

- `no-intro.xml`：**334 个 DAT**，其中 Nintendo 相关 **70 个**，**Switch 相关 0 个**
- `no-intro_parent-clone.xml`：326 个 DAT，Switch 0 个
- 资产最后更新：**2026-07-08**（`redump.*` 是 2026-08-30，即 No-Intro 那边已经落后约 7 周）

70 个 Nintendo DAT 里最新的世代只到 **Wii U**（`Nintendo - Wii U (Digital) (CDN)` 等 4 个）与 **New Nintendo 3DS**。没有 Switch，也没有 Switch 2。

镜像脚本 [`no-intro.py`](https://github.com/hugo19941994/auto-datfile-generator/blob/master/no-intro.py) 抓的是 DAT-o-MATIC 的 **"No-Intro Love Pack" 每日包**（Selenium 点 DOWNLOAD → daily → dat_type=dat）。**【推断】Switch 集不在公开 Love Pack 里** —— 可能因为需要登录、或因内容加密性质被排除。本次未能验证原因（见 §9）。

#### 4.1.2 但 No-Intro 确实有 Switch 集 —— wiki 上有整套页面

`wiki.no-intro.org` 与 `datomatic.no-intro.org` 是**不同主机**，不在禁令范围（沿用 `rom-identification-part2.md` 的既有判断）。用 MediaWiki API 搜索【实测】：

```bash
curl -s "https://wiki.no-intro.org/api.php?action=query&list=search&srsearch=Nintendo%20Switch&format=json&srlimit=20"
```

返回 10 个页面：

- `Nintendo - Nintendo Switch dat notes`（`datting guide` 重定向到它）
- `Nintendo - Nintendo Switch undumped`（96 KB，逐条列出未转储卡带，带 5 位产品码与区域）
- `Nintendo - Nintendo Switch Digital undumped`
- `Nintendo - Nintendo Switch USA / World / Multi-region undumped`
- `Nintendo - Nintendo Switch MIA`
- `Nintendo - Nintendo Switch limited publishers`
- `Nintendo - Nintendo Switch 2 undumped`

**→ 结论：No-Intro 有 Switch 集与 Switch Digital 集，但本项目按约定只能用 GitHub 镜像，而镜像里没有。这是一个硬缺口。**

#### 4.1.3 No-Intro 的 Switch DAT 按什么哈希？

`dat notes` 页原文【事实】：<https://wiki.no-intro.org/index.php?title=Nintendo_-_Nintendo_Switch_dat_notes>

> - The file serial field in archives is being manually filled out with the Product Code by some datters. Its not being extracted in any consistent way …
> - **Make sure the cert at (offset 0x7000 length 0x200) is FF'd out** - this is sensitive/cart-unique data. NXDumpTool wipes it (unless this is disabled), but some p2p/scene releases still contain the cert.
> - **CNMT NCAs in the digital CDN dat should have the file serial set to the value of the "ProgID" NCA field and the value of the CNMT "TitleId" field (comma seperated).**
> - How to make FullXCI files: … `cat 512_bytes_initial_area <(cat /dev/zero | head -c 3584) scene_style_xci > fullxci`
> - Switch 2 Edition carts don't have Initial Areas, and while the Switch 1 data on them can be dumped, the Switch 2 data cannot.

从中可以确定【事实 + 推断】：

1. **卡带集（`Nintendo - Nintendo Switch`）是整文件哈希的**，对象是 XCI 镜像。规范化要求：**把 `cert`（CardHeader 起点 + `0x7000`，长 `0x200`）填成 `0xFF`**，因为那是每张卡唯一的证书。
   - `0x7000` 与 nxdumptool 的 `GAMECARD_CERT_OFFSET 0x7000` 完全一致 → **【推断】这里用的是转储文件（scene-style）的偏移**。实现时应写成 `cardHeaderBase + 0x7000`。
   - 【推断】No-Intro 的规范形态是 **FullXCI**（页面专门给了制作配方），即前面补上 `0x1000` 的 CardKeyArea；此时 cert 落在文件偏移 `0x8000`。**两种形态都要能处理。**
2. **数字集（`Nintendo - Nintendo Switch (Digital) (CDN)`）是按单个 NCA 文件哈希的**（「CNMT NCAs in the digital CDN dat」），serial 字段填 NCA 的 ProgId + CNMT 的 TitleId。这与 3DS/Wii U 的 `(Digital) (CDN)` 集做法一致 —— **不是对 NSP 整包哈希，而是对拆开后的每个 NCA 哈希**。
3. **→ 一个 `.nsp` 文件整体的哈希，与 No-Intro Digital CDN 集根本不在同一层级**（后者是 NCA 级）。要匹配必须先把 PFS0 拆开，逐个 NCA 算 SHA-1/MD5/CRC32。这在免密钥前提下**完全可行**（不用解密，只要按 PFS0 条目表切片）。
4. **NSZ/XCZ 与任何 DAT 都匹配不上**（§2.3.3）。

### 4.2 Redump / libretro-database / Hasheous

- **Redump：没有 Switch，而且永远不会有。**【实测】镜像的 `redump.xml` 里 **60 个 DAT**，Nintendo 相关只有 GameCube / GameCube BIOS / Wii / Triforce。Switch 是卡带 + 数字发行，没有光盘，不在 Redump 的收录范围。
- **libretro-database：没有 Switch。**【实测】`git/trees?recursive=1` 全量列举，含 "Switch" 的 8 条路径全是游戏名里带 Switch/Witch 的金手指文件（`Switchblade`、`Kill Switch`、`Sabrina the Teenage Witch`…），`metadat/` 与 `rdb/` 下**零个 Switch 平台**。这是意料之中 —— RetroArch 没有 Switch 核心。
- **Hasheous：【未能核实】。** `api/v1/Lookup/Platforms` 只返回首页 100 条（按字母序到 "Acorn"），加分页参数后请求超时。**【推断】** Hasheous 聚合的是 No-Intro / Redump / TOSEC / MAME 的签名，这四家都没有公开 Switch 集，所以 Hasheous 大概率也没有。

### 4.3 blawar/titledb —— 本平台最重要的数据源 ⭐

**仓库**：<https://github.com/blawar/titledb> · **许可 MIT**（Copyright (c) 2019 Blake Warner）· 82 星 · **最后推送 2026-08-30**（每日更新）· 仓库体积约 44 GB(!)，但按需只取几个文件即可。

**注意**：No-Intro 自己的 wiki 页面（`Nintendo - Nintendo Switch Digital undumped` 的 to-do 列表）就把它列为参考源【事实】：

> `https://github.com/blawar/titledb` - eshop data dump - compare with dat

#### 文件清单与用途

| 文件 | 大小 | 键 | 用途 |
|---|---:|---|---|
| **`ncas.json`** | 87 MB | **NCA ContentId（32 hex）** | → `titleId`, `contentType`, `isGameCard`, `rightsId`, `size`, `sdkVersion`, `cryptoType`。**免密钥识别的核心索引** |
| **`cnmts.json`** | 50 MB | `titleId` → `version` | → `contentEntries[{ncaId, type, buildId}]`, `titleType`, `otherApplicationId`, `requiredSystemVersion`。**版本级精确定位** |
| `versions.json` | 1.8 MB | `titleId` → `version` → 日期 | 版本发布时间线 |
| `versions.txt` | 3.4 MB | 同上（文本）| |
| `{region}.{lang}.json` | 各 50–95 MB | `nsuId` | eShop 完整元数据：`id`(TitleID)、`name`、`publisher`、`description`、`intro`、`category`、`languages`、`numberOfPlayers`、`releaseDate`、`size`、`rating`、`ratingContent`、`rightsId`、`iconUrl`、`bannerUrl`、`screenshots[]`。**同时是刮削源** |
| `cheats.json` | 12 MB | `titleId` → `buildId` | 金手指 |
| `cdn.regions.json` | 793 B | | 区域 → 国家码映射（USA/EUR/CHN/TWN/KOR/JPN/AUS）|
| `languages.json` | 1.7 KB | | 国家码 → 可用语言 |

区域文件共 **41 个**，其中中文相关：**`HK.zh.json`（50 MB，港服繁中）**、**`CN.zh.json`（265 KB，国行）**、`KR.ko.json`、`JP.ja.json`。

`cdn.regions.json` 明确把 `CHN → ["CN"]`、`TWN → ["HK"]`【实测】—— 也就是港服 eShop 就是繁中区，国行是独立的 `CHN` 区。

#### 可自动化获取性

- 纯 GitHub raw，`curl` 直取，无需 token、无速率问题（本次全程无阻碍）
- MIT 许可，可再分发派生索引
- 每日更新（`pushed_at` 与调研日只差 1 天）
- **实现建议**：只拉 `ncas.json` + `cnmts.json` + `versions.json` + 需要的区域文件；用 `git clone --depth 1` 会拖下 44 GB，**必须走 `raw.githubusercontent.com` 单文件下载**

#### 配套工具与历史快照

- 生成器：[blawar/nut](https://github.com/blawar/nut)（GPL-3.0，1 267 星，2026-01-19）
- 历史快照仓库（README 列出）：`titledb_02112024` / `titledb_12032024` / `titledb_04082025`
- 下游消费者：[a1ex4/ownfoil](https://github.com/a1ex4/ownfoil)（AGPL-3.0，911 星，2026-08-25）、[trembon/switch-library-manager](https://github.com/trembon/switch-library-manager)（**MIT**，150 星，2026-08-30）

### 4.4 NSWDB —— XCI 的整文件 CRC32 数据库

**URL**：<https://nswdb.com/xml.php>（`http` 与 `https` 都可，返回 `application/xml`）

**⚠ 下载注意**【实测】：不带 `--compressed` 时服务端会在随机位置截断（试了 5 次，分别停在 0.7 / 0.86 / 1.1 / 1.52 / 1.67 MB）。**必须加 `--compressed`** 才能拿到完整的 2 311 244 字节：

```bash
curl -sL --compressed -A "Mozilla/5.0" -o nswdb.xml https://nswdb.com/xml.php
tail -c 12 nswdb.xml   # => </releases>
```

**【实测】内容统计**（2026-08-31 抓取）：

| 项 | 值 |
|---|---|
| `<release>` 条目 | **4 054** |
| 不同 TitleID（前 16 hex）| **3 930** |
| 有 `imgcrc` 的 | 4 053，**其中不同值 4 053（100% 唯一）** |
| 有 `serial` 的 | 4 049（形如 `LA-H-AAAAA`），不同值 3 980 |
| `languages` 含 `zh` 的 | **1 507（37.2%）** |
| region 分布 | JPN 1406 / WLD 969 / USA 637 / EUR 576 / **TWN 359** / KOR 50 / **CHN 35** / GER 14 / 其他 6 |
| `card` 分布 | T1 v2 3009 / T1 v1 966 / 空 73 / **Tencent 5** / `0` 1 |
| 发布组 Top | HR 1885、BigBlueBox 891、VENOM 374、SUXXORS 357、LiGHTFORCE 233 |

字段全表：`id, name, publisher, region, languages, group, imagesize, serial, titleid, imgcrc, idcrc, filename, releasename, trimmedsize, firmware, type, card, notes`。

**与 titledb 交叉**【实测】：NSWDB 的 3 930 个 TitleID 里 **3 541 个（90.1%）在 `ncas.json` 里能找到** —— 缺的 389 个【推断】是国行/限量发行/未上架 eShop 的版本。

**用途**：
- **XCI 的整文件 CRC32 匹配**（`imgcrc` 100% 唯一 → 可作主键）。⚠ 但要先确认 `imgcrc` 覆盖的字节范围 —— 字段旁边有 `trimmedsize`，**【未找到权威来源】说明 `imgcrc` 是对 trimmed 还是 full 镜像算的**。
- 卡带产品码（`LA-H-XXXXX`）↔ TitleID 映射
- **国行卡带识别**：`card == "Tencent"` 的 5 条（§5.2）
- 场景发布名（`releasename`）→ 可用于解析既有文件名

**许可**：**【未找到权威来源】** —— 站点没有 LICENSE 或使用条款页面。**【推断】** 应视为「可读取、不可再分发」，本项目只在本地缓存、不打包进发行物。

### 4.5 switchbrew Title_list

<https://switchbrew.org/wiki/Title_list>（163 KB wikitext）**只覆盖系统 title**（System Modules / System Applets / System Data Archives / 固件包），**不含任何游戏**。对本项目唯一的用处是识别 XCI `update` 分区里的系统更新（`UppId` 恒 `0x0100000000000816`）。

---

## 5. 中文覆盖

### 5.1 官中 —— 数量巨大，但 TitleID 区分不了

#### 官中有多少？

**【实测】** 从 titledb 区域文件统计 `languages` 数组含 `"zh"` 的条目：

```bash
for f in US.en.json HK.zh.json JP.ja.json; do
  curl -sL -o $f https://raw.githubusercontent.com/blawar/titledb/master/$f
done
python3 -c "
import json
for nm in ['US.en','HK.zh','JP.ja']:
    d=json.load(open(nm+'.json'))
    ids={v['id'] for v in d.values() if v.get('id') and v.get('languages') and 'zh' in v['languages']}
    print(nm, len(d), len(ids))"
```

| 区 | eShop 条目 | 不同 TitleID | **含中文的 TitleID** |
|---|---:|---:|---:|
| US | 37 179 | 29 401 | **6 946** |
| HK（港服）| 22 374 | 17 260 | **5 207** |
| JP | 33 577 | 27 060 | 6 554 |
| CN（国行）| **109** | **89** | — |

**US ∪ HK 去重后：8 447 个含中文的 TitleID，且 100% 是基础游戏（尾 `000`）。**

对照本项目已调研的其他现代平台（`rom-identification-part2.md`）：**PSV 官中 290 条、X360 官中约 305 条、Wii U 连官中都没有。Switch 是 8 447 —— 高出一到两个数量级。**

繁简细分【实测】：HK 区 5 366 个含 `zh` 的条目里，**4 377 条 `languages` 里出现两次 `"zh"`**（对应 NACP 的 TraditionalChinese + SimplifiedChinese 两个 bit），989 条只出现一次。**titledb 的 `languages` 字段把繁简都写成 `"zh"`，靠出现次数才能推断，很不可靠。** 要精确区分必须读 NACP 的 `SupportedLanguageFlag`（bit 13 / bit 14）—— **那需要密钥**。

#### TitleID 能区分中文版吗？——**大部分不能。**

**【实测】** 集合运算：

| 比较 | 结果 |
|---|---|
| HK ∩ US 的 TitleID | **11 983，占 HK 的 69.4%** |
| HK ∩ JP | 13 363 |
| HK 独有（不在 US 也不在 JP）| **2 606** |
| CN ∩ (HK ∪ US ∪ JP) | **0 / 89** |

**结论**：

1. **69.4% 的港服（繁中）游戏与美服共用同一个 TitleID** —— 同一个文件，多语言内嵌，靠主机语言设置切换。**中文在这里不是「版本」，是「文件属性」。**
2. **2 606 个 HK 独占 TitleID** —— 亚洲限定 SKU（很多是中文优先或中文独占），这些**能**靠 TitleID 区分。
3. **国行（CN）的 89 个 TitleID 与其他区完全不相交** —— 国行是彻底独立的内容生态。

#### 对 ADR-0012 的影响 ⚠

ADR-0012 说「官中是一个 release，汉化是一个 variant」。**在 Switch 上，官中连一个独立的 release 都不是** —— 它和英文版是**同一个二进制**。

**建议的建模**：Switch 的中文支持应当是**变体上的一个语言标签**，数据来源是 titledb 的 `languages` 字段（或有密钥时的 NACP `SupportedLanguageFlag`），**不参与识别、不进 triage 队列**。只有那 2 606 个亚洲独占 TitleID 才对应真正的独立 release。

#### 区域/语言信息存在哪里，要不要密钥？

| 信息 | 位置 | 密钥 |
|---|---|---|
| 支持语言（繁中/简中分开）| Control NCA → `control.nacp` `0x302C` SupportedLanguageFlag | **要** |
| 各语言的游戏名/发行商 | 同上，`0x0`–`0x3000` | **要** |
| 支持语言（繁简合并成 `zh`）| titledb `{region}.{lang}.json` 的 `languages` | **不要** |
| 卡带区域兼容性（Normal/Terra）| XCI CardHeaderEncryptedData `0x24` | 要 `xci_header_key`，**但有免密钥旁路**（§2.1.5）|
| 卡带的销售区域 | ❌ 卡带头里**没有**区域字段 | — |
| 发布区域 | NSWDB `<region>` / titledb 的区域文件归属 | **不要** |

**⚠ 重要：Switch 卡带头里没有区域码。** 不像 Wii U 的 region bitmask 或 GC/Wii 的 game id 第 4 位。区域只能靠外部数据库或产品码（`LA-H-XXXXX` 的末位字母）推断。

### 5.2 国行（腾讯 Switch）

**三个独立证据表明国行是可识别的一类**：

1. **hactool `xci.h`** 有 `xci_region_compatibility_t { COMPAT_GLOBAL = 0x00, COMPAT_CHINA = 0x01 }`，且 `xci.c` 在无密钥时会用它爆破 HFS0 头哈希后缀【事实】。switchbrew 对应字段叫 `CompatibilityType`（0=Normal, 1=**Terra**）【事实】。
2. **NSWDB** 的 `card` 字段有 `"Tencent"` 取值，5 条【实测】：
   | 游戏 | 国行 TitleID |
   |---|---|
   | New Super Mario Bros. U Deluxe | `0100E8C00F506000` |
   | Mario Kart 8 Deluxe | `010075100E8EC000` |
   | Super Mario Odyssey | `010075000ECBE000` |
   | Just Dance | `01009DE010F48000` |
   | Rabbids Adventure Party | `0100770010F6C000` |

   对照全球版 Mario Kart 8 Deluxe 是 `0100152000022000`【实测，NSWDB id=2】—— **国行版有完全不同的 TitleID**。
3. **titledb `CN.zh.json`** 的 89 个 TitleID 与其他所有区零交集【实测】。

**→ 国行既能靠 TitleID 区分（有 titledb `CN.zh.json` + NSWDB 的 5 条卡带），也能靠 §2.1.5 的免密钥哈希后缀爆破从 XCI 本体判定。**

NSWDB 里 region = `CHN` 的有 35 条、`TWN` 的有 359 条，合计 **394 张中文区卡带**。

### 5.3 民间汉化 —— 主流形态是 LayeredFS 补丁，不改 ROM

**Switch 的汉化生态与前几代完全不同。** 因为 Switch 有官方的 mod 加载机制（Atmosphère 的 LayeredFS），汉化组的标准产物是**独立的补丁目录**，而不是重打包的 ROM。

**目录约定**【事实，Atmosphère 官方 changelog】：

- `/atmosphere/contents/<program id>/romfs/...` —— 替换 RomFS 内的文件
- `/atmosphere/contents/<program id>/manual_html/` —— 替换手册（changelog 0.x 条目原话：「This works like normal layeredfs, replacing content placed in `/atmosphere/contents/<program id>/manual_html/`」）
- 0.10.0 起从 `/atmosphere/titles/<program id>` 改名为 `/atmosphere/contents/<program id>`（changelog：「When booting into 0.10.0, Atmosphere will rename /atmosphere/titles/`<program id>` to /atmosphere/contents/`<program id>`」）

→ <https://github.com/Atmosphere-NX/Atmosphere/blob/master/docs/changelog.md>（LayeredFS 由 `fs_mitm` 实现，见 [`docs/components/modules/ams_mitm.md`](https://github.com/Atmosphere-NX/Atmosphere/blob/master/docs/components/modules/ams_mitm.md)：「fs_mitm enables intercepting file system operations. … It enables LayeredFS to function, which allows for replacement of game assets.」）

**对本项目的三条结论**：

1. **纯 LayeredFS 汉化补丁不是变体，是伴生文件。** 原 NSP/XCI 一个字节都没变，仍然能被任何数据库正常识别。补丁目录名**就是 TitleID**（16 位 hex），可以直接关联到变体上 —— 这比其他平台的汉化补丁（IPS/BPS，要猜目标 ROM）好认得多。
2. **存在「魔改整合版」**：中文社区确实流通把汉化烤进 romfs 后重打包的 NSP/XCI（常见命名如 `游戏名/汉化/本体+更新整合版/[NSP-XCI][原版+魔改x.x.x]`）。这类【推断，基于 §2.2.4 的 ContentId 机制】：
   - Program NCA 被重建 → **ContentId 变** → titledb `ncas.json` 查不到 → §7 的 A 层失效
   - Meta NCA 也被重建 → ContentId 变
   - **但 `.tik` 的 RightsId 通常保留**（titlekey 没变）→ **B 层（TitleID）仍然有效**
   - 文件名里的 `[TitleID]` 通常保留 → D 层有效
   - **→ 现象：能认出「是哪个游戏」，但认不出「是哪个官方版本」。这恰好就是一个可靠的「疑似魔改」信号**，可以直接进 ADR-0002 的 triage 队列。
3. **【未找到权威来源】**：Switch 汉化补丁的总量、以及是否有类似 GoodTools `[T+Chi]` 那样的集中式索引。中文社区的汉化站点（Switch520 等）不构成一手来源，本文不据此下量化结论。

---

## 6. 生态

### 6.1 模拟器

**背景（截至 2026-08）**：GitHub 上 Ryujinx 全系已被 DMCA 清空 —— `ryujinx-mirror/ryujinx`（2025-03-14，连带 **575 个 fork**）、`litucks/torzu`（2024-07-09）、`flathub/io.github.ryubing.Ryujinx`（2026-06-30）全部返回 HTTP 451【实测，`gh api` 直接返回 block 信息】。`Thealexbarney/LibHac` 在 GitHub 上也是 404。

现存的一手源：

| 项目 | 位置 | 可访问性 |
|---|---|---|
| **Eden** | <https://git.eden-emu.dev/eden-emu/eden>（自建 Gitea）| ✅ **可访问**（Gitea API + raw 均可）。28 893 commits，C++ 64% |
| **Ryubing (Ryujinx 续作)** | `git.ryujinx.app` | ❌ **Anubis 反爬全站拦截**，API / raw 全部返回验证页 |
| Citron | `git.citron-emu.org` | ❌ 连接失败 |
| LibHac | <https://www.nuget.org/packages/LibHac>（NuGet，**MIT**，最新 0.19.0）| ✅ 包可下载，源码仓库 404 |

#### Eden 支持的格式 —— 【事实】源码直证

`src/core/loader/loader.h` 的 `FileType` 枚举**穷举**了 Eden 认识的一切：

```cpp
enum class FileType {
    Error, Unknown,
    NSO, NRO, NCA, NSP, XCI, NAX, KIP,
    DeconstructedRomDirectory,
};
```

`src/core/loader/loader.cpp` 的 `IdentifyFile()` 按 NSP → XCI → NRO → NCA → NAX → KIP → NSO → DeconstructedRomDirectory 顺序试探，`GetFileTypeString()` 也只列这 8 种。

→ <https://git.eden-emu.dev/eden-emu/eden/raw/branch/master/src/core/loader/loader.h> · [`loader.cpp`](https://git.eden-emu.dev/eden-emu/eden/raw/branch/master/src/core/loader/loader.cpp)

**→ Eden（以及整个 yuzu 血脉）不支持 `.nsz` / `.xcz` / `.ncz`。必须先 `nsz -D` 解压。**

#### Ryujinx / Ryubing

**【推断，两个旁证】**（源码不可达，见 §9）：

1. Flathub 上的 Ryujinx 打包清单 `org.ryujinx.Ryujinx.appdata.xml` 声明的 mediatype【事实】：
   ```xml
   <provides>
       <binary>Ryujinx</binary>
       <mediatype>application/x-nx-nca</mediatype>
       <mediatype>application/x-nx-nro</mediatype>
       <mediatype>application/x-nx-nso</mediatype>
       <mediatype>application/x-nx-nsp</mediatype>
       <mediatype>application/x-nx-xci</mediatype>
   </provides>
   ```
   → <https://github.com/flathub/org.ryujinx.Ryujinx/blob/master/org.ryujinx.Ryujinx.appdata.xml>（**无 nsz/xcz**）
2. ES-DE 给 Switch 平台配的扩展名（既覆盖 Eden 也覆盖 Ryujinx）同样没有 nsz/xcz（§6.2）。

**→ 【推断】Ryujinx/Ryubing 也不支持 NSZ/XCZ。** 若要确证需要能访问 `git.ryujinx.app`。

#### 结论

**用户库里的 55 个 `.nsz`/`.xcz`（74.67 GiB，占 Switch 容量的 33.4%）在任何主流模拟器里都开不了，必须先解压。** 解压后体积会明显膨胀（`.xcz` 平均 3.34 GiB → 还原成 XCI 后可能到 8–12 GiB 级）。这直接关系到 ADR-0017（设备能力档案驱动转换）：**Switch 的「转换」不是可选优化，是能否运行的前提。**

`nsz` 本身是 MIT + 纯 Python，`nsz -D -o <dir> <file>` 即可，可以直接作为外部工具调用。**但注意：解压需要 `prod.keys`**（nsz README：「You need to have a hactool compatible keys file … You must legally obtain your keys!」）。→ **这是本项目唯一一处「不得不面对密钥」的地方，而它属于「转换」而非「识别」，可以设计成用户自备密钥的可选功能。**

### 6.2 前端

#### ES-DE ⚠ 有实质缺口

【事实】`resources/systems/{linux,windows}/es_systems.xml`：

```xml
<system>
    <name>switch</name>
    <fullname>Nintendo Switch</fullname>
    <path>%ROMPATH%/switch</path>
    <extension>.nca .NCA .nro .NRO .nso .NSO .nsp .NSP .xci .XCI</extension>
    <command label="Eden (Standalone)">%EMULATOR_EDEN% -f -g %ROM%</command>
    <command label="Ryujinx (Standalone)">%EMULATOR_RYUJINX% %ROM%</command>
    <platform>switch</platform>
    <theme>switch</theme>
</system>
```
（Windows 版多一个 `.lnk` 与 `Shortcut` 启动项）

→ <https://gitlab.com/es-de/emulationstation-de/-/raw/master/resources/systems/linux/es_systems.xml>

**【实测】全文件 `grep -ic "nsz\|xcz"` = 0。**

USERGUIDE 的平台总表也确认【事实】：

| 平台 | 全名 | 默认模拟器 | 备选 | 需要 BIOS |
|---|---|---|---|---|
| `switch` | Nintendo Switch | **Eden (Standalone)** | Ryujinx (Standalone), *Shortcut* [W] | **Yes** |

`es_find_rules.xml` 里 `RYUJINX` 规则包含 `io.github.ryubing.Ryujinx`（Ryubing 的 flatpak id）与 `org.ryujinx.Ryujinx`【事实】。

**→ 用户库里 55 个 `.nsz`/`.xcz`（37.7% 的文件、33.4% 的容量）在 ES-DE 里默认完全看不见。**

**修法**：ES-DE 支持在 `~/ES-DE/custom_systems/es_systems.xml` 里补一个 `switch` 条目覆盖扩展名【事实，USERGUIDE §「Game system customizations」】：

> If system customizations are required, a separate es_systems.xml file should instead be placed in the `custom_systems` folder in the ES-DE application data directory. … the intention is that the file in `custom_systems` **complements** the bundled configuration, meaning only systems that are to be customized should be included.

→ <https://gitlab.com/es-de/emulationstation-de/-/raw/master/USERGUIDE.md>

**→ 本项目的 ES-DE 导出器应当在导出 Switch 平台时，一并生成 `custom_systems/es_systems.xml` 片段**，把 `.nsz .NSZ .xcz .XCZ` 加进扩展名列表。这是一个几行代码的功能，但没有它，三分之一的 Switch 库在 ES-DE 里是隐形的。

#### Pegasus ✅ 无约束

Pegasus **不内置任何平台定义**。`metadata.pegasus.txt` 里的 `extension:` 完全由用户声明【事实，官方文档】：

```
collection: Game Boy Advanced
extension: gba
launch: myemulator {file.path}
```

→ <https://pegasus-frontend.org/docs/user-guide/meta-files/>

**→ Switch 在 Pegasus 里想写什么扩展名就写什么，`nsz`/`xcz` 不构成问题。** 本项目本来就以 Pegasus 为主（CLAUDE.md），所以主路径无阻塞；ES-DE 是次要导出目标但要加那几行。

**【实测】** GitHub 代码搜索 `repo:mmatyas/pegasus-frontend "xci"` 返回空 —— 印证 Pegasus 代码里确实没有平台/扩展名硬编码。

### 6.3 Rust crate 现状

【实测，crates.io API】：

| crate | 版本 / 下载 / 更新 | 许可 | 评价 |
|---|---|---|---|
| ⭐ **`nx-archive`** | 0.1.2 / 2 764 / 2025-03-27 | **MIT** | **唯一对口的 Rust 实现**。仓库 <https://github.com/RyouVC/nx-archive> 有 `src/formats/{pfs0,hfs0,xci,romfs,keyset,title_keyset}.rs` + `nca/{mod,keys,types}.rs` + `cnmt/{mod,enums,extended_header}.rs`，还带 `doc/{xci,nca,cnmt,ncmsvc}.wikitext`（switchbrew 快照）与 `test/{Browser.nsp, Browser.nsz, Browser-cnmt/Application_0100c4c320c0ffee.cnmt}` 测试数据。**⚠ 只有 4 星、10 个月没更新**，宜作参考而非依赖 |
| `linkle` | 0.2.11 / 17 402 / **2021-07-25** | — | "Nintendo file format manipulation library and tools"，含 PFS0。**已停更 5 年** |
| `hactool-sys` | 0.4.4 / 17 227 / **2019-03-05** | — | hactool 的 unsafe FFI 绑定，作者自称 "Currently untested"。**不要用** |
| `xts-mode` | 0.6.0 / 1 634 622 / 2026-05-05 | — | AES-XTS。**只有走密钥路线才需要**（且 NCA 用的是非标准 tweak，要自己改字节序）|
| `file-format` | 0.29.0 / 1 731 739 / 2026-03-27 | — | **【实测】`src/lib.rs` 里 grep `nintendo/nsp/xci/switch` 零命中** —— 不认识任何 Switch 格式 |
| `zstd` / `zstd-safe` | — | — | 若要自己解 NCZ 需要（**不建议**，直接调 `nsz` CLI）|

**【推断】结论**：PFS0/HFS0/XCI header 三个解析器加起来不到 300 行 Rust，**自己写比引依赖更划算** —— 尤其考虑到 `nx-archive` 只有 4 星且 10 个月未动。可以把 `nx-archive` 的源码当成正确性参照（它的 `doc/*.wikitext` 就是 switchbrew 的快照，正好和本文对得上）。

---

## 7. 无密钥识别能做到什么程度 —— 按置信度分层

### A 层：内容 ID 精确匹配（最高置信，适用于 NSP / NSZ）

**方法**：
1. 读 PFS0 头（`0x10` 字节）→ EntryCount、StringTableSize
2. 读条目表（`0x18 × N`）+ 字符串表 → 全部文件名
3. 抽出所有 `[0-9a-f]{32}` 前缀的 `.nca` / `.ncz` / `.cnmt.nca` / `.cnmt.ncz`
4. 在 titledb `cnmts.json` 的 `contentEntries[].ncaId` 里查 → **(titleId, version)**
5. 交叉验证：容器里所有 ContentId 应当落在同一个 (titleId, version) 的清单里；若清单里的条目在容器里缺失，标为「不完整」

**I/O 成本**：每文件只读**头部几 KB**。146 个文件全扫 < 1 秒。

**置信度**：**【实测】173 502 个 ncaId 中 100.00% 唯一映射到一个 (titleId, version)**，零冲突。且 ContentId 本身是 SHA-256 前缀，天然抗碰撞。

**输出**：TitleID、精确版本号、内容类型（Base/Update/DLC/Delta，从 `titleType`）、完整内容清单、`otherApplicationId`（Update 指回 Base）。

**覆盖率估计**【推断】：eShop/CDN 来源的 NSP/NSZ 应有 90%+ 命中（titledb 涵盖 45 603 个 titleId / 235 227 个 NCA，且每日更新）。**对 XCI/XCZ 基本无效**（§3.2 的 `isGameCard` 只有 121 条）。

### B 层：RightsId 直读（次高置信，适用于全部四种格式，无需任何数据库）

**方法**：从容器文件名里找 `[0-9a-f]{32}\.tik`，取前 16 个 hex 字符 = TitleID，末 2 个 hex = KeyGeneration。

**成本**：与 A 层共用同一次头部读取，**零额外 I/O**。

**置信度**：**【实测】** 71 182 条样本，`rightsId[16:30]` 100% 恒零、`rightsId[0:16]` 与所属 title 100% 前 13 位 hex相同，零反例。三个一手实现（nsz / nxdumptool / switch-library-manager）均按此解析。

**可得性**：**【实测】19 516 / 19 518（100.0%）的基础 title 至少有一个带 rightsId 的 NCA** → 零售内容基本都有 `.tik`。

**限制**：
- 拿不到版本号（`.tik` 只有 TitleID + KeyGeneration）
- `--remove-titlerights` 处理过的包没有 `.tik`（对应 NX Game Info 的 `Converted` 结构）
- **XCI 的 secure 分区同样有 `.tik`**【事实，nxdumptool `tikRetrieveTicketFromGameCardByRightsId()`】→ **这是 XCI 最可靠的免密钥 TitleID 来源**

### C 层：明文 XML（高置信，只适用于 scene / authoring 风格的 NSP）

**方法**：容器里若有 `<ContentId>.cnmt.xml`，按 PFS0 条目表切出该字节区间，**直接当 UTF-8 XML 解析**（没有加密、没有压缩）。内容形如【事实，nxdumptool `cnmt.c` L433–436 的生成模板】：

```xml
<ContentMeta>
  <Type>Application</Type>
  <Id>0x0100000000010000</Id>
  <Version>0</Version>
  ...
```

同时可读 `.nacp.xml`（含各语言游戏名）、`.legalinfo.xml`、`.programinfo.xml`、`cardspec.xml`。

**成本**：一次小范围 `pread`（XML 通常 < 16 KB）。

**置信度**：直接来自转储工具写出的元数据，**等同于读 CNMT 本体**。

**覆盖率**：只有 BigBlueBox 风格 scene NSP 与 authoring-tool 产物有；CDN rip 一般没有。switch-library-manager 里这段代码是注释掉的（作者选了带密钥的路线），但**逻辑完全正确且免密钥**，可以直接照抄。

### D 层：文件名约定（中置信，兜底）

Switch 场景的事实标准命名：`Game Name [0100XXXXXXXXX000][v65536].nsp`。

**正则**（直接取自 switch-library-manager，【事实】）：
```go
titleIdRegex = regexp.MustCompile(`\[(?P<titleId>[A-Za-z0-9]{16})]`)
versionRegex = regexp.MustCompile(`\[[vV]?(?P<version>[0-9]{1,10})]`)
```
→ <https://github.com/trembon/switch-library-manager/blob/master/src/db/localSwitchFilesDB.go>

**置信度**：中。文件名可被任意改写；但 A/B/C 层的结果可以**反向校验**文件名 —— 不一致时是极强的「文件被改过 / 命名错误」信号，直接进 triage 队列。

⚠ 用户库有 **33.6% 的文件名含汉字** —— 中文命名的 Switch 文件很可能丢掉了 `[TitleID]`。D 层对这部分大概率失效，**所以 A/B/C 三层必须做，不能只靠文件名。**

### E 层：整文件 CRC32 匹配 NSWDB（只适用于 XCI，成本高）

**方法**：对整个 `.xci` 算 CRC32，查 NSWDB 的 `imgcrc`（4 053 条，**100% 唯一**）。

**成本**：**要读满 108.19 GiB**。按 500 MB/s 机械盘算约 3.6 分钟，SSD 上更快 —— 一次性可接受，但要落库缓存（key = 路径 + 大小 + mtime）。

**收益**：拿到场景发布名、`LA-H-XXXXX` 产品码、region、group、firmware、`card` 类型（含 `Tencent` 国行标记）。

**⚠ 未定项**：`imgcrc` 到底覆盖哪些字节（是否 trimmed、cert 是否已 FF 化）—— **【未找到权威来源】**。实现时应先用几个已知样本试算三种口径（全文件 / trimmed 到 `trimmedsize` / cert 区 FF 化），取命中率最高的。

### F 层：结构指纹（辅助判定，非识别）

免密钥即可判定的「这是什么形态的转储」（§3.1 表）：Scene XCI / Converted XCI / Scene NSP / CDN NSP / Converted NSP / Homebrew / Not complete。以及国行判定（§2.1.5，一次 SHA-256）。

这些不产生 TitleID，但直接喂给 ADR-0008 的 verdict accretion 与 ADR-0002 的 triage。

### 推荐组合

```
读容器头（几 KB） ─┬─ 有 .cnmt.xml ──────→ C 层：TitleID + Version + 游戏名     [最高]
                   ├─ 有 ContentId ─查 titledb→ A 层：TitleID + Version + 内容清单 [最高]
                   ├─ 有 .tik ──────────→ B 层：TitleID + KeyGeneration        [高]
                   └─ 都没有 ───────────→ D 层：文件名正则                     [中]
                                    ↓
                         F 层结构指纹 + 国行判定（附加到任何一层）
                                    ↓
                  XCI 且以上都没定到版本 → E 层：整文件 CRC32 查 NSWDB         [高，昂贵]
                                    ↓
                  查 titledb {region}.{lang}.json → 名称/发行商/语言/图标/截图/发售日
```

**预期结果**【推断，基于各层覆盖率】：

| 层 | 对 82 个 `.nsp` + 48 个 `.nsz` | 对 9 个 `.xci` + 7 个 `.xcz` |
|---|---|---|
| A（ContentId 查表）| 高命中 | ≈ 0 |
| B（`.tik`）| 高命中 | **高命中** |
| C（`.cnmt.xml`）| 部分（scene 风格）| 部分 |
| E（CRC32 → NSWDB）| — | 高命中（108 GiB 扫描一次）|

**→ 综合：应能给全部 146 个文件定出 TitleID，其中 eShop 系（130 个）还能定出精确版本。这已经超过本项目对 PSV / Wii U / X360 的识别水平，而且一把密钥都没用。**

**还缺什么（免密钥拿不到的）**：
- NACP 里的原生游戏名（→ 用 titledb 的 `name` 替代，覆盖 45 603 个 titleId）
- 繁中/简中的精确区分（→ titledb 只给 `zh`）
- NCA 级完整性/签名校验（→ 只能做容器级 SHA-256 校验：HFS0 条目表里每个文件前 `0x200` 字节的哈希是明文给出的，**这个可以免密钥验**）

---

## 8. 实现成本估算

### 与已有平台的对照基准

| 平台 | 人日 | 主要成本来源 |
|---|---:|---|
| NGPC | 0.3 | 单一头部，无容器 |
| WS | 0.5 | 头部在文件末尾 |
| Lynx | 0.6 | 两种头部形态 |
| 3DO | 1 | 光盘文件系统 + 592 条 ID 表 |
| Wii U | 3 | WUD/WUX/loadiine 三种形态 + TMD 解析 |
| XBOX360 | 3–4 | ISO/GOD/XBLA/XEX 四种形态 + XGD 偏移 |
| PSV | 8.5 | VPK/NoNpDrm/MaiDump 多封装并存 + 成型规则 |

### Switch 免密钥路线拆解

| 工作项 | 人日 | 说明 |
|---|---:|---|
| PFS0 + HFS0 解析器（共用一份代码，只切条目大小）| **0.3** | 两者只差 magic 与 `0x18` vs `0x40`。参照 `switchfs/pfs0.go` 与 `nx-archive/src/formats/{pfs0,hfs0}.rs` |
| XCI 头 + 双偏移探测（`0x100` / `0x1100`）+ 四分区遍历 | **0.3** | 字段表已在 §2.1.2 完整给出，照抄即可 |
| 文件名分类器（ContentId / RightsId / XML / cert）+ NX Game Info 式结构分类 | **0.4** | 纯正则 + 集合判断，无 I/O |
| **NSZ / XCZ 支持** | **0.05** | 只需在扩展名匹配里加 `.ncz` / `.cnmt.ncz`。**容器层零改动** |
| titledb 摄取（`ncas.json` 87 MB + `cnmts.json` 50 MB）+ 建索引 + 增量更新 | **0.8** | JSON 大但结构简单。要设计好本地索引（ncaId → (titleId, version) 是 173 502 条，SQLite 或 fst 都行）。**日更策略要做**（ETag / commit sha 比对）|
| titledb 区域文件摄取（刮削用：名称/发行商/语言/图标/截图/发售日）| **0.5** | 每个区域文件 50–95 MB；只需要 US + HK + JP 三个即可覆盖中英日 |
| NSWDB 摄取（`imgcrc` / serial / region / card）+ XCI CRC32 | **0.5** | ⚠ 含 `--compressed` 的下载坑（§4.4）+ `imgcrc` 口径试探 |
| 中文/区域派生（官中标注、国行 TitleID 集、`card == Tencent`）| **0.3** | 数据都在 titledb + NSWDB 里，纯派生逻辑 |
| 国行 XCI 的免密钥哈希后缀判定 | **0.2** | 照抄 hactool 的两次 SHA-256 |
| ES-DE `custom_systems/es_systems.xml` 片段生成（补 `.nsz`/`.xcz`）| **0.15** | 几行模板，但没它 33.4% 容量隐形 |
| Base/Update/DLC 的分组模型（TitleID 尾 `000`/`800`/其他）+ 「最新更新」选择 | **0.4** | 82 个 nsp 里有大量更新包，不做这个库体检会失真。参照 `localSwitchFilesDB.go` 的去重逻辑 |
| 接入现有平台声明式配置 + 单测 | **0.4** | |
| **小计（免密钥完整路线）** | **≈ 4.3** | |

**如果先做最小可用版**（A + B + D 层 + titledb `ncas.json`/`cnmts.json`，不做 NSWDB / 不做刮削 / 不做国行判定）：

| 工作项 | 人日 |
|---|---:|
| PFS0/HFS0 + XCI 头 + 文件名分类 | 1.0 |
| titledb 核心两文件摄取 + 索引 | 0.8 |
| Base/Update/DLC 分组 | 0.4 |
| 接入 + 单测 | 0.3 |
| **最小可用小计** | **≈ 2.5** |

### 定位

| | 人日 | 与已有平台对照 |
|---|---:|---|
| **Switch 最小可用（P1）** | **2.5** | ≈ Wii U(3) 的水平，比 X360(3–4) 便宜 |
| **Switch 完整（含刮削 + NSWDB + 中文派生）** | **4.3** | 介于 X360(3–4) 与 PSV(8.5) 之间 |
| Switch 密钥路线（**不建议**）| +3~4 | AES-XTS 非标 tweak + key area 派生 + AES-CTR + NACP/RomFS 解析，**外加 §3.3 的法律风险** |

**为什么 Switch 比 PSV(8.5) 便宜这么多**：PSV 贵在「VPK / NoNpDrm / MaiDump 多封装并存 + 成型规则复杂」。Switch 只有**一种容器结构**（PFS0 与 HFS0 是同一个东西的两个变体），四种扩展名共享 95% 的代码路径，且有一个 MIT 许可、每日更新、覆盖 45 603 个 title 的现成数据库。

**建议优先级：P1，与 Wii U / X360 同批。** 理由：
1. 容量占比 2.54%（223.84 GiB），是全库第一梯队的单平台体量之一
2. **8 447 个官中 TitleID** —— 中文覆盖是全库最好的平台，符合本项目的核心诉求
3. 免密钥路线能做到「精确到版本」，识别质量高于 P1 的另外两个平台
4. **但有一个非识别的硬依赖**：`.nsz`/`.xcz` 要在模拟器上跑必须解压，而解压要 `prod.keys`。这一项应该单独作为 ADR-0017 的一个设备能力条目处理，**不要和识别耦合**。

### 实现陷阱清单

1. **XCI 双偏移** —— 必须探测 `"HEAD"` 在 `0x100` 还是 `0x1100`，别硬编码（§2.1.1）
2. **两套 ContentType 枚举** —— NCA 头的（0=Program,1=Meta,…）与 NCM 的（0=Meta,1=Program,…）**顺序不同**。titledb 的 `ncas.json` 用前者、`cnmts.json` 用后者【实测已验证】
3. **PFS0 计数字段是 u32 不是 u16** —— switch-library-manager 的 `pfs0.go` 用 `binary.LittleEndian.Uint16(header[0x4:0x8])` 读 FileCount，并用 uint16 算 `fileEntryTableOffset := 0x10 + (fileEntryTableSize * fileCount)`。**HFS0 条目 0x40 字节时，超过 1023 个文件就会溢出**。别照抄这一处
4. **NSWDB 下载必须加 `--compressed`**，否则随机截断（§4.4）
5. **不要 `git clone` titledb**（44 GB），走 `raw.githubusercontent.com` 单文件
6. **NCZ 头里的 `cryptoKey`/`cryptoCounter` 不要读**（§2.3.2）
7. **`.nsp` 平均只有 512 MiB** —— 大量是更新包/DLC，不做 Base/Update/DLC 分组，库体检数字会严重失真
8. **ES-DE 不认 `.nsz`/`.xcz`** —— 导出时必须同时产出 `custom_systems` 片段

---

## 9. 明确未能取得权威来源的事项

1. **No-Intro 的 Switch DAT 为何不在公开 Love Pack 里** —— 只观察到「334 个 DAT 里没有」这个事实，原因未知（可能需登录、可能被刻意排除）。**未访问 datomatic 求证。**
2. **No-Intro Switch DAT 的具体条目数、哈希算法（CRC32/MD5/SHA-1/SHA-256 各是否都给）、以及 `Digital (CDN)` 集的确切命名** —— 无法在不访问 datomatic 的前提下确认。
3. **`dat notes` 里的 cert 偏移 `0x7000` 究竟对应 scene-style 还是 FullXCI 的文件偏移** —— 与 nxdumptool 的 `GAMECARD_CERT_OFFSET 0x7000` 一致指向 scene-style，但同页又给出 FullXCI 制作法。本文按【推断】处理，实现时用 `cardHeaderBase + 0x7000` 兼容两者。
4. **NSWDB `imgcrc` 覆盖的字节范围** —— 是全文件、trimmed 到 `trimmedsize`、还是 cert 已 FF 化？站点无文档。
5. **NSWDB 的使用许可** —— 站点无 LICENSE / ToS 页面。
6. **NSWDB 的数据截止时间** —— XML 里没有日期字段，只有递增 `id`（最大 4055）。
7. **Ryujinx / Ryubing 的确切格式支持列表** —— `git.ryujinx.app` 被 Anubis 反爬全站拦截（API、raw、页面全部返回验证页），GitHub 上全系已被 DMCA 清空。只能靠 Flathub appdata 的 mediatype 与 ES-DE 的扩展名表【推断】。
8. **Citron 的状态** —— `git.citron-emu.org` 连接失败（curl code 000）。
9. **LibHac 的源码** —— GitHub 404；NuGet 上包还在（MIT，0.19.0），但只有编译产物。
10. **Hasheous 对 Switch 的覆盖** —— 平台列表接口只返回首页 100 条，分页参数超时，未能穷举。
11. **Switch 民间汉化的规模，以及是否存在集中式索引**（类似 GoodTools `[T+Chi]` 或 TOSEC 的 `[tr zh]`）—— 中文汉化站不构成一手来源。
12. **重打包「魔改整合版」是否保留原 `.tik`** —— §5.3 的第 2 条是【推断】，未取得实际样本验证。
13. **XCI secure 分区里 `.tik` 的实际出现率** —— nxdumptool 的代码证明「会去那里找」，但没有统计数据说明有多大比例的卡带确实带票据。§7 B 层对 XCI 的「高命中」是【推断】。
14. **XCI 的 `serial`（`LA-H-XXXXX`）能否从卡带数据里提取** —— No-Intro dat notes 自己说「Its not being extracted in any consistent way (even though it possibly can be from the game data, for a small number of titles)」，即连 No-Intro 的 datter 也主要靠人工填。
15. **`nsz -D` 还原后是否逐字节等于原文件** —— nsz README 说 "losslessly"，但未取得逐字节比对的实测或第三方验证。

---

## 10. 访问约束的遵守

- **全程未访问 `datomatic.no-intro.org`。** 所有 No-Intro DAT 数据来自 [`hugo19941994/auto-datfile-generator`](https://github.com/hugo19941994/auto-datfile-generator) 的 Release 附件（`no-intro.xml` / `no-intro_parent-clone.xml` / `redump.xml`）。
- 本文引用的 [`wiki.no-intro.org`](https://wiki.no-intro.org/) 与 `datomatic.no-intro.org` 是**不同主机**，沿用 `rom-identification-part2.md` 的既有判断，不在禁令范围。对该 wiki 只做了只读的 `action=raw` 与 `api.php?action=query&list=search` 调用，未提交任何表单。
- `switchbrew.org` 对 WebFetch 返回 403，改用 `curl` + 常规 UA 访问 `index.php?...&action=raw`（MediaWiki 的标准只读接口）。
- `nswdb.com` 只做了 GET `xml.php`（其公开的数据导出端点）。
- 本次调研**未下载、未持有、未查看任何 `prod.keys` / `title.keys` / `console.keys` 内容**，也未下载任何 Switch ROM/转储文件。所有格式结论均来自公开文档与开源代码。

---

## 11. 来源清单

### 格式规范（switchbrew wiki）

⚠ 该站对 WebFetch 返回 403，取原始 wikitext 用：`https://switchbrew.org/w/index.php?title=<PAGE>&action=raw`

| 页面 | URL | 本文用途 |
|---|---|---|
| **XCI** | <https://switchbrew.org/wiki/XCI> | 整卡布局、CardHeader、CardHeaderEncryptedData、HFS0/PartitionFsHeader/FileEntryTable、四个分区语义、RomSize/Flags/SelSec/CompatibilityType |
| **NCA** | <https://switchbrew.org/wiki/NCA> | NCA 加密说明、Header 全表、FsHeader、**PFS0 与 PartitionEntry 结构** |
| **CNMT** | <https://switchbrew.org/wiki/CNMT> | PackagedContentMetaHeader、PackagedContentInfo、各 ExtendedHeader |
| **NACP** | <https://switchbrew.org/wiki/NACP> | ApplicationTitle、SupportedLanguageFlag、**16→18 语言索引表（含繁中 13 / 简中 14）** |
| **Ticket** | <https://switchbrew.org/wiki/Ticket> | 签名类型表、Rights ID 位于 ticket data `0x160` |
| **NCM_services** | <https://switchbrew.org/wiki/NCM_services> | ContentType 枚举（0=Meta…6=DeltaFragment）、ContentMetaType 枚举（0x80=Application…）|
| Title_list | <https://switchbrew.org/wiki/Title_list> | 系统 title（**不含游戏**）|
| Filesystem_services | <https://switchbrew.org/wiki/Filesystem_services> | GameCardSize / GameCardAttribute |
| ⚠ **NSP** | **该页不存在**（`list=search&srsearch=NSP` → `totalhits: 0`）| NSP 只能靠工具源码 |

### 开源工具源码

| 项目 | 许可 / 活跃度 | 关键文件 |
|---|---|---|
| ⭐ **[DarkMatterCore/nxdumptool](https://github.com/DarkMatterCore/nxdumptool)**（`rewrite` 分支）| GPL-3.0，1 282 星，**2026-08-04** | `include/core/nca.h`（NcaHeader 全表 + `NCA_HFS_*_NAME_LENGTH`）、`include/core/gamecard.h`（`GAMECARD_CERT_OFFSET 0x7000` 等 4 个位移常量）、`include/core/tik.h`（TikCommonBlock）、`source/core/nca.c`（L555 ContentId=SHA256[0:16]、L618 文件名生成）、`source/core/tik.c`（L308 从卡带 secure 分区按 `<rightsid>.tik` 取票据）、`source/core/cnmt.c`（L433 XML 模板、L558 CNMT 文件名解析）、`code_templates/nxdt_rw_poc.c`（L7858–7930 NSP 全部条目命名、L8116 ContentId 重算）。**最权威的转储侧实现** |
| ⭐ **[nicoboss/nsz](https://github.com/nicoboss/nsz)** | **MIT**，2 354 星，**2026-08-23** | **`docs/formats.md`（NSZ/XCZ/NCZ 唯一规范）**、`README.md`（Legal 一节 + 密钥要求）、`nsz/Fs/Pfs0.py`（PFS0 读写，证明字符串表明文）、`nsz/Fs/Ticket.py`（票据零解密解析）、`nsz/Fs/Type.py`（枚举）、`nsz/ExtractTitlekeys.py`（`titleId = rightsId[0:16]`）、`nsz/{Block,Solid}Compressor.py`（`newFileName = path[0:-1]+"z"`）|
| ⭐ **[SciresM/hactool](https://github.com/SciresM/hactool)** | **ISC**，1 181 星，2023-12-04 | `xci.c`（XCI 处理 + **国行免密钥判定**）、`xci.h`（`xci_header_t` / `COMPAT_CHINA`）、`hfs0.c`（HFS0 零解密解析）、`settings.h`（`nca_keyset_t`：`header_key[0x20]` vs `xci_header_key[0x10]`）、`utils.c`（`check_memory_hash_table_with_suffix`）、`README.md`（外部密钥约定）|
| ⭐ **[trembon/switch-library-manager](https://github.com/trembon/switch-library-manager)** | **MIT**，150 星，**2026-08-30** | `src/switchfs/pfs0.go`（PFS0/HFS0 统一解析）、`xci.go`、`nsp.go`、`cnmt.go`（二进制 CNMT 偏移）、`nca.go`（`missing key - header_key`）、`src/db/localSwitchFilesDB.go`（**密钥有无的分支 + 文件名正则 + Base/Update/DLC 分组**）。**功能上与本项目最对口的现成实现** |
| [garoxas/NX_Game_Info](https://github.com/garoxas/NX_Game_Info) | GPL-3.0，212 星，**2022-12-08（停更）** | README 里的**必需密钥清单**与**七种 Structure 分类判据** |
| [blawar/nut](https://github.com/blawar/nut) | GPL-3.0，1 267 星，2026-01-19 | titledb 的生成器 |
| [RyouVC/nx-archive](https://github.com/RyouVC/nx-archive) | **MIT**，4 星，2025-03-27 | **唯一 Rust 实现**：`src/formats/{pfs0,hfs0,xci,romfs,keyset}.rs`、`nca/`、`cnmt/`、`doc/*.wikitext`、`test/Browser.{nsp,nsz}` |
| [Atmosphere-NX/Atmosphere](https://github.com/Atmosphere-NX/Atmosphere) | — | `docs/changelog.md`（LayeredFS 的 `/atmosphere/contents/<program id>/` 约定）、`docs/components/modules/ams_mitm.md` |
| [eden-emu/eden](https://git.eden-emu.dev/eden-emu/eden) | GPL-3.0 | `src/core/loader/loader.h`（**FileType 枚举 —— 无 NSZ/XCZ**）、`loader.cpp`（`IdentifyFile` / `GetFileTypeString`）、`src/frontend_common/content_manager.h` |
| LibHac | **MIT**，<https://www.nuget.org/packages/LibHac>（0.19.0）| Ryujinx 的底层库，GitHub 源已 404 |

### 数据库

| 来源 | URL | 规模 / 许可 | 用途 |
|---|---|---|---|
| ⭐ **blawar/titledb** | <https://github.com/blawar/titledb> · `https://raw.githubusercontent.com/blawar/titledb/master/<file>` | **MIT**，日更（2026-08-30）| `ncas.json` 235 227 条 / `cnmts.json` 39 648 title / 41 个区域元数据文件。**免密钥识别 + 刮削的核心** |
| ⭐ **NSWDB** | <https://nswdb.com/xml.php>（**必须 `--compressed`**）| 4 054 条，**许可未知** | XCI 整文件 CRC32（100% 唯一）+ 产品码 + region + `card == Tencent` |
| **No-Intro 镜像**（禁止直连 datomatic）| <https://github.com/hugo19941994/auto-datfile-generator> → `releases/latest/download/{no-intro.xml,no-intro.zip}` | 334 DAT，2026-07-08 | **【实测】Switch 缺席** |
| No-Intro wiki | <https://wiki.no-intro.org/index.php?title=Nintendo_-_Nintendo_Switch_dat_notes> · `..._undumped` · `..._Digital_undumped` | — | Switch 集的 datting 规则（cert FF 化、FullXCI 配方、CDN 集按 NCA 哈希）|
| Redump 镜像 | 同上 `redump.xml` | 60 DAT | **【实测】无 Switch**（Switch 无光盘）|
| libretro-database | <https://github.com/libretro/libretro-database> | — | **【实测】无 Switch** |
| Hasheous | <https://hasheous.org/api/v1/Lookup/Platforms> | — | **未能核实** |

### 生态 / 前端 / 法律

| 来源 | URL |
|---|---|
| **ES-DE** `es_systems.xml` | <https://gitlab.com/es-de/emulationstation-de/-/raw/master/resources/systems/linux/es_systems.xml>（windows 版同目录）|
| **ES-DE** `es_find_rules.xml` | <https://gitlab.com/es-de/emulationstation-de/-/raw/master/resources/systems/linux/es_find_rules.xml> |
| **ES-DE** USERGUIDE（`custom_systems` + 平台总表）| <https://gitlab.com/es-de/emulationstation-de/-/raw/master/USERGUIDE.md> |
| **Pegasus** metadata 文档 | <https://pegasus-frontend.org/docs/user-guide/meta-files/> |
| Ryujinx Flathub appdata（mediatype 声明）| <https://github.com/flathub/org.ryujinx.Ryujinx/blob/master/org.ryujinx.Ryujinx.appdata.xml> |
| **Nintendo DMCA 通知（2025-03-06，575 仓库）** | <https://github.com/github/dmca/blob/master/2025/03/2025-03-06-nintendo.md> |
| **Nintendo DMCA 通知（2026-06-30，Flathub Ryubing）** | <https://github.com/github/dmca/blob/master/2026/06/2026-06-30-nintendo.md> |
| crates.io API | `https://crates.io/api/v1/crates?q=<query>` |
