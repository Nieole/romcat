# NTFS 与 APFS 修改时间精度实测 —— `(路径, 大小, 修改时间)` 够不够支撑增量扫描

> **调研日期**：2026-08-31
> **要回答的问题**：票 02（`.scratch/rom-metadata-automation/issues/02-catalog-and-incremental-scan.md`）最后一条验收 ——「在 NTFS 与 APFS 上验证修改时间精度足以支撑增量判断」。这条**没法用 fixture 验**，只能在真盘上量。
> **约束**：主库只读（ADR-0004）。本次探查**只读取元数据**，未写入任何文件、未修改任何时间戳、未卸载或重新挂载该盘。全部命令为 `stat` / `readdir` / 只读 `open`。
> **标注约定**：
> - **【实测】** = 本次在本机真盘上跑出来的结果，附命令与数字
> - **【事实】** = 有一手来源 URL 直接支撑
> - **【推断】** = 基于实测与事实的推理，**未经来源或实验直接证实**
> - **【未找到权威来源】** = 明确标注，不猜
>
> **相关文档**：库的规模与形态见 [`../library-facts.md`](../library-facts.md)；只读约束见 [ADR-0004](../adr/0004-rom-files-are-read-only.md)。

---

## 目录

- [第 0 章 · 核心结论速览](#第-0-章--核心结论速览)
- [第 1 部分 · 测量环境与方法（可复现）](#第-1-部分--测量环境与方法可复现)
- [第 2 部分 · 精度到底是多少](#第-2-部分--精度到底是多少)
- [第 3 部分 · 走的是哪条系统调用，Rust 是不是同一条路](#第-3-部分--走的是哪条系统调用rust-是不是同一条路)
- [第 4 部分 · 三元组够不够用](#第-4-部分--三元组够不够用)
- [第 5 部分 · 跨平台一致性](#第-5-部分--跨平台一致性)
- [第 6 部分 · 顺带查的两件事](#第-6-部分--顺带查的两件事)
- [第 7 部分 · 对票 02 的可执行建议](#第-7-部分--对票-02-的可执行建议)
- [附录 A · 交回的待办](#附录-a--交回的待办)
- [附录 B · 复现脚本与一手来源](#附录-b--复现脚本与一手来源)

---

## 第 0 章 · 核心结论速览

1. **⭐ NTFS 经 macOS 读出来的修改时间是完整的 100 纳秒精度，不是 1 秒。** 【实测】4,577 个文件，亚秒部分**100.00% 是 100 ns 的整数倍**，且在亚秒非零的 1,255 个样本里，**73.71%（925 个）落在「不是整微秒」的刻度上**。也就是说 NTFS 原生的 100 ns 刻度被驱动**原样透出**，没有被截断到秒、毫秒或微秒。担心的「亚秒位全是 0」没有发生。

2. **APFS 是 1 纳秒，比 NTFS 更细。**【实测】4,000 个文件里只有 71.12% 是 100 ns 的整数倍，例如 `1788154814.199511406`。两侧的精度都远超增量判断的需要，**精度不是瓶颈**。

3. **⭐ 跨平台没有系统性时区偏差 —— 这是本次风险最高的一条，已被实测排除。** 用 Windows 自己写进回收站 `$I` 文件**内容里**的绝对 UTC FILETIME 作参照（那是原始字节，不经任何文件系统驱动），与 macOS 侧 `stat` 读出的同一个文件的 mtime 逐个对照：**12/12 全部吻合，差值 0.4 ms – 31.6 ms，中位数 1.8 ms**，没有任何一个出现 ±8 小时的时区特征。fskit 的「FILETIME → Unix 纪元」换算在**纪元（1601）、单位（100 ns）、时区（UTC）三项上全部正确**。

4. **⭐ 但跨平台确实有一个系统性差异，而且已被实测坐实 —— 出在文件名上，不在时间戳上：macOS 的 NTFS 驱动把文件名规范化成 NFD 返回，盘上存的（也是 Windows 会返回的）是 NFC。**决定性证据：同一个字符串在 `.tar.zst` 的 tar 头里（原始字节，不经任何驱动）是 **NFC**，在 `readdir` 返回的同名文件上是 **NFD**，**12 个归档 16 处比对，16/16 全部如此，反例 0**。**路径是三元组的第一个字段**，两侧字符串不同 → 这些文件在换平台后会被判成「旧的删了、新的加了」。**受影响面【实测】占路径的 1.99%（采样 4,577 条中 91 条），外推全库约 5,100 个文件** —— 不是全库，但足够让人以为增量扫描坏了。好消息是**可以在工具侧一劳永逸消掉**（路径键入库前统一规范化成 NFC）。

5. **三元组可以用**，但要附三个条件：路径键必须**相对扫描根 + 分隔符归一 + Unicode 规范化归一**；时间戳必须以**「Unix 纪元起的整数纳秒」**持久化，不能存平台原生结构、不能截断到秒；对 `stat` 失败的文件走单独通道。

6. **在这个库里，三元组漏判的实际概率接近于零。**【实测】87.0% 的文件创建时间**晚于**修改时间（拷贝入库的典型特征），而「落盘后又被就地改过 1 小时以上」的只有 **3 个（0.07%）**。这是个只读 ROM 库，不是工作目录 —— 理论失效场景（同一 100 ns 刻度内改动且大小不变）在这里没有现实触发路径。

7. **全库扫描确认：没有任何一个时间戳异常。**【实测】对 `/Volumes/新加卷/Game` 全量 `find` 判定「早于 1990 年或晚于 2026-09-01」的文件，结果 **0 条**。纪元 0、1601 年、未来时间一个都没有。

8. **⭐ errno 45 的文件比想象的更糟：不只是拿不到 mtime，是什么都拿不到。**【实测】全库精确计数 **4,085 个（占 256,128 的 1.59%）**。对这些文件 `stat`/`lstat`/`open`/`read`/`fstat` **全部**返回 errno 45，且**稳定复现**（同一路径连测 5 次全失败）。只有 `readdir` 能拿到名字与「它是个文件」。更隐蔽的是：**`getattrlistbulk`（Finder 与快速遍历器走的那条路）根本不把它们列出来 —— 23 个里列出 0 个**。建议**不要**每次当作已变重扫，而是单列「无法读取」状态。

---

## 第 1 部分 · 测量环境与方法（可复现）

### 1.1 环境

| | |
|---|---|
| 主机 | macOS **15.7.7**（Build 24G720），Apple Silicon |
| 被测卷（NTFS） | `/dev/disk4s1` → `/Volumes/新加卷`，18.0 TB（已用 17.5 TB） |
| 挂载参数 | `ntfs, local, nodev, nosuid, read-only, noowners, noatime, fskit` |
| NTFS 驱动 | Apple 自带 `/System/Library/Filesystems/ntfs.fs`，`CFBundleIdentifier = com.apple.filesystems.util.ntfs`，**版本 3.14.3**，`Info.plist` 里 `FSImplementation = UserFS` |
| 承载进程 | `/usr/libexec/fskitd`、`/usr/libexec/fskit_agent`、`/System/Library/PrivateFrameworks/UserFS.framework/userfsd` |
| 对照卷（APFS） | `Macintosh HD`（`File System Personality: APFS`） |
| 语言运行时 | Python **3.14.2**、rustc **1.98.0** |
| 被扫描的库 | `/Volumes/新加卷/Game`，**256,128 个文件 / 8.60 TiB**（数字取自 [`../library-facts.md`](../library-facts.md)） |

**【实测】驱动身份的确认命令**：

```bash
mount | grep 新加卷
#   /dev/disk4s1 on /Volumes/新加卷 (ntfs, local, nodev, nosuid, read-only, noowners, noatime, fskit)
plutil -p /System/Library/Filesystems/ntfs.fs/Contents/Info.plist | grep -E 'FSImplementation|CFBundleVersion|CFBundleIdentifier'
#   "CFBundleIdentifier" => "com.apple.filesystems.util.ntfs"
#   "CFBundleVersion" => "3.14.3"
#   "FSImplementation" => [ 0 => "UserFS" ]
ps aux | grep -E 'fskitd|userfsd|fskit_agent'
```

**注意**：`pluginkit -mv -p com.apple.fskit.fsmodule` 只列出 `com.apple.fskit.msdos` 与 `com.apple.fskit.exfat` 两个 FSKit 扩展，**没有** NTFS 的。NTFS 走的是 `ntfs.fs` + `UserFS.framework`（`FSImplementation = UserFS`），挂载参数里那个 `fskit` 标记来自同一套用户态文件系统栈。**这不是第三方驱动（Paragon / Tuxera / macFUSE + ntfs-3g 都没装）**，`/Library/Filesystems/` 是空的。

### 1.2 采样方法（为什么不做全量遍历）

上次完整遍历用了 27 分钟（并发 8）。本次除了第 6 部分那一次**专门的**全库扫描外，其余全部走**分层随机采样**：

1. 挑 **29 个顶层平台目录**（`3ds FC GB gba gbc MD n64 nds ngc pc ps ps2 ps3 psp SFC SS switch VB Wii WIIU wsc 街机 模拟器 DOS 安卓 存档 ons krkr2 PSV`）；
2. 每个目录做**广度优先目录枚举**，受「深度 ≤ 4 层 / ≤ 4000 个目录 / ≤ 45 秒（PSV 放宽到 120 秒）」三重限制；
3. 把枚举到的目录**打乱**，每个目录随机取 ≤ 25 个文件，直到该平台取满 220 个（PSV 600 个）。

**【实测】采样规模**：枚举 **6,840 个目录**，尝试 `stat` **4,791 个文件**，成功 **4,577** 个，失败 **214** 个（全部 errno 45），总耗时 **94 秒**。
APFS 对照组用同一脚本跑 `~/dev`、`~/Library/Caches`、`/Applications`、`/usr/share` 四个根，枚举 9,281 个目录，取到 **4,000 个文件**。

采样脚本见 [附录 B](#附录-b--复现脚本与一手来源)。**脚本只调用 `os.scandir` 与 `os.stat`，不打开、不写入任何文件。**

### 1.3 三套独立的读取路径互相印证

同一批路径分别用三个入口读，用来排除「是某个语言运行时的锅」：

| 入口 | 底层 | 用途 |
|---|---|---|
| Python `os.stat().st_mtime_ns` | `stat(2)` | 主采样，整数纳秒，无浮点损失 |
| BSD `stat(1) -f "%Fm"` | `stat(2)` | 十进制字符串输出，旁证；也用来取 `%FB`(birthtime)/`%Fc`(ctime)/`%Fa`(atime) |
| Rust `std::fs::metadata().modified()` | `stat(2)` | **工具真正要用的那条路** |

---

## 第 2 部分 · 精度到底是多少

### 2.1 ⭐ NTFS：完整的 100 纳秒，没有被截断

【实测】4,577 个文件的 `st_mtime_ns`，把亚秒部分（`mtime_ns % 1_000_000_000`）拿出来看整除性：

| 亚秒部分能被整除的粒度 | 文件数 | 占比 |
|---|---|---|
| **1 ns** | 4,577 | 100.00% |
| **100 ns** | **4,577** | **100.00%** |
| 1 µs | 3,652 | 79.79% |
| 10 µs | 3,564 | 77.87% |
| 100 µs | 3,550 | 77.56% |
| 1 ms | 3,545 | 77.45% |
| 10 ms | 3,515 | 76.80% |
| 100 ms | 3,341 | 73.00% |
| 1 s（即亚秒部分为 0） | 3,322 | 72.58% |

这张表要这样读：

- **「100 ns 那一行是 100.00%，而 1 µs 那一行掉到 79.79%」= 量化步长恰好是 100 ns。**每一个观测值都落在 100 ns 的整数刻度上（不多不少），而有整整 20% 的值**落在两个整微秒之间** —— 这只有在数据源本身就是 100 ns 刻度时才会出现。
- 亚秒部分为 0 的 72.58% **不是精度不够**，而是那些文件的时间戳本来就是整秒（由只保留整秒的工具写入 —— tar / 老 zip / 拷贝工具）。
- 只看**亚秒非零**的 1,255 个样本，按「第一个能整除它的粒度」分档：

| 最细有效粒度 | 文件数 | 占非整秒样本 |
|---|---|---|
| 100 ms | 19 | 1.51% |
| 10 ms | 174 | 13.86% |
| 1 ms | 30 | 2.39% |
| 100 µs | 5 | 0.40% |
| 10 µs | 14 | 1.12% |
| 1 µs | 88 | 7.01% |
| **100 ns** | **925** | **73.71%** |
| 10 ns | 0 | 0.00% |
| 1 ns | 0 | 0.00% |

**925 个文件（占亚秒非零样本的 73.71%）的时间戳精确到 100 ns 这一档，而 10 ns 与 1 ns 两档恰好是 0 个。**这就是 100 ns 硬量化的指纹。

再看一个独立指标：1,255 个非整秒样本里有 **1,129 个互不相同的亚秒值**，把它们排序后**相邻差的最小值是 200 ns**（次小的几个是 800 / 1500 / 1800 / 2300 ns）。若真实刻度粗于 100 ns，不可能出现 200 ns 的相邻间隔。

**其余三个时间戳同样是 100 ns 粒度**【实测】（用 `stat -f "%Fm|%FB|%Fc|%Fa"` 对同一批 4,577 个文件取）：

| 时间戳 | 亚秒非零占比 | 全部是 100 ns 整数倍 | 全部是 1 µs 整数倍 |
|---|---|---|---|
| `mtime` 修改时间 | 27.4% | ✅ 是 | ❌ 否 |
| `birthtime` 创建时间 | 51.7% | ✅ 是 | ❌ 否 |
| `ctime`（NTFS 的 MFT 变更时间） | **100.0%** | ✅ 是 | ❌ 否 |
| `atime` 访问时间 | 65.2% | ✅ 是 | ❌ 否 |

**目录也一样**【实测】：`psp` 下随机 9 个目录，亚秒部分 9/9 非零且全是 100 ns 整数倍（例 `1779125591.906385600`）。

### 2.2 APFS 对照：1 纳秒

【实测】4,000 个文件：

| | 占比 |
|---|---|
| 亚秒部分为 0 | 38.35% |
| 亚秒部分非 0 | **61.65%** |
| 亚秒部分是 100 ns 整数倍 | **71.12%** ← 关键差异 |
| 亚秒部分是 1 µs 整数倍 | 70.90% |

**NTFS 那一行是 100.00%，APFS 只有 71.12%** —— 剩下 28.88% 的值带着 100 ns 除不尽的尾数，例如本仓库 `CONTEXT.md` 的 `mtime = 1788154814.199511406`（末位 `406`）。**APFS 是真正的 1 ns 粒度。**

APFS 上 38.35% 为整秒的原因与 NTFS 同理：`/usr/share`、`/Applications` 里大量文件是从 tar / pkg 里解出来的，源时间戳只有整秒。

### 2.3 两者差异的实际含义

| | NTFS（本机 fskit 只读） | APFS |
|---|---|---|
| 存储单位 | 100 ns（FILETIME 原生刻度） | 1 ns |
| 纪元 | 1601-01-01 UTC | 1970-01-01 UTC |
| 实测暴露精度 | **100 ns，完整透出** | **1 ns** |
| 时区语义 | 盘上存 UTC | 存 UTC |
| 增量判断够不够 | **绰绰有余** | **绰绰有余** |

**跨这两个文件系统做增量判断时，唯一需要小心的是：同一个内容从 APFS 复制到 NTFS 会被量化到 100 ns。**在本项目里这个方向不存在 —— 主库是 NTFS 只读，中立库在本机 APFS 上只存**读到的数值**，不回写。

---

## 第 3 部分 · 走的是哪条系统调用，Rust 是不是同一条路

### 3.1 macOS 侧：三个入口是同一条路

**【事实】** Rust 标准库文档对 `std::fs::Metadata::modified()` 的原话：

> Returns the last modification time listed in this metadata.
> The returned value corresponds to the **`mtime` field of `stat` on Unix platforms** and the **`ftLastWriteTime` field on Windows platforms**.

—— <https://doc.rust-lang.org/std/fs/struct.Metadata.html>

也就是说在 macOS 上，`modified()` 就是 `stat(2)` 的 `st_mtimespec`（`tv_sec` + `tv_nsec`），与 Python 的 `os.stat().st_mtime_ns`、BSD `stat(1)` 的 `%Fm` **是同一个内核字段**。

**【实测】直接验证，不靠推理**：把采样到的前 400 个路径同时喂给 Rust 探针与 Python，逐位比对 `mtime` 的纳秒整数值：

```
Rust std::fs::metadata().modified() 与 Python os.stat().st_mtime_ns 比对：
  逐位相同: 400
  不同:     0
  Rust 侧取不到: 0
```

**400/400 逐位相同。工具用的就是这条路，本文所有精度结论对它直接成立。**

Rust 探针源码见 [附录 B](#附录-b--复现脚本与一手来源)。它用 `duration_since(UNIX_EPOCH)` 把 `SystemTime` 转成 `i128` 纳秒后打印 —— 这也是建议中立库持久化时用的形式（见第 7 部分）。

### 3.2 Windows 侧：另一条路，但读的是同一个数

**【事实】** 在 Windows 上 `modified()` 返回的是 `ftLastWriteTime`，即 **NTFS 盘上那个 64 位 FILETIME 的原值**，没有经过任何文件系统转换层。Rust 的 `SystemTime` 在 Windows 上内部就是 FILETIME 表示，分辨率 100 ns。

**【推断】** 因此两侧读到的是同一个物理量的两种表示：

```
盘上 $STANDARD_INFORMATION 的 FILETIME（100ns since 1601 UTC）
   │
   ├─ Windows：GetFileTime → ftLastWriteTime → Rust SystemTime（仍是 FILETIME）
   │
   └─ macOS：ntfs.fs/UserFS 换算 → st_mtimespec（ns since 1970 UTC）→ Rust SystemTime
```

`duration_since(UNIX_EPOCH)` 在两侧都把它归一到「Unix 纪元起的时长」。**因为 NTFS 的刻度恒为 100 ns，而 100 ns 在纳秒表示下是整数，这条换算是无损且可逆的 —— 不存在舍入。**第 5 部分用实测确认了 macOS 侧这条换算没有偏差。

**⚠️ 但这条链有一个工程陷阱**：`SystemTime` 是**平台相关的不透明类型**，不要直接序列化它，也不要把它当作跨平台可比的值存进中立库。必须显式转成 `i64` 纳秒（或 `i64` 个 100 ns 刻度）再存。详见第 7 部分。

---

## 第 4 部分 · 三元组够不够用

### 4.1 什么情况下 `(路径, 大小, 修改时间)` 会漏判

三元组判「未变」的充要条件是三个字段全都没变。因此**漏判 = 内容变了，但路径、大小、mtime 三个都没变**。具体到这台机器上量出来的 100 ns 精度，失效场景只剩下面几类：

| # | 场景 | 需要同时满足 | 在本库的现实性 |
|---|---|---|---|
| 1 | **同一 100 ns 刻度内改动且大小不变** | 改动发生在与上次写入**同一个 100 ns 刻度**内，且新旧内容字节数完全相同 | **实际不可能**。要求两次独立写入落在同一个千万分之一秒里 |
| 2 | **时间戳被显式还原** | 有人用 `SetFileTime` / `touch -r` / 归档解包（`tar -p`、7-Zip 恢复时间戳）把 mtime 写回旧值，且大小不变 | **这是唯一现实的漏判路径**。例如原地解包覆盖同名同大小文件、用同步工具带 `-t` 覆盖 |
| 3 | **同大小的原地覆盖 + 时间戳伪造** | 定向 timestomping | 本库无此场景 |
| 4 | **路径复用**：删掉 A、新建同名 B，恰好同大小同 mtime | 三个字段全撞 | **可以量**，见下 |
| 5 | **mtime 只精确到秒的那 72.58% 文件** | 若判据只存整秒，则窗口从 100 ns 放大到 1 s | 只要**不截断**就不存在；见第 7 部分 |

**场景 5 值得单独强调**：库里 **72.58% 的文件本来就只有整秒的时间戳**（是写入工具的锅，不是文件系统的锅）。对这批文件，判据的实际分辨率就是 1 秒 —— **但这不是精度问题，是这些文件从来就没有更细的信息**。Windows 与 macOS 读到的都是同一个整秒值，两侧一致，不影响增量判断。

**场景 4 可以量**【实测】把采样的 4,577 个文件按不同判据分组，看有多少文件落在「与别的文件完全同键」的碰撞簇里：

| 判据 | 落在碰撞簇里的文件 | 最大簇 |
|---|---|---|
| `(大小, mtime@100ns)` | 103（2.25%） | 16 |
| `(大小, mtime@秒)` | 135（2.95%） | 16 |
| `(大小, mtime, birthtime)` | 34（0.74%） | 16 |
| `(大小, mtime, inode)` | **0（0.00%）** | 1 |

注意这里**没有把路径算进去** —— 加上路径后碰撞就归零了，因为路径本身唯一。这张表的意义是：**即使路径被复用，也还有 97.75% 的文件靠 `(大小, mtime)` 就能区分开**；而**只要再加 inode，区分度就是 100%**。

### 4.2 在这个库里，漏判的实际概率有多大

这不是通用文件系统，是个**只读的 ROM 库**。用两组实测数据来定量：

**证据一：绝大多数文件是「拷进来就没再动过」。**【实测】4,577 个文件里：

| | 文件数 | 占比 |
|---|---|---|
| `birthtime > mtime`（创建时间晚于修改时间 —— 拷贝入库的指纹） | 3,983 | **87.0%** |
| `birthtime == mtime` | 17 | 0.4% |
| `mtime > birthtime` | 577 | 12.6% |

`birthtime > mtime` 意味着：文件是被**拷贝/解压**到这个盘上的，创建时刻是落盘那一刻，而 mtime 保留了源头的值。**这类文件此后再没被写过。**

**证据二：连那 12.6% 也几乎全是「创建时顺带写入」，不是「事后修改」。**【实测】对 `mtime > birthtime` 的 577 个文件，看差值分布：

| `mtime - birthtime` | 文件数 | 占全样本 |
|---|---|---|
| > 1 秒 | 431 | 9.42% |
| > 1 分钟 | 217 | 4.74% |
| > 1 小时 | **3** | **0.07%** |
| > 1 天 | **1** | **0.02%** |

（分位数：p10 = 0.06 s，p50 = 38.5 s，p90 = 178.8 s）

差值在秒到分钟量级的，是「新建文件 → 边下载/解压边写 → 写完」这一个连续动作，不是两次独立的修改。**真正意义上「落盘很久之后又被就地改过」的文件只有 3 个，占 0.07%。**

**结论**：本库的写入模式是**近乎一次写入、此后只读**。第 4.1 节里唯一现实的漏判路径（场景 2：解包覆盖 + 恢复时间戳 + 大小不变）需要用户主动去做「原地覆盖同名同大小文件」这个动作 —— 而 ADR-0004 明确工具自己一个字节都不改。**剩下的风险来自用户手动整理，量级是每年几次，而不是每次扫描。**

### 4.3 如果要更强的判据，有哪些选项，各自代价多大

【实测】四个候选判据在本机 NTFS 上的可用性：

| 判据 | 可用性 | 额外 I/O | 跨平台可比 | 代价与说明 |
|---|---|---|---|---|
| **inode / 文件 ID**（`st_ino`） | ✅ 可用。4,577 个全部唯一，`nlink` 全为 1 | **零** —— `stat` 顺带返回 | ⚠️ **不可比，见下** | 最便宜的加强。**但只在同一平台的两次扫描间有意义** |
| **创建时间**（`st_birthtime`） | ✅ 可用，100 ns 粒度，缺失 0 个 | **零** —— `stat -f %FB` / `statx` 顺带返回 | ⚠️ 需实测 | 把碰撞从 2.25% 降到 0.74%。**它的真正价值不是防漏判，是识别「同一路径被换了一个新文件」** |
| **MFT 变更时间**（`st_ctime`） | ✅ 可用，亚秒 100% 非零；`ctime == mtime` 只占 5.20% | **零** | ⚠️ 需实测 | 比 mtime 更难被伪造（`SetFileTime` 改不了它）。**但 NTFS 上它随任何元数据变动而变，会造成假阳性** |
| **内容哈希** | ✅ 可用 | **极高** —— 要读完整文件 | ✅ 完全可比 | 全库 8.60 TiB，外置盘 100–200 MB/s → **12–24 小时**。绝不能常态化 |

**关于 inode 的可比性【推断】**：macOS 侧 `st_ino` 报的是 **MFT 记录号**（实测到的值是 76474、10137、10139 这类小而连续的数）。Windows 的 `nFileIndexHigh/Low` 是 `(序列号 << 48) | MFT 记录号` —— **低 48 位相同，高 16 位不同**。所以直接比会全不相等，取低 48 位才可能对上。**本次未在 Windows 侧验证，列为待办。**

**建议的分层策略**（成本从零递增，只在必要时升级）：

1. **第 0 层（默认）**：`(路径, 大小, mtime)` —— 零额外成本。
2. **第 1 层（免费加固）**：把 `birthtime` 与 `inode` 一起存进中立库，但**不参与「变没变」的判断**，只用来在扫描结果异常时**解释发生了什么**（同路径 inode 变了 = 换文件；inode 没变但 mtime 变了 = 原地改）。它们是 `stat` 顺带来的，不存白不存。
3. **第 2 层（按需）**：只对「大小相同但 birthtime 或 inode 变了」的少数文件补算哈希。按实测，这类文件不到 1%。
4. **第 3 层（人工触发）**：`--rehash-all`，明确告知要跑十几个小时。

**不建议**把 `ctime` 放进判断条件 —— 它对元数据变动（权限、属性、甚至碎片整理）敏感，会制造大量假阳性重扫。存下来做诊断可以。

---

## 第 5 部分 · 跨平台一致性

这是本次风险最高的一节。问题是：**用户在 macOS 上扫过一遍，把盘接回 Windows 再扫，会不会整个库被判为全部变化？**

结论分两半：**时间戳这条链是干净的（已实测排除）；文件名那条链是脏的（已实测坐实）。**

### 5.1 【事实】NTFS 时间戳的一手规范：纪元、单位、时区

Microsoft 官方文档 *File Times*（<https://learn.microsoft.com/en-us/windows/win32/sysinfo/file-times>）原话：

> A *file time* is a 64-bit value that represents the number of **100-nanosecond intervals** that have elapsed since **12:00 A.M. January 1, 1601 Coordinated Universal Time (UTC)**.

> **The NTFS file system stores time values in UTC format, so they are not affected by changes in time zone or daylight saving time.** The FAT file system stores time values based on the local time of the computer.

> The NTFS file system records times on disk in UTC.

**这三段把三件事钉死了**：单位 = 100 ns；纪元 = 1601-01-01 UTC；盘上存的就是 UTC，**与时区和夏令时无关**。

**因此时区偏差不可能来自 NTFS 本身，只可能来自读它的驱动。**问题收敛成一句：**macOS 的 `ntfs.fs`/UserFS 这一层，换算对不对？**

### 5.2 ⭐【实测】用 Windows 自己写下的绝对 UTC 值做交叉验证：12/12 吻合

要验证一个驱动的时区换算，需要一个**不经过这个驱动**的绝对时间参照。找到了一个：**Windows 回收站的 `$I` 元数据文件**。

Windows Explorer 删除文件时，会在 `$RECYCLE.BIN/<SID>/` 下写一个 `$I......` 文件，**文件内容的偏移 16 处是一个 64 位 FILETIME，记录删除时刻，绝对 UTC 值**。同时，这个 `$I` 文件本身的 NTFS mtime 也被设为写它的那一刻。两者应当只差几毫秒。

关键在于：**`$I` 内部那个 FILETIME 是文件的「内容」，是原始字节 —— 任何文件系统驱动都不会去动它。**而 `$I` 文件的 mtime 是「元数据」，必须经过驱动换算。**把两者对照，就把「盘上存了什么」和「fskit 换算成了什么」分离开了。**

`/Volumes/新加卷/$RECYCLE.BIN` 下有 12 个 `$I` 文件。【实测】结果：

| `$I` 文件 | 内部记录的删除时刻（绝对 UTC，原始字节） | fskit `stat` 读出的 mtime（UTC） | 差 |
|---|---|---|---|
| `$IDMBADA.rar` | 2024-08-12 07:26:52.410 | 2024-08-12 07:26:52.410580 | **+0.6 ms** |
| `$IIMECKM.rar` | 2024-08-12 07:26:52.488 | 2024-08-12 07:26:52.488735 | **+0.7 ms** |
| `$I9Y8S6W.rar` | 2024-08-12 07:26:52.504 | 2024-08-12 07:26:52.535581 | +31.6 ms |
| `$I8O374G.nsz` | 2024-09-11 13:43:18.177 | 2024-09-11 13:43:18.178888 | +1.9 ms |
| `$I8BMZHL.nsz` | 2024-09-11 13:47:06.045 | 2024-09-11 13:47:06.061431 | +16.4 ms |
| `$IUOGGGN.nsz` | 2024-09-11 13:47:21.849 | 2024-09-11 13:47:21.850956 | +2.0 ms |
| `$IVOV85H.nsz` | 2024-09-11 13:47:21.855 | 2024-09-11 13:47:21.855476 | **+0.5 ms** |
| `$IM0VEQD.nsz` | 2024-09-11 13:54:22.326 | 2024-09-11 13:54:22.351429 | +25.4 ms |
| `$IC5JAJ7.nsz` | 2024-09-11 13:57:19.847 | 2024-09-11 13:57:19.847484 | **+0.5 ms** |
| `$I9T00ZP.nsz` | 2024-09-11 13:57:19.851 | 2024-09-11 13:57:19.852678 | +1.7 ms |
| `$IX9ODNA.exe` | 2024-10-05 13:27:18.535 | 2024-10-05 13:27:18.535433 | **+0.4 ms** |
| `$IO07VZY` | 2025-05-09 13:10:19.445 | 2025-05-09 13:10:19.462142 | +17.1 ms |

```
n=12  差的中位数 = 0.0018 s  最小 = 0.0004 s  最大 = 0.0316 s
|差| < 5 秒的:            12/12
|差| ≈ 8 小时(28800s)的:   0/12
```

**这就是决定性证据**：

- **纪元正确** —— 若 fskit 用错纪元（比如把 1601 当 1970），差值会是 369 年量级，不是毫秒。
- **单位正确** —— 若把 100 ns 当 1 ns 或 1 µs，差值会是数量级偏移。
- **时区正确** —— 若 fskit 把盘上的 UTC 当本地时间（或反向多减一次 8 小时），差值会精确地是 ±28800 秒。**实测 0/12 出现这个特征。**
- **亚秒是真的** —— 差值落在 0.4–31.6 ms，正是 Explorer「算出删除时刻」到「文件系统给它盖时间戳」之间的真实延迟。若 fskit 的亚秒位是伪造或补零的，不可能与一个独立来源吻合到毫秒。

**顺带**：`$I` 内部的 FILETIME 亚秒位全部止于毫秒（`.410000`、`.488000`），而同一文件的 NTFS mtime 有完整 100 ns 尾数（`.410580`）—— 这是第 2 部分「100 ns 完整透出」的又一个独立佐证。

### 5.3 【实测】第二个独立验证：ZIP 的本地时间往返

第二条证据，语义与上面不同，作为交叉印证。**ZIP 的 DOS 时间字段存的是「打包机器的本地墙钟」，2 秒粒度，不带时区。**当 Windows 解包时，它把这个墙钟当本地时间，换算成 UTC 写进 NTFS。

库里 `AC3_CHS_Patch_1.02.zip` 与解出来的 `AC3_CHS_Patch_1.02/` 目录并排放着。把 ZIP 里记的 DOS 本地时间，与 fskit 读出的 mtime **渲染到 Asia/Shanghai 之后**比：

| 成员 | ZIP DOS（本地墙钟） | fskit mtime → Asia/Shanghai | 差 | 大小 |
|---|---|---|---|---|
| `patch/ja1.ac3` | 1999-12-11 18:16:06 | 1999-12-11 18:16:06.000000 | **0.0 s** | 4,995,641 = 4,995,641 ✅ |
| `patch/ja2.ac3` | 1999-12-11 18:16:20 | 1999-12-11 18:16:20.000000 | **0.0 s** | 4,967,631 = 4,967,631 ✅ |
| `patch/jp1.ac3` | 1999-12-11 18:16:38 | 1999-12-11 18:16:38.000000 | **0.0 s** | 7,493,829 = 7,493,829 ✅ |
| `patch/jp2.ac3` | 1999-12-11 18:17:06 | 1999-12-11 18:17:06.000000 | **0.0 s** | 5,489,102 = 5,489,102 ✅ |

**4/4 精确到 0.0 秒。**「Windows 写本地墙钟 → 盘上存 UTC → fskit 读回 → macOS 渲染成本地」这一圈**原样闭合**。若 fskit 少做或多做一次 8 小时换算，这里会显示 ±28800 秒。

**测量仪器自校验**：为确认我解 FILETIME 的代码本身没错，另取 80 个带 `0x000A` NTFS 扩展字段的 ZIP，比较**同一个成员**的「DOS 本地时间」与「NTFS 扩展字段里的 UTC FILETIME」：230 条里偏移为 0 秒的 146 条、−1 秒 55 条、−2 秒 17 条、+1 秒 11 条、+2 秒 1 条 —— **全部落在 DOS 时间 2 秒粒度的舍入范围内，没有一条是 ±8 小时**。解码器正确。

### 5.4 ⭐【实测】真正的跨平台断点在文件名：macOS 返回 NFD，盘上是 NFC

时间戳这条链干净，但**路径这条链不干净**，而路径是三元组的第一个字段。

**现象**【实测】用 `os.listdir` 的 **bytes** 接口（Python 不做任何解码或规范化）读一个日文文件名，拿到的原始字节里含 `E3 82 99`，即 **U+3099 COMBINING KATAKANA-HIRAGANA VOICED SOUND MARK** —— 分解形式（NFD）：

```
readdir 原始字节: b'New \xe3\x82\xb9\xe3\x83\xbc\xe3\x83\x8f\xe3\x82\x99\xe3\x83\xbc...'
                                              ~~~~~~~~~~~~~~~~ ハ + 组合浊点
解码后 : New スーパーマリオブラザーズ 2 0004008c0007ad00.tar.zst
是 NFC : False        是 NFD : True
NFC 应为: b'New \xe3\x82\xb9\xe3\x83\xbc\xe3\x83\x91\xe3\x83\xbc...'
                                              ~~~~~~~~ パ（预组合，单码位）
```

**这是驱动加的，还是盘上本来就是这样？** 用与 5.2 相同的手法把两者分离 —— 找一个**不经过驱动的原始字节参照**。`.tar.zst` 正合适：**tar 头部的成员名是文件内容里的原始字节**，谁建的归档就是谁写的形式。而这些归档的成员目录名，恰好就是归档自己的文件名去掉扩展名。

【实测】12 个 `.tar.zst`，16 处比对：

| 磁盘名（fskit `readdir` 返回） | 归档内 tar 头的成员名（原始字节） |
|---|---|
| `New スーパーマリオブラザーズ 2 …tar.zst` → **NFD** | `New スーパーマリオブラザーズ 2 …/` → **NFC** |
| `みんなで まもって騎士 姫のトキメキらぷそでぃ …tar.zst` → **NFD** | 同名目录 → **NFC** |
| `アイカツ!365日のアイドルデイズ …tar.zst` → **NFD** | 同名目录 → **NFC** |
| `カルドセプト リボルト …tar.zst` → **NFD** | 同名目录 → **NFC** |
| `コウシンデータバージョン4.0 スレチガイミーヒロバ …tar.zst` → **NFD** | 同名目录 → **NFC** |

```
成功读出 tar 头的归档 12 个
磁盘 NFD 且 tar 也 NFD = 0
磁盘 NFD 但 tar 是 NFC = 16     ← 16/16，零反例
```

**同一个字符串，在原始字节里是 NFC，经 fskit 读出来变成 NFD。结论确凿：macOS 的 NTFS 驱动在 `readdir` 出口做了 NFD 规范化。**

三条旁证互相吻合：

1. 【实测】枚举 **40,650 个名字**，含假名的 93 个里，**含组合式浊点（NFD 特征）70 个，含预组合浊音假名（NFC 特征）0 个**。一个 Windows 管理的盘上不可能所有日文名恰好都是 NFD —— 除非是出口被统一规范化了。
2. 【实测】NFD 与 NFC 两种写法**都能 `os.path.exists` 成功** —— 驱动做的是**规范化不敏感**的查找（HFS+/APFS 的语义）。
3. 【实测】同一个挂载点却是**大小写敏感**的：`gba` 存在、`GBA` 与 `Gba` 都不存在；`SFC` 存在、`sfc` 不存在。而 **Windows 上的 NTFS 默认大小写不敏感**。「大小写敏感 + 规范化不敏感」这个组合不是 NTFS 的语义，是驱动自己叠上去的一层。

**受影响面【实测】**（4,577 条采样路径）：

| | 条数 | 占比 |
|---|---|---|
| 整条路径 `NFC(p) != p` | **91** | **1.99%** |
| 仅文件名受影响 | 55 | 1.20% |
| 文件名没事、但某一级父目录带组合字符 | 36 | 0.79% |

出现的组合用字符：`U+3099`（组合浊点）×164、`U+309A`（组合半浊点）×14、**`U+0301`（组合锐音符）×6** —— **不只是日文**，拉丁重音字母同样中招。

那 36 条尤其要注意：**一个目录名受影响，它下面整棵子树的路径键全变。**实测受影响的目录有 5 个，例如：

```
/Volumes/新加卷/Game/switch/魔界战记7/[魔界战记7].魔界戦記ディスガイア７/HK/DLC
/Volumes/新加卷/Game/pc/Gift～ギフト～
/Volumes/新加卷/Game/pc/未来ノスタルジア
/Volumes/新加卷/Game/ps/PS1街：命运的交叉点[5.5修复版]-2022.5.3/【原声音乐】/街 オリジナル サウンドトラック/CD1
```

**后果**：外推全库约 **5,100 个文件**（1.99% × 256,128）在「macOS 扫一遍 → Windows 再扫」时，路径键对不上，会被判成「旧的删了、新的加了」。**不是整个库，但足以让人以为增量扫描坏了**，而且这批文件每次换平台都会被重新识别、重新哈希 —— 恰好是日文名的 ROM，属于识别成本最高的一批。

**这个问题可以在工具侧一劳永逸地消掉**，见第 7 部分。

### 5.5 还有一条差异：大小写

【实测】macOS 侧这个挂载点**大小写敏感**，Windows 侧 NTFS **默认大小写不敏感**。

它**不会**造成增量扫描误判（`readdir` 两边返回的都是盘上真实存储的那个大小写形式，字符串相同）。它的风险在别处：如果中立库的路径查找做成大小写不敏感，Windows 上正常、macOS 上会漏；反之如果库里存在**仅大小写不同的两个条目**，Windows 上它们无法共存而 macOS 上可以。**【推断】风险很低，但路径键的比较应当明确定为「大小写敏感的精确比较」，并在库体检里报告「仅大小写不同的路径对」。**

---

## 第 6 部分 · 顺带查的两件事

### 6.1 有没有修改时间明显不合理的文件 —— 答案是没有，一个都没有

这一项**没有采样，跑了全库**（它只做 `stat`，不读内容，可以承受）：

```bash
find "/Volumes/新加卷/Game" -type f \
     \( ! -newermt "1990-01-01 00:00:00" -o -newermt "2026-09-01 00:00:00" \) -print
```

**【实测】结果：0 条。**

也就是说全库 **没有任何一个文件**的修改时间早于 1990 年或晚于 2026-09-01。具体到担心的三类：

| 异常类型 | 全库命中 |
|---|---|
| 纪元 0（1970-01-01） | **0** |
| 1601 年（FILETIME 为 0 时的表现） | **0** |
| 未来时间 | **0** |

采样侧的分布也印证这一点【实测】：4,577 个样本的 mtime 全部落在 **1997-11-06 15:26:08 UTC 到 2026-06-15 04:28:48 UTC** 之间，没有离群值，`mtime < 0`、`mtime == 0`、`0 < mtime < 1980` 三项都是 0 个。

**对增量判断的含义：不需要为「时间戳荒谬」写特判分支。**但仍建议在中立库里对 mtime 做一次**合法区间断言**（比如 1980–「当前时间 + 1 天」），把它当作**数据完整性护栏**而不是业务逻辑 —— 因为这批数据来自 `stat` 失败率 1.59% 的驱动，将来换驱动、换平台时，这条断言是最先报警的地方。

### 6.2 ⭐ `stat` 失败的那批文件（errno 45）该怎么对待

上次库体检说「约 1%」。**这次给出精确数字。**

**【实测】全库精确计数：4,085 个文件，占 256,128 的 1.59%。** 全部是同一个错误：`Operation not supported`（errno 45 / `ENOTSUP`）。

**这些文件比想象的更麻烦 —— 不是「拿不到 mtime」，是「什么都拿不到」。**【实测】对其中 40 个逐一探测：

| 操作 | 结果 |
|---|---|
| `stat()` | ❌ errno 45 |
| `lstat()` | ❌ errno 45 |
| `open()` + `read()` | ❌ errno 45 |
| `open()` + `fstat()` | ❌ errno 45（`open` 就失败了） |
| `readdir()` 能列出这个名字 | ✅ **是** |
| `scandir` 的 `d_type` 判断「它是个文件」 | ✅ **是** |
| **`getattrlistbulk()` 能列出它** | ❌ **不能 —— 23 个里列出 0 个** |

**稳定复现**【实测】：同一路径连续 `stat` 5 次，5 次全是 errno 45。不是偶发、不是超时、不是竞态。

**⚠️ `getattrlistbulk` 那一行是个埋着的坑。**它是 Finder 与各种「快速目录遍历」走的批量属性接口。在这个驱动上它**直接把这些文件从目录列表里省略掉了**：

```
目录                                          listdir  bulk  errno45  bulk 列出的
3ds/…/日服dlc                                     99     91       5        0
FC/【模拟工具】/PSV模拟器/FC模拟器                      2      1       1        0
gba/【全部汉化】/0940jianti                          4      1       3        0
MD                                               28     23       5        0
合计（12 个目录）                                                   23        0
```

**这意味着：如果将来为了提速把遍历换成 `getattrlistbulk`（或某个用它的 crate），这 4,085 个文件会从「读不到元数据」悄悄变成「根本不存在」—— 于是它们会被判为「已删除」，从中立库里被抹掉。**当前 `RealFs::read_dir` 用的是 `std::fs::read_dir`（底层 `opendir`/`readdir`），**看得见它们**，这是对的。**这条要写进代码注释，不然将来的性能优化会踩上去。**

**分布**【实测，全库】：涉及 34 个顶层目录，不集中在某一个平台：

| 顶层目录 | 文件数 | | 顶层目录 | 文件数 |
|---|---|---|---|---|
| `MD+SS游戏合集v1.1` | 997 | | `ps2` | 218 |
| `psp` | 695 | | `SFC` | 148 |
| `废都物语吧` | 345 | | `ps` | 146 |
| `模拟器` | 321 | | `PSV` | 141 |
| `废都物语` | 247 | | `Wii` | 115 |
| `电软时光机1.0[杂志整合]` | 246 | | 其余 23 个目录 | 466 |

扩展名同样散：`.bin` 620、`.dll` 401、`.smd` 307、`.zst` 289、`.pdf` 281、`.mid` 260、`.edat` 240、`.wav` 163、`.exe` 117、`.ini` 117、`.txt` 100、`.smc` 106、`.doc` 93、`.chd` 90 …

**成因【推断】**（本次**未能**证实，见待办）：现有证据是 —— 与扩展名无关（同样的扩展名在成功组里大量存在）、与文件名长度无关（失败组均值 20.4 字符 vs 成功组 17.4）、与非 BMP 字符无关（两组都是 0）、**不是整个目录失败**（同目录里往往 99 个成功 5 个失败）、稳定复现、且这些文件**在 Windows 上正常**。这个「逐文件、跨类型、同目录内混杂」的形态最符合**文件级的 NTFS 属性**，最可能的候选是：

- **NTFS 压缩**（`compact /c`，LZNT1）—— 命中率最高的猜测。失败集里大量小而高度可压缩的文件（`.txt` / `.ini` / `.mid` / `.bmp` / `.wav` / `.lmu`），以及 `MD+SS游戏合集v1.1` 一个整合包就占 997 个，很像「右键 → 属性 → 高级 → 压缩内容」施加在整个文件夹上的结果。
- **WOF / `compact /EXE` 压缩**（重解析点 + `WofCompressedData` 流）—— `.dll` 401、`.exe` 117 与这个特征吻合。
- 稀疏文件、EFS 加密文件。

**【未找到权威来源】**：Apple 没有公开文档说明 `ntfs.fs` / UserFS 对哪些 NTFS 文件属性不支持、`ENOTSUP` 具体覆盖哪些情形。搜索只找到「macOS 内置 NTFS 只读支持」这一层面的第三方描述（<https://eclecticlight.co/2025/11/18/which-local-file-systems-does-macos-26-support/>），没有精度或属性支持矩阵。

#### 增量扫描该怎么对待它们 —— 建议

**不要每次都当作「已变」重扫。**理由：

1. 这批文件在 macOS 上**根本读不到内容**，判它「已变」也无法进一步处理 —— 识别管线拿不到字节，算不了哈希，撞不了 DAT。「重扫」的结果只能是再失败一次。每次扫描白烧 4,085 次系统调用，还会把它们反复推进待确认队列。
2. 判它「已删除」更糟 —— 会把 Windows 侧扫出来的正确结论抹掉。
3. 它是**环境限制**，不是**库的状态**。库体检报告里它属于「macOS 环境限制」那一节，不属于「库里有什么」。

**建议：给中立库的条目加一个显式的第三态。**

| 状态 | 含义 | 增量扫描的行为 |
|---|---|---|
| `known` | 元数据完整，三元组有效 | 正常比对 |
| **`unreadable`** | **路径存在（`readdir` 看得见），但元数据不可得** | **既不判「变」也不判「删」。跳过，计数，如实报告** |
| `missing` | `readdir` 里已经没有这个名字了 | 判为删除 |

配套三条：

- **`unreadable` 必须由 `readdir` 的结果驱动**，不能由 `stat` 失败推断「文件没了」。`readdir` 列得出名字 = 文件在；`stat` 失败 = 读不到属性。**这两件事必须分开。**（当前 `RealFs::read_dir` 在 `meta` 出错时落到 `(len: 0, modified: None)`，语义上把「读不到」压成了「大小 0、时间未知」—— 建议区分开，否则这 4,085 个文件会与库里那 4,317 个**真正的空文件**混为一谈。）
- **记录状态发生的那次转变**。若某个文件从 `unreadable` 变成 `known`（比如换了驱动、或用户在 Windows 上取消了压缩属性），那次要当作「新出现」走完整识别。
- **在库体检报告里单列一行**：「本次扫描有 N 个文件因驱动限制无法读取元数据（macOS `ntfs.fs`，`ENOTSUP`）；这些文件在 Windows 上正常，涉及它们的识别必须在 Windows 上做」。这与 `library-facts.md` 里已有的口径一致。

---

## 第 7 部分 · 对票 02 的可执行建议

### 7.1 直接回答：三元组能不能用

**能用。** 票 02 的验收「在 NTFS 与 APFS 上验证修改时间精度足以支撑增量判断」**通过** —— 而且不是勉强通过：

- NTFS 侧实测 **100 ns** 完整精度，APFS 侧 **1 ns**。判据需要的分辨率远低于此。
- 时间戳的跨平台语义已实测确认无偏差（第 5.2 / 5.3 节）。
- 本库的写入模式是近乎一次写入（87.0% 的文件从未被改过，「事后修改」只有 0.07%），三元组唯一的现实漏判路径在这里几乎不可能触发。

**但要附四个条件。前两条是硬要求，不做就会出问题；后两条是低成本加固。**

### 7.2 条件一（硬要求）：路径键必须规范化

**这是本次调研发现的最大风险**，而且它与「时间戳精度」无关 —— 是路径字段的问题。

```
路径键 = 规范化( 相对扫描根的路径 )

规范化 =  ① 相对化：去掉挂载点前缀（/Volumes/新加卷/Game 或 G:\Game），只存库内相对路径
          ② 分隔符：统一成 '/'（Windows 上把 '\' 换掉）
          ③ Unicode：统一 NFC        ← 少了这条，约 5,100 个文件每次换平台就重扫一遍
          ④ 比较：大小写敏感的精确比较（不做大小写折叠）
```

**③ 是本次新发现的必需项。** macOS 的 NTFS 驱动在 `readdir` 出口把名字规范化成 NFD，Windows 返回盘上原样的 NFC（第 5.4 节，16/16 实测坐实）。**统一成 NFC** 是正确方向 —— 因为盘上存的就是 NFC，Windows 侧零成本，只有 macOS 侧需要转一次。

Rust 侧实现：`unicode-normalization` crate 的 `.nfc()`，对纯 ASCII 路径几乎零成本（先做一次 `is_nfc_quick` 快速判定，绝大多数路径直接跳过）。**注意只规范化存进中立库的「路径键」，实际去访问文件时必须用 `readdir` 返回的原样字符串** —— 虽然实测该驱动的查找是规范化不敏感的（NFC/NFD 都能打开），但不该依赖这个行为。

**建议同时在库体检里加一行**：「本次扫描有 N 条路径的 Unicode 形式被规范化（原样 NFD → 存储 NFC）」。这样这个问题一旦在别的平台上表现不同，第一时间看得见。

### 7.3 条件二（硬要求）：时间戳的持久化形式

```rust
// ✅ 这样存
struct CatalogEntry {
    path_key: String,      // 见 7.2
    len: u64,
    /// Unix 纪元起的纳秒。NTFS 的 100 ns 刻度在这里是整数，无损。
    /// 取不到时为 None —— 与「0」严格区分（见 7.5）。
    modified_unix_nanos: Option<i64>,
}
```

三条禁令：

1. **不要序列化 `std::time::SystemTime`。** 它是平台相关的不透明类型（Windows 上是 FILETIME，Unix 上是 timespec），跨平台不可比。必须 `duration_since(UNIX_EPOCH)` 转成整数再存。
2. **不要截断到秒。** 库里 27.42% 的文件有亚秒信息，截断等于白扔掉两个数量级的分辨率，把漏判窗口从 100 ns 放大到 1 s。
3. **不要用 `f64` 存时间戳。** `f64` 在 2026 年这个量级（约 1.79 × 10⁹ 秒）的有效分辨率约 238 ns —— **比 NTFS 的 100 ns 刻度还粗**，会把相邻刻度混同。用 `i64` 纳秒（范围到 2262 年，够用）。本次采样脚本本身就是因为这个原因全程用 `st_mtime_ns` 而不是 `st_mtime`。

`i64` 纳秒对负值（1970 年前）也成立，虽然实测全库没有这种文件（第 6.1 节）。

### 7.4 条件三（低成本加固）：顺手把 `birthtime` 与 `inode` 也存下来，但不参与判断

`stat` 一次就返回了，**不存白不存**。它们不进「变没变」的判断条件，只用来**解释**扫描结果：

| 观察 | 解释 |
|---|---|
| 路径在，`inode` 变了 | 这个路径被换成了另一个文件 → 必须重新识别，即使大小与 mtime 碰巧相同 |
| `inode` 没变，`mtime` 变了 | 同一个文件被原地改过 → 正常的「内容变化」 |
| `mtime` 没变但 `birthtime` 变了 | 文件被重新拷贝过（内容多半没变，但值得记一笔） |

实测这能把「大小 + 时间」的碰撞率从 2.25% 降到 0（加 inode）或 0.74%（加 birthtime）。

**注意跨平台**：`inode` 与 `birthtime` **只在同平台的两次扫描之间可比**。macOS 侧 `st_ino` 是 MFT 记录号，Windows 的文件 ID 还带 16 位序列号在高位（第 4.3 节，**未验证**）。所以：**存下来，但换平台时不要拿它做判断**，只在同平台连续扫描时使用。中立库里要记下「这条记录是哪个平台扫出来的」。

### 7.5 条件四：`stat` 失败的文件走第三态

见第 6.2 节。要点重复一遍：

- 加 `unreadable` 状态，既不判「变」也不判「删」。
- **`unreadable` 由 `readdir` 驱动，不由 `stat` 失败推断。**
- 当前 `crates/core/src/fs/real.rs` 的 `read_dir` 在 `entry.metadata()` 出错时落到 `(len: 0, modified: None)`。`modified: None` 是对的；但 `len: 0` 会让这 4,085 个文件与库里 **4,317 个真正的空文件**混在一起。**建议把 `len` 也改成 `Option<u64>`，或在 `DirEntry` 上加一个显式的 `metadata_unavailable: bool`。**
- 在 `read_dir` 上写注释：**不要为了提速换成 `getattrlistbulk`** —— 那条路会让这些文件从目录列表里消失，进而被误判为已删除（第 6.2 节实测，23 个里列出 0 个）。

### 7.6 建议加进票 02 的验收项

现有六条验收保持不变，建议补三条：

- [ ] 路径键做「相对化 + 分隔符归一 + **NFC 规范化** + 大小写敏感比较」，并在库体检里报告被规范化的路径条数
- [ ] 修改时间以 `i64`「Unix 纪元起纳秒」持久化，不截断到秒、不用浮点、不序列化 `SystemTime`
- [ ] `stat` 失败的文件有独立状态，既不判为变化也不判为删除；`len` 与「读不到元数据」可区分

以及一条交给人工的（见附录 A）：

- [ ] **（人工，Windows 真机）** 在 Windows 上对同一批文件核对时间戳、路径 Unicode 形式与文件 ID，确认增量扫描能跨平台复用

### 7.7 一句话总结

> **`(路径, 大小, 修改时间)` 可以用。时间戳这一侧实测完全够（NTFS 100 ns、APFS 1 ns、无时区偏差）；真正要防的是路径那一侧 —— macOS 的 NTFS 驱动会把文件名规范化成 NFD，不统一成 NFC 的话，约 5,100 个日文/带重音名的文件每次换平台都会被当成新文件重扫一遍。**

---

## 附录 A · 交回的待办

### A.1 ⭐ 必须在 Windows 真机上人工验证（本次只能在 macOS 侧测）

**性质**：与票 23（Windows 真机中文输入法验证）同类 —— **自动化无法在 macOS 上完成**，因为要验证的正是「另一个操作系统读同一块盘会得到什么」。

**为什么仍然需要**：第 5.2 / 5.3 节已经用 Windows 自己写下的绝对 UTC 值证明了 **macOS 侧的换算没有偏差**，这是很强的证据 —— 但它证明的是「fskit 读出来的值等于盘上存的值」，**不是**「Windows 的 Rust 程序算出来的 `i64` 纳秒等于 macOS 的 Rust 程序算出来的 `i64` 纳秒」。中间还隔着 Rust 在 Windows 上的 `ftLastWriteTime → SystemTime → duration_since(UNIX_EPOCH)` 这一段，以及路径与文件 ID 的表示。**这一段必须真机跑一次。**

**怎么验（可直接执行）**

**第 1 步（macOS 侧，先做）** —— 用附录 B 的采样脚本导出一份基准，选**不含 `stat` 失败文件**的若干目录，输出 CSV：

```bash
# 输出：相对路径 | 大小 | mtime(Unix 纪元起纳秒) | inode
python3 sample_mtime.py baseline.jsonl '[["/Volumes/新加卷/Game/SFC","SFC",400,60],
                                         ["/Volumes/新加卷/Game/psp","psp",400,60],
                                         ["/Volumes/新加卷/Game/3ds","3ds",400,60]]'
python3 - <<'PY'
import json, unicodedata
R="/Volumes/新加卷/Game/"
with open("baseline.csv","w") as o:
    for l in open("baseline.jsonl"):
        r=json.loads(l); p=r["path"][len(R):]
        o.write(f'{unicodedata.normalize("NFC",p)}\t{r["size"]}\t{r["mtime_ns"]}\t{r["ino"]}\t{p!=unicodedata.normalize("NFC",p)}\n')
PY
```

**第 2 步（Windows 侧）** —— 把盘接到 Windows，跑：

```powershell
$Root = 'G:\Game'          # 按实际盘符改
$Out  = 'windows.tsv'
$UnixEpochTicks = 621355968000000000    # .NET Ticks(0001-01-01) → Unix 纪元

Get-ChildItem -LiteralPath $Root -Recurse -File -Force -ErrorAction SilentlyContinue |
  ForEach-Object {
    $rel     = $_.FullName.Substring($Root.Length + 1) -replace '\\','/'
    $nfc     = $rel.Normalize([Text.NormalizationForm]::FormC)
    $unixNs  = ($_.LastWriteTimeUtc.Ticks - $UnixEpochTicks) * 100
    "{0}`t{1}`t{2}`t{3}`t{4}" -f $nfc, $_.Length, $unixNs, $_.Attributes, ($rel -cne $nfc)
  } | Set-Content -Encoding utf8 $Out
```

**第 3 步** —— 把两份对比，逐项确认：

| # | 要确认的 | 通过标准 | 不通过意味着 |
|---|---|---|---|
| 1 | **mtime 的绝对值** | 两侧 `mtime_ns` **逐位相同** | 有系统性时区/纪元偏差 → 增量扫描跨平台完全失效，必须换判据 |
| 2 | **路径的 Unicode 形式** | Windows 侧输出的第 5 列（`rel -cne nfc`）**全为 `False`**，即盘上就是 NFC | 若 Windows 侧也出现 NFD，说明盘上本来就混着两种形式，7.2 的规范化仍然正确但报告口径要改 |
| 3 | **NFC 规范化后两侧路径键集合相等** | 差集为空 | 规范化方案没覆盖全（可能还有别的组合字符，如朝鲜文 Hangul 会分解成字母） |
| 4 | **大小** | 逐个相同 | 驱动对某类文件报错了大小 |
| 5 | **文件 ID 的可比性** | `fsutil file queryfileid` 的低 48 位 == macOS 的 `st_ino` | 7.4 里「inode 只在同平台可比」的说法要收紧成「完全不可比」 |
| 6 | **那 4,085 个 `ENOTSUP` 文件** | 在 Windows 上能正常 `stat`；且 `Attributes` 里是否带 `Compressed` / `SparseFile` / `ReparsePoint` / `Encrypted` | 这条直接**证实或推翻**第 6.2 节关于成因的【推断】。可用 `Get-ChildItem -Force \| Where-Object { $_.Attributes -band [IO.FileAttributes]::Compressed }` 交叉核对 |
| 7 | **大小写** | 确认 Windows 侧 `gba` 与 `GBA` 指向同一个目录（macOS 侧实测是两个不同的名字，只有 `gba` 存在） | —— |

**验证失败的后果与应对**：

- **第 1 项失败** → 三元组作废，退化为 `(路径, 大小, 内容哈希)`，代价是首扫要读完 8.60 TiB（12–24 小时）。**这是最坏情况，但第 5.2 节的 12/12 实测让它的概率很低。**
- **第 2/3 项失败** → 只需调整规范化目标形式，工具侧改动很小。
- **第 6 项** → 无论结果如何都不阻塞，但它决定库体检报告里怎么向用户描述这 4,085 个文件（「这些文件被 NTFS 压缩过」比「驱动不支持」有用得多 —— 前者用户可以自己在 Windows 上 `compact /u` 解掉）。

**状态**：`ready-for-human`。**不阻塞票 02 的实现** —— 7.2–7.5 的四个条件在两种结果下都成立，先按它们实现即可；这项验证是在真正跨平台使用之前的一道闸门。

### A.2 次要待办

| # | 待办 | 为什么 |
|---|---|---|
| 1 | 在 `crates/core/src/fs/real.rs` 的 `read_dir` 上写注释，说明**不可**改用 `getattrlistbulk` | 实测该接口会让 4,085 个文件从目录列表里消失，被误判为已删除（第 6.2 节） |
| 2 | 把 `DirEntry::len` 改成 `Option<u64>`，或加 `metadata_unavailable` 标志 | 现在「读不到元数据」被压成 `len: 0`，与库里 4,317 个真正的空文件混淆 |
| 3 | 库体检报告加两行：被 NFC 规范化的路径条数、`ENOTSUP` 文件条数 | 这两个数字是将来换平台/换驱动时最先报警的地方 |
| 4 | 中立库记录每条记录「是哪个平台、哪个卷 UUID 扫出来的」 | `inode`/`birthtime` 只在同平台可比；卷 UUID（本盘 `39CD08E6-B8B0-4209-BF8E-DFEAF68D9D79`）能防止把另一块盘的扫描结果当成本盘的增量 |

---

## 附录 B · 复现脚本与一手来源

> 全部脚本**只读**：只调用 `os.scandir` / `os.stat` / 只读 `open`。没有任何写入、重命名、`utime` 或挂载操作。

### B.1 采样脚本 `sample_mtime.py`

```python
#!/usr/bin/env python3
"""只读采样：收集文件的 mtime 纳秒等元数据，输出 JSONL。绝不写入被采样的卷。"""
import json, os, random, sys, time

random.seed(20260831)

def collect_dirs(root, max_depth, budget, deadline):
    """广度优先枚举目录，受深度、数量与时间三重限制。"""
    out, frontier, depth = [root], [root], 0
    while frontier and depth < max_depth and len(out) < budget:
        nxt = []
        for d in frontier:
            if time.time() > deadline or len(out) >= budget:
                break
            try:
                with os.scandir(d) as it:
                    for e in it:
                        try:
                            if e.is_dir(follow_symlinks=False):
                                nxt.append(e.path)
                        except OSError:
                            pass
            except OSError:
                pass
        out.extend(nxt); frontier = nxt; depth += 1
    return out

def sample(root, label, want, max_depth=4, dir_budget=4000, seconds=120, per_dir=25):
    deadline = time.time() + seconds
    dirs = collect_dirs(root, max_depth, dir_budget, deadline)
    random.shuffle(dirs)
    rows, errs, seen = [], [], 0
    for d in dirs:
        if len(rows) >= want or time.time() > deadline:
            break
        try:
            with os.scandir(d) as it:
                ents = list(it)
        except OSError as ex:
            errs.append({"path": d, "errno": ex.errno, "kind": "scandir"}); continue
        random.shuffle(ents)
        taken = 0
        for e in ents:
            if taken >= per_dir or len(rows) >= want:
                break
            try:
                if not e.is_file(follow_symlinks=False):
                    continue
            except OSError as ex:
                errs.append({"path": e.path, "errno": ex.errno, "kind": "is_file"}); continue
            seen += 1
            try:
                st = os.stat(e.path, follow_symlinks=False)   # ← stat(2)
            except OSError as ex:
                errs.append({"path": e.path, "errno": ex.errno, "kind": "stat"}); continue
            rows.append({"label": label, "path": e.path, "size": st.st_size,
                         "mtime_ns": st.st_mtime_ns, "atime_ns": st.st_atime_ns,
                         "ctime_ns": st.st_ctime_ns, "ino": st.st_ino,
                         "dev": st.st_dev, "nlink": st.st_nlink})
            taken += 1
    return rows, errs, len(dirs), seen

if __name__ == "__main__":
    out_path = sys.argv[1]
    spec = json.loads(sys.argv[2])   # [[root, label, want, seconds], ...]
    all_rows, all_errs = [], []
    for root, label, want, secs in spec:
        rows, errs, ndirs, seen = sample(root, label, want, seconds=secs)
        print(f"{label}: dirs={ndirs} ok={len(rows)} err={len(errs)}", flush=True)
        all_rows += rows; all_errs += errs
    with open(out_path, "w") as f:
        for r in all_rows: f.write(json.dumps(r, ensure_ascii=False) + "\n")
    with open(out_path + ".err", "w") as f:
        for r in all_errs: f.write(json.dumps(r, ensure_ascii=False) + "\n")
```

调用：

```bash
# NTFS：29 个平台目录
python3 sample_mtime.py ntfs.jsonl '[["/Volumes/新加卷/Game/SFC","SFC",220,45], ...]'
# APFS 对照
python3 sample_mtime.py apfs.jsonl '[["/Users/nicoer/dev","dev",1200,60],
                                     ["/Users/nicoer/Library/Caches","caches",1200,60],
                                     ["/Applications","apps",800,60],
                                     ["/usr/share","usr_share",800,60]]'
```

**全精度旁证**（含 `birthtime` / `ctime` / `atime`，避免 `f64` 损失）：

```bash
tr '\n' '\0' < allpaths.txt | xargs -0 stat -f "%Fm|%FB|%Fc|%Fa|%i|%z|%N" > statout.txt
```

### B.2 Rust 探针 `probe.rs`（验证 `Metadata::modified()` 走同一条路）

```rust
// 只读探针：用 std::fs::metadata().modified() 取 mtime，打印到纳秒。
use std::{env, fs, io::{self, BufRead}, time::UNIX_EPOCH};

fn main() {
    let args: Vec<String> = env::args().collect();
    let paths: Vec<String> = if args.len() > 1 { args[1..].to_vec() }
        else { io::stdin().lock().lines().filter_map(|l| l.ok()).collect() };
    for p in paths {
        match fs::metadata(&p) {
            Ok(m) => match m.modified() {
                Ok(t) => {
                    let ns = match t.duration_since(UNIX_EPOCH) {
                        Ok(d)  =>   d.as_nanos() as i128,
                        Err(e) => -(e.duration().as_nanos() as i128),
                    };
                    println!("OK\t{}\t{}\t{}", ns, m.len(), p);
                }
                Err(e) => println!("MTERR\t{}\t{}\t{}", e.raw_os_error().unwrap_or(-1), m.len(), p),
            },
            Err(e) => println!("STATERR\t{}\t0\t{}", e.raw_os_error().unwrap_or(-1), p),
        }
    }
}
```

```bash
rustc -O -o probe probe.rs && ./probe < paths.txt > rsout.txt
# 然后与 Python 的 st_mtime_ns 逐位比对 → 400/400 相同
```

### B.3 ⭐ 时区验证：回收站 `$I` 文件的绝对 UTC FILETIME

```python
import glob, struct, os, datetime
DELTA = 11644473600 * 10_000_000      # 1601-01-01 → 1970-01-01，单位 100ns

for p in glob.glob("/Volumes/新加卷/$RECYCLE.BIN/*/$I*"):
    with open(p, "rb") as f: b = f.read(600)
    ver, osize, ft = struct.unpack("<qqq", b[:24])    # 偏移16 = 删除时刻 FILETIME(UTC)
    if ver == 1:
        name = b[24:544].decode("utf-16-le", "replace").split("\x00")[0]
    else:
        nl = struct.unpack("<I", b[24:28])[0]
        name = b[28:28+nl*2].decode("utf-16-le", "replace").split("\x00")[0]
    del_utc_ns = (ft - DELTA) * 100                   # 原始字节：不经任何驱动
    fs_utc_ns  = os.stat(p).st_mtime_ns               # 元数据：经过 fskit 换算
    print(f"{os.path.basename(p):<14} 差={(fs_utc_ns-del_utc_ns)/1e9:+.4f}s  原路径={name}")
```

判读：**差应当落在毫秒级**（Explorer 算出时刻到文件系统盖戳的延迟）。若出现 ±28800 秒即为 8 小时时区偏差；若出现年量级偏差即为纪元错误。**实测 12/12 全部落在 +0.0004 ~ +0.0316 秒。**

### B.4 ⭐ 文件名规范化：tar 头原始字节 vs `readdir`

```python
import os, unicodedata
from compression import zstd          # Python 3.14+

def tar_first_names(path, limit=1_500_000):
    with open(path, "rb") as f: raw = f.read(limit)
    plain = zstd.ZstdDecompressor().decompress(raw)     # 只需第一个块
    out, off = [], 0
    while off + 512 <= len(plain) and len(out) < 12:
        blk = plain[off:off+512]
        if blk[:1] == b"\0": break
        name   = blk[0:100].split(b"\0")[0]
        prefix = blk[345:500].split(b"\0")[0]
        sz     = int(blk[124:135].split(b"\0")[0].strip() or b"0", 8)
        out.append(((prefix + b"/" + name if prefix else name).decode("utf-8", "replace")))
        off += 512 + ((sz + 511) // 512) * 512
    return out

COMB = ("゙", "゚")           # 组合浊点/半浊点 = NFD 特征
for p in ["/Volumes/新加卷/Game/3ds/.../New スーパーマリオブラザーズ 2 ....tar.zst"]:
    disk = os.path.basename(p)
    print("磁盘名(readdir):", disk, "NFD?", any(c in COMB for c in disk))
    for nm in tar_first_names(p):
        print("tar 头成员名  :", nm, "NFD?", any(c in COMB for c in nm))
```

**判读**：tar 头里的名字是文件**内容**，任何文件系统驱动都不会改它。磁盘名 NFD + tar 名 NFC = **驱动做了 NFD 规范化**。实测 16/16 如此，反例 0。

字节级确认（Python 的 bytes 接口不做任何解码或规范化）：

```python
names = os.listdir(b"/Volumes/.../\xe6\x97\xa5\xe6\x9c\x8ddlc")   # bytes 接口
b"\xe3\x82\x99" in names[0]      # U+3099 组合浊点 → True 即为 NFD
```

### B.5 `ENOTSUP` 文件的探测

```python
# 稳定性
[os.stat(p) for _ in range(5)]                    # 5/5 errno 45

# 除 stat 外还能拿到什么
os.lstat(p)                                        # errno 45
open(p, "rb").read(16)                             # errno 45
os.path.basename(p) in os.listdir(os.path.dirname(p))   # True ← 只有 readdir 有效

# getattrlistbulk 会不会列出它 —— 用 ctypes 调 getattrlistbulk(2)
#   请求 ATTR_CMN_RETURNED_ATTRS|ATTR_CMN_NAME|ATTR_CMN_OBJTYPE|ATTR_CMN_ERROR
#          |ATTR_CMN_MODTIME|ATTR_CMN_FILEID + ATTR_FILE_DATALENGTH
#   实测：12 个目录里的 23 个 ENOTSUP 文件，被列出 0 个
```

全库精确计数（**这一次是全量遍历**，因为只做 `stat`）：

```bash
find "/Volumes/新加卷/Game" -type f \
     \( ! -newermt "1990-01-01 00:00:00" -o -newermt "2026-09-01 00:00:00" \) \
     -print > anomalies.txt 2> anomalies.err
wc -l anomalies.txt     # → 0     异常时间戳
wc -l anomalies.err     # → 4085  Operation not supported
```

### B.6 一手来源清单

| 来源 | URL | 用于本文哪一处 |
|---|---|---|
| Microsoft — *File Times* | <https://learn.microsoft.com/en-us/windows/win32/sysinfo/file-times> | NTFS 时间戳的纪元（1601-01-01 UTC）、单位（100 ns）、**「NTFS 盘上存 UTC，不受时区与夏令时影响」**原话；FAT 与 CDFS 的对照 |
| Microsoft — `FILETIME` structure | <https://learn.microsoft.com/en-us/windows/win32/api/minwinbase/ns-minwinbase-filetime> | 64 位 FILETIME 的定义 |
| Rust std — `fs::Metadata::modified` | <https://doc.rust-lang.org/std/fs/struct.Metadata.html> | **「corresponds to the `mtime` field of `stat` on Unix platforms and the `ftLastWriteTime` field on Windows platforms」**原话 —— 第 3 部分的立论基础 |
| The Eclectic Light Company — *Which local file systems does macOS 26 support?* | <https://eclecticlight.co/2025/11/18/which-local-file-systems-does-macos-26-support/> | macOS 内置 NTFS **只读**支持；第三方（Paragon 等）才提供写入。**注：这不是 Apple 官方来源** |
| Apple — FSKit | <https://developer.apple.com/documentation/fskit> | FSKit 作为受支持的文件系统扩展 API |

**【未找到权威来源】** 的三项，明确列出不猜：

1. **Apple 没有任何公开文档描述 `ntfs.fs` / UserFS 的时间戳精度或换算规则。** 第 2 部分与第 5.2 节的结论**全部来自实测**，不是文档。
2. **Apple 没有公开 `ENOTSUP` 覆盖哪些 NTFS 文件属性**（第 6.2 节的成因是【推断】）。
3. **Apple 没有公开该驱动是否对文件名做 Unicode 规范化**（第 5.4 节的结论来自实测，16/16 + 40,650 个名字的分布 + 字节级确认，但没有文档背书）。

### B.7 本次实测覆盖面一览

| 测量 | 样本量 | 方法 |
|---|---|---|
| NTFS mtime 精度 | **4,577 文件 / 29 平台目录 / 6,840 目录** | 分层随机采样 + `os.stat` |
| APFS mtime 精度（对照） | **4,000 文件 / 4 个根 / 9,281 目录** | 同一脚本 |
| Rust 与 Python 一致性 | **400 文件** | 逐位比对 `i128` 纳秒 |
| 时区（时区无关，决定性） | **12 个 `$I` 文件** | 内部绝对 UTC FILETIME vs fskit mtime |
| 时区（本地时间往返，交叉印证） | **4 个 ZIP 成员** | DOS 本地时间 vs fskit mtime→Asia/Shanghai |
| FILETIME 解码器自校验 | **230 个 ZIP 成员 / 80 个 ZIP** | DOS 本地时间 vs `0x000A` NTFS 扩展字段 |
| 文件名规范化（决定性） | **12 个 `.tar.zst` / 16 处比对** | tar 头原始字节 vs `readdir` |
| 文件名规范化（分布） | **40,650 个名字** | `readdir` 假名统计 |
| 异常时间戳 | **全库 256,128 文件** | 全量 `find -newermt` |
| `ENOTSUP` 计数 | **全库 256,128 文件** | 同上，取 stderr |
| `ENOTSUP` 行为探测 | **40 个文件 / 12 个目录** | `stat`/`lstat`/`open`/`readdir`/`getattrlistbulk` |
| 写入模式（就地修改比例） | **4,577 文件** | `birthtime` 与 `mtime` 的差值分布 |
