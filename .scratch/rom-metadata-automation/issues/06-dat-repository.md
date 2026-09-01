# 06 — DAT 仓库

**What to build:** 工具在本地维护多个哈希数据库的镜像与索引，并能告诉维护者每个平台有多少条可用记录。这是识别的弹药库。

有两条必须遵守的取数纪律：绝不直连 No-Intro 的 DAT 生成站点（一次参数畸形的请求就会导致永久 IP 封禁），以及 Redump 不能走那个 GitHub 镜像（镜像硬编码了已于 2026 年冻结的旧域名，会少掉数十个系统，且旧站会以「系统存在但无收录」的形式误导）。

**Blocked by:** 02

**Status:** done

- [x] No-Intro 的 DAT 走 GitHub 每日镜像获取 —— 源写在 `crates/core/src/dat/sources.toml`：
      取法「GitHub 发行资产」、仓库 `hugo19941994/auto-datfile-generator`、资产 `no-intro.zip`。
      **真机实测**（跑的是生产代码的列举 → 排计划 → 解析 → 入库）：一件整包展开成
      **48 份 DAT、76,429 条条目、156,628 条文件记录**，覆盖 31 个平台。抽查对得上调研：
      WS 257 / WSC 242、NGPC 128 / NGP 13、GBA 3,533、Lynx `.lnx` 18 与 `.lyx` 155。
      测试 `tests/dat_sync.rs::五个源一趟同步下来每个平台数得出多少条`、`整包那一档的映射作用在包里面`。

- [x] 代码中不存在任何指向 No-Intro DAT 生成站点的请求路径 —— `dat::guard` 是一道**运行时闸门**
      而不只是约定：主机白名单里没有任何 `no-intro.org`，`datomatic.no-intro.org` 另有点名拒绝
      （端口、用户信息、大小写、子域四种绕法都堵死）。闸门挂在两处：`sync::plan` 排计划时
      （**一个字节发出去之前**）与 `HttpFetcher` 的**每一跳**（自己跟重定向，就为了让每跳都过闸门）。
      测试 `guard::tests::绝不连_datomatic`、`名单里没有任何_no_intro_的生成站点`、
      `fetch::tests::一个_302_也送不到禁区去`、`sync::tests::计划这一层就把禁区拦下来`、
      `tests/dat_sync.rs::换掉数据源清单也绕不过取数纪律`（断言 `asked()` 里那个域名一次都没出现）、
      `cli/tests/dat.rs::导出的底稿里没有任何禁区地址`。

- [x] Redump 的 DAT 从其现行域名获取，不使用那个硬编码旧域名的镜像 —— 源写的是 `https://redump.info`；
      闸门拒 `redump.org` / `www.redump.org` / `old.redump.info`，并**按资产名**拒掉那个镜像的
      `redump.*`（`guard::check_asset`，在**读清单**这一步就报错，不必等到跑完）。
      **真机实测** 13 个系统 46,664 条，其中 **Wii U 541 条**——冻结旧站上这个系统显示
      「No discs found」——Xbox 360 3,707 条、3DO 669 条，三个数与调研在 `redump.info` 上量到的完全一致。
      测试 `guard::tests::绝不连_redump_的冻结镜像`、`镜像里的_redump_资产不许碰`、
      `registry::tests::清单把_redump_指到那个镜像上时读清单就失败`、
      `sync::tests::redump_的冻结镜像也在计划这一层被拦下`。

- [x] 一并纳入 MAME 的哈希数据、TOSEC 与 GoodNES —— **真机实测**：MAME 的 `hash/` software list
      23 份 27,485 条（许可最干净的一档，CC0）、TOSEC 309 份 61,347 条、GoodNES 2 份 30,244 条。
      三种格式各有解析器：`dat::logiqx`（Logiqx / No-Intro）、`dat::softlist`（MAME）、
      `dat::gamedb`（BizHawk 纯文本）。
      **带保留**：街机的 BIOS / device 判据（`mame -listxml` 的 `isbios` / `isdevice`）**不在 `hash/` 里**
      ——那是硬编码在 C++ 驱动源码中的另一个取数形态，本票未纳入，见挂账 **D44**（原 D35 的另一半）。

- [x] 从代码仓库拉取文件清单时不使用会在一千条处静默截断的接口 —— 闸门拒绝
      `api.github.com/repos/*/*/contents/*`；列举一律走 `git/trees?recursive=1`，**并且检查
      `truncated` 标志**，为真时当场停下（`SyncError::Truncated`）——只换接口不看那个标志，
      等于把同一个坑换个地方再踩。**真机实测**：TOSEC 一次列举拿到 10,649 个 blob（含 4,502 份 DAT），
      `truncated=false`；MAME 与 BizHawk 用 `git/trees/<分支>:<子树>` 只列那一棵子树，
      避开了 MAME 五万多个文件的全仓递归。
      测试 `guard::tests::会静默截断的那个接口不许用`、
      `tests/dat_sync.rs::文件清单被截断要当场停下而不是照单全收`。

