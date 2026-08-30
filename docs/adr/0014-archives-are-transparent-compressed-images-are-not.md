# 归档穿透识别、压缩镜像原样对待；rar 转换是独立的显式操作

库里大量 ROM 以 rar / zip / 7z 打包，光盘类大量使用 chd / cso / pbp。这两类东西性质相反，处理方式必须分开：

- **透明容器（zip / 7z / rar）**：只是包装。识别必须穿透它读内部字节，容器自身的哈希毫无意义。分卷压缩的多个分卷合起来才算一个容器。
- **压缩镜像（chd / cso / pbp / rvz）**：**不是包装**，它就是变体本身的形态，模拟器直接读它。绝不能当容器解包。

## 性能地基（待调研确认）

zip / 7z / rar 的容器元数据中都存有内部文件的 CRC32、未压缩大小与文件名，理论上**零解压即可取得**；而 No-Intro 与 TOSEC 的 DAT 都带 CRC32。CHD 头部据信存有原始镜像的 SHA-1，可直接与 Redump 比对。若这些成立，「全库解压」这个性能噩梦就能避免。已派专项调研核实——**这是整条识别管线的地基，未经证实不得作为设计前提**。已知风险点：7z 与 rar 的 solid block 可能导致「读一个小文件也要解压整块」。

## rar 的决定

RetroArch 支持 zip 与 7z 但不支持 rar（待同一份调研核实）。若属实，用户库中的 rar 部分处于**当前不可玩**状态——这已超出元数据自动化的范围，但它是库的真实状态。

- **识别能力 v1 内置**：穿透 rar 取 CRC32 与内部文件名，不改动任何文件，零风险。
- **转换为 zip 是独立的显式操作**：干跑预览、先告知空间开销、**只新增不删除**。删除原 rar 是另一个需要单独确认的动作。转换目标选 zip 而非 7z——兼容性最好，且 zip 的中央目录存 CRC32，转换后仍可零解压识别。

这样既支持了需求，又没有拆掉 ADR-0004（v1 对 ROM 只读）的安全边界：转换只新增文件，不修改也不删除原文件。

## 修订（据压缩容器调研的源码级结论）

**性能地基成立，但形态与原设想不同。**

- ZIP / 7z / RAR 的元数据里**只有 CRC-32**，没有 MD5、没有 SHA-1（RAR5 可选额外存 BLAKE2sp）。而 DAT 每条 `<rom>` 都带 `size` + `crc`。因此**第一命中层必须用 CRC-32 + 未压缩大小**。若第一层坚持 SHA-1，10T 必须全量解压。
- **本 ADR 原文中「CHD 头部存有原始镜像 SHA-1 可直接与 Redump 比对」是错的**。CHD V5 头部确有 `rawsha1`（偏移 64），但 `chdman info` 的两个 SHA-1 **都对不上 Redump**——粒度与扇区布局双重不同。
- **压缩镜像反而比归档更好识别**：只有 CHD 存内容哈希，CSO/ZSO/DAX/PBP/WIA/RVZ/WBFS 都没有，但它们全都把关键信息明文放在文件前部——PBP 的 `PARAM.SFO` 在 `param_sfo_offset`（通常 0x28）处未压缩；WIA/RVZ 把光盘头 0x80 字节原样放在文件偏移 0x58；WBFS 把 Wii 光盘头 256 字节放在 `hd_sector_size`（通常 0x200）。读几百字节即可取得 Game ID / DISC_ID。
- **solid block 可解**：7z 头部的 `NumUnPackStreamsInFolders` / `FileToFolder` / `UnpackPositions` 零解压可得，可按 folder 分组、一块一趟流式喂多个 hasher，把朴素实现的约 100× 放大降回 1×。**RAR 的 solid 更糟——头部无等价索引，只能顺序走。**

## 必须显式处理的坑

- ⭐ **NKit 检测必须前置于 CRC 匹配**。Dolphin 源码原话：这个文件的 CRC32 可能和好 dump 的 CRC32 相同，即使两个文件并不完全一样。判据：光盘逻辑偏移 `0x200` 处 4 字节 == `"NKIT"`。漏掉这一步会把 NKit 文件误认成完好转储。
- **RAR5 分卷的非末段 CRC 是「打包后数据」的 CRC**，比对 DAT 必然全落空。
- **ZIP flag bit 3**：local header 的 CRC 与大小为 0，必须读 central directory。另需处理 ZIP64 哨兵。
- **`.zip.001` / `.7z.001` 与 `.z01…zip` 是两回事**：前者是 7-Zip 的通用 Split handler，纯字节切分，`cat` 即可还原；后者是 ZIP 官方 split，入口是**最后**那个 `.zip`。
- **RAR5 只写 BLAKE2sp 时**，该容器退化为必须完整解压。
- **WBFS 有损**，无法还原与 Redump 一致的 ISO；**RVZ 无损**。

## UnRAR 许可

`unrar` crate 内嵌 UnRAR 的 C++ 源码，许可传染（禁止用于开发兼容压缩器，分发时须三处附带原文，Debian 归 non-free）。**零解压层自行实现 RAR4 / RAR5 头部解析器**——只需读头不需解压，可彻底避开该许可。

## 模拟器支持矩阵中的反直觉结论

- **RetroArch 只支持 zip / 7z / apk / zst，不支持 rar**（`file_archive_get_file_backend()` 的分派只有这四个）。例外仅 melonDS（libarchive 白名单含 `.rar`）与 DeSmuME 的 Windows 前端。
- **PPSSPP 不支持 ZSO 与 CSO v2**（源码 `hdr.ver > 1` 报错，对应 issue 已 closed as not planned），**而 PCSX2 支持 zso**。与直觉相反。
- **ES-DE 的哈希刮削对归档文件本身算哈希、不穿透**；**Pegasus 纯扩展名匹配、归档不透明**。