- [x] 能报告每个平台可用的记录条数，含中文汉化条目数 —— `romcat dat report`（**一个字节都不联网**）。
      **真机实测：32 个平台、395 份 DAT、242,169 条条目、481,106 条文件记录、汉化 1,247 条、官中 884 条。**
      前几名：FC 48,644 条（汉化 1,010）、MD 21,143（汉化 146）、PSV 17,786（官中 346）、
      NDS 16,346、PS1 14,455、PS2 12,503、SFC 12,372（汉化 25）、GBA 11,292（汉化 13）。
      汉化与官中**分两列**（ADR-0012：官中版是发行版、汉化版是变体）：汉化全部来自
      TOSEC 499 + GoodNES 748，官中全部来自 No-Intro 467 + Redump 410 + TOSEC 7——
      合成一个数就看不出「TOSEC / GoodNES 补的正是官方库覆盖不到的那部分」。
      顺带更正了调研里两个数（都写进了 `sources.toml` 与模块文档）：GoodNES 的 MD「172 条」
      = 102 条 `[T±Chi]` 汉化 + 70 条 `(Ch)` 国别码，两者不能相加；TOSEC 的「960 条」每个平台
      都恰好是实测值的两倍——那个计数把 `<game name>` 与 `<description>` 各数了一次，按条目数是 499。
      测试 `report::tests::按平台与按源两个维度都数得出`、`文本报告把汉化与官中分两列`。

- [x] DAT 可增量更新，不必每次全量重下 —— 指纹三条来路：发行资产的 `updated_at`+大小、
      `Content-Disposition` 里的附件名（Redump 既不给 ETag 也不给 Last-Modified）、git blob sha。
      **真机实测第二趟：348 件全部「已是最新」，0 件重取、0 份 DAT 重写。**
      连 `--full` 也不必重下——取回来的原件按源缓存并带一个指纹旁证文件，实测整包与
      Redump 的 13 个 ZIP 全部命中缓存。指纹**问不出来时是 `None`**，那一份每趟重取；
      绝不拿固定占位串顶上，那会让它从此再也不更新而报告照报旧数。
      **带保留**：改一条 DAT→平台映射仍要 `--full` 才重新入库（整包的指纹粒度就是整包，
      数据源只发整包），见挂账 **D42**；不过重新入库只是重解，不再重下 106 MB。
      测试 `tests/dat_sync.rs::第二趟指纹没变就一件都不取`、`指纹变了就整件换掉不是叠加`、
      `全取一趟也不必把那个整包再下一遍`、`sync::tests::问不出指纹的那一份每趟都重取`。

## 落地形态

- 核心在 `crates/core/src/dat/`：`guard`（取数纪律的闸门）、`fetch`（唯一碰网络的接缝，
  测试用 `CannedFetcher` 全离线跑）、`registry` + `sources.toml`（**数据源清单是数据不是代码**，
  `--sources` 可整份换掉，`romcat dat sources --dump-builtin` 导底稿）、`logiqx` / `softlist` /
  `gamedb`（三种格式）、`chinese`（条目名里的汉化与官中标记）、`repo`（DAT 库）、
  `sync`（列举 → 排计划 → 取 → 入库，**排计划那一步是纯计算**）、`report`。
- 命令行：`romcat dat sync` / `romcat dat report` / `romcat dat sources`。
- **DAT 库与中立库分开存**（`工作目录/dat/dat.sqlite3`，实测 155 MB）：它与哪个主库无关，
  两块盘同步一次就够；而且中立库的结构版本一变就要删库重扫，不该赔上一百多 MB 的 DAT。
- **每份 DAT 带一个哈希口径**（含头 / 去头 / 逐芯片）。真机实测 12 份去头（NES headerless、
  SFC、FDS、Lynx LYX、Satellaview、Sufami Turbo，共 10,218 条）、360 份含头、23 份逐芯片。
  票 07 要同时算两套哈希，据这一列决定拿哪一套去撞。

## 挂账

新开 **D39**（DAT 引入的一批词还没进词表）、**D40**（TOSEC 按平台整族收）、
**D41**（缓存没有上限也不自清）、**D42**（整包的增量粒度是整个包）、
**D43**（libretro 那份 TOSEC 转换版没纳入）、**D44**（街机的 BIOS/device 判据，原 D35 的另一半）。
