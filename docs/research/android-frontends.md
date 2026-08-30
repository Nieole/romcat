# 安卓端/掌机端模拟器前端：游戏元数据格式与导入导出能力调研

> 调研日期：2026-08-31
> 调研对象：Beacon Game Launcher、Cocoon (Cocoon Shell)、iiSU、Daijishō、Pegasus Frontend (Android)
> 取材原则：只采信一手来源（官方仓库、官方 wiki/文档、官方发布说明、应用商店页面、可读源码）。
> 二手来源（媒体评测、第三方设置指南）在文中一律显式标注为「二手」，不作为结论依据。

---

## 0. 一句话结论（先看这个）

- **Cocoon**：**直接就是 ES-DE 标准**。导入/导出都是 `gamelists/<platform>/gamelist.xml` + `downloaded_media/<platform>/<mediatype>/`。**不需要独立适配器。**
- **Daijishō**：导入端复用 **ES gamelist.xml**（Skraper 产出）+ 图片文件夹；平台/模拟器配置是它自己的公开 JSON。**元数据侧不需要独立适配器**，但如果要生成"平台+启动器配置"则需要写它的 platform JSON（格式公开可读）。
- **iiSU**：通过 **"ES-DE Metadata Linking"** 直接挂载 ES-DE 目录复用其元数据与媒体。**元数据侧不需要独立适配器。**
- **Pegasus (Android)**：用自己的 `metadata.pegasus.txt`。**注意：Android 版编译时不包含 ES gamelist.xml 读取器**（有源码证据），所以"导出 ES gamelist"对 Pegasus 安卓版无效，必须导出 Pegasus 格式（但这是公开、有完整文档的标准格式）。
- **Beacon**：**唯一真正的黑盒**。闭源、付费、无官网、无公开文档、无社区逆向说明。目前**找不到任何权威来源**说明它的元数据存储格式或是否支持导入外部元数据。要支持它必须先逆向，当前不具备条件。

---

## 1. 三个"已有标准"基线（后面反复引用）

在判断"是否复用已有标准"之前，先把三个基线格式钉死。

### 1.1 EmulationStation 经典 `gamelist.xml`
- 一手来源：<https://github.com/Aloshi/EmulationStation/blob/master/GAMELISTS.md>
- 每个系统一个 `gamelist.xml`，`<game>` 节点含 `path` / `name` / `desc` / `image` / `video` / `rating` / `releasedate` / `developer` / `publisher` / `genre` / `players` 等字段。

### 1.2 ES-DE（EmulationStation Desktop Edition）目录约定
一手来源：ES-DE 官方用户手册 <https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>

- gamelist 位置：`~/ES-DE/gamelists/<system>/gamelist.xml`
  （手册原文：*"if you have gamelist.xml files in your ROMs directory tree then ES-DE will ignore those by default, so you need to move them to the ~/ES-DE/gamelists/ tree."*）
- 媒体位置：`~/ES-DE/downloaded_media/<system name>/<media type>`
  Android 上的实例路径（手册原文给出）：`/storage/emulated/0/ES-DE/downloaded_media/c64/screenshots/`
- 媒体子目录名（手册原文列表）：
  `3dboxes` / `backcovers` / `covers` / `custom` / `fanart` / `manuals` / `marquees` / `miximages` / `physicalmedia` / `screenshots` / `titlescreens` / `videos`
- 命名规则：媒体文件名必须与 ROM 文件**完全对应**（含子目录层级），例：
  ROM `~/ROMs/c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u`
  → `~/ES-DE/downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg`
- 图片扩展名支持 `.jpg` / `.png` / `.webp`，视频支持 `.mp4` / `.mkv` / `.avi` / `.wmv` / `.mov` / `.webm`。

### 1.3 Pegasus `metadata.pegasus.txt`
一手来源：<https://pegasus-frontend.org/docs/user-guide/meta-files/> 与 <https://pegasus-frontend.org/docs/user-guide/meta-assets/>

- 接受的文件名：`metadata.pegasus.txt`、`metadata.txt`、`*.metadata.pegasus.txt`、`*.metadata.txt`；可放在游戏目录，或集中放 `<config dir>/metafiles`。
- 语法：`name: value`，键名大小写不敏感；续行需以空格/Tab 开头；`#` 为注释。
- collection 字段：`collection`（必需）、`launch`/`command`、`workdir`/`cwd`、`extension`/`file`/`regex`/`directory`、`ignore-*`、`shortname`、`summary`、`description`、`sort-by`。
- game 字段：`game`（必需）、`file`/`files`、`developer`、`publisher`、`genre`、`tag`、`summary`、`description`、`players`、`release`（YYYY-MM-DD）、`rating`（0-100% 或 0-1）、`launch`/`command`、`workdir`。
- 媒体约定：`<dir>/media/<gamename>/<assettype>.<ext>`，`<gamename>` 可以是游戏标题或去扩展名的文件名；asset key 包括 `boxFront`/`box_front`/`boxart2D`、`boxBack`、`boxSpine`、`boxFull`、`cartridge`/`disc`/`cart`、`logo`/`wheel`、`poster`/`flyer`、`marquee`、`bezel`/`screenmarquee`/`border`、`panel`、`cabinetLeft/Right`、`tile`、`banner`、`steam`/`steamgrid`/`grid`、`background`、`music`、`screenshot`、`titlescreen`、`video`。图片 PNG/JPG，视频 WEBM/MP4/AVI，音频 MP3/OGG/WAV。

---

## 2. Beacon Game Launcher

### 2.1 它到底是什么
| 项 | 内容 | 来源 |
|---|---|---|
| 产品全名 | Beacon Game Launcher | Google Play |
| 包名 | `com.radikal.gamelauncher` | <https://play.google.com/store/apps/details?id=com.radikal.gamelauncher> |
| 开发者 | NERDS TAKE OVER | 同上（Play 商店开发者名） |
| 平台 | Android 7.0+ | Play 商店 |
| 开源/闭源 | **闭源**，未找到任何公开源码仓库 | 全网检索未见官方仓库 |
| 收费 | 付费买断，约 US$2.99（**注意**：Play 页面正文本次抓取失败，该价格来自多个二手来源一致陈述，包括 XDA 与 AppBrain） | 二手：<https://www.xda-developers.com/this-app-turned-my-old-android-phone-into-a-gaming-handheld/> |
| 官网/文档 | **未找到**。无官网、无 GitHub、无公开 wiki | — |

Play 商店官方文案（经 apkcombo 转录，原始 Play 页面因反爬未能完整抓取）：
> "Does not contain games or emulators. Beacon is an Android launcher designed to enhance your classic gaming experience. Add your preferred emulators, curate your retro game collection, and launch games directly from the app. Your retro game library is enriched by automatically scraping cover art and metadata."
> — <https://apkcombo.com/beacon-game-launcher/com.radikal.gamelauncher/>

用户点名的"Beacon"确认就是这个产品（安卓模拟器前端），不是同名的 usebeacon.app 或 GitHub 上的 `inthevortex/Beacon`。

### 2.2 元数据存在哪、什么格式
**未找到权威来源。** 没有官方文档说明其数据库/配置文件的位置与格式，也没有社区逆向出来的格式说明。

已知的（**二手**，来自第三方设置指南）：
- 有 `Settings → Backup & Restore`，会生成"单一文件"，包含平台配置、ROM 路径、主题偏好等核心设置；恢复时要求 ROM 目录结构一致。
  二手来源：<https://hypercombogamer.com/how-to-set-up-beacon-game-launcher-on-android/>
- 备份文件的具体格式（JSON / SQLite / 自定义二进制）**该指南也未说明**。

### 2.3 是否支持导入外部元数据
**未找到权威来源证明支持**。检索了 Play 商店更新日志（1.8.x 全系）、全网搜索"Beacon + gamelist / ES-DE / Skraper / import"，均无结果。1.8.x 更新日志内容全部是"新增模拟器支持 / RetroArch core / 字体"之类，无任何元数据导入项。

已知的手动兜底（**二手**）：长按游戏 → Edit → "Pick image" 逐个从相册/下载目录挑封面。
二手来源：<https://hypercombogamer.com/how-to-set-up-beacon-game-launcher-on-android/>

### 2.4 是否支持导出
除上述备份文件外，**未找到权威来源**说明存在面向其他前端的元数据导出。

### 2.5 媒体资源目录约定与命名
**未找到权威来源。**

### 2.6 抓取源
**二手**：ScreenScraper.fr + SteamGridDB，需要用户各自注册账号后在 Beacon 内填入。
二手来源：<https://www.joeysretrohandhelds.com/guides/beacon-game-launcher-setup-guide/>

### 2.7 是否复用已有标准
**未找到任何证据表明它复用 ES gamelist.xml 或 Pegasus 格式。** 从其"自动抓取封面与元数据"的产品定位看，更像是纯自扫描 + 内部库，但这是推断，不是结论。

---

## 3. Cocoon（Cocoon Shell / CocoonFE）

### 3.1 它到底是什么
| 项 | 内容 | 来源 |
|---|---|---|
| 产品全名 | Cocoon Shell（仓库名 CocoonFE） | <https://github.com/inssekt/CocoonFE> |
| 包名 | `rip.moth.cocoonshell` | README 中的 Obtainium 链接参数，同上 |
| 开发者 | inssekt | 同上 |
| 平台 | Android（官方称正在做 Linux / Windows 版） | README |
| 开源/闭源 | **闭源**。GitHub 仓库只有 README、`platforms/*.json`、横幅图与 Release APK，无源码、无 LICENSE | 仓库根目录内容 + GitHub API `license: null` |
| 开发者原话 | *"I never had intentions to open-source it, simply because it's my baby. However I have made a promise…if for whatever reason I am no able to support and update Cocoon, the source code will be made public."* | <https://cocoon-shell.com/news/post-2-0/> |
| 收费 | 免费（GitHub Releases 直接发 APK，站点有 Ko-fi 捐赠） | <https://github.com/inssekt/CocoonFE/releases> |
| 官网/文档 | <https://cocoon-shell.com/> ，wiki：<https://cocoon-shell.com/wiki/> | — |

> ⚠️ 更正一个流传的说法：部分聚合站/短视频稿件称 Cocoon 是 "free and open source"。**这是错的**，开发者本人已明确说明不开源。

自我定位（README 原文）：
> "Cocoon is an emulation frontend (similar to EmulationStation & Pegasus) which acts as an application to organise your library and launch games through Emulators."

### 3.2 元数据存在哪、什么格式 —— **关键发现：就是 ES-DE 格式**

来源：<https://cocoon-shell.com/wiki/backup-and-restore/>

Cocoon 的数据目录（首次设置时由用户自选，wiki 原文："Time to pick where Cocoon stores its downloaded artwork, logs, and other data"）下有：

```
Cocoon/
├── gamelists/            # 每平台一个 ES-DE 格式的 gamelist.xml
│   └── <platform>/gamelist.xml
├── downloaded_media/     # 与 ES-DE 同构
│   ├── snes/
│   │   ├── covers/
│   │   ├── logos/
│   │   └── heroes/
│   ├── gba/
│   │   └── ...
├── backups/              # 布局/网格备份，JSON，带时间戳
└── themes/
```

- gamelist 官方说明原文：*"These files follow the ES-DE format, so they're also compatible with other frontends like EmulationStation."*
- gamelist 覆盖的字段（wiki 列举）：titles、descriptions、ratings、play counts、play time、last played dates、favorites、hidden status、RetroAchievements IDs、emulator assignments。
- 布局备份是 JSON（网格坐标、文件夹、文件夹图、Android app tile，最多 5 层嵌套）。
- Cocoon **自身内部数据库**的文件位置/格式**官方文档未说明**（未找到权威来源）。但这不重要——对接只需要走 `gamelists/` 与 `downloaded_media/`。

平台短名（ROM 目录名）：官方兼容性页列出 122 个平台的 Folder Name，例 `nes` / `snes` / `n64` / `gb` / `psx` / `ps2` / `psp`。
来源：<https://cocoon-shell.com/wiki/compatibility/>
（官方页面**没有**明说这套短名与 ES-DE/RetroPie 完全一致，但从样例看是同一套惯例。）

### 3.3 是否支持导入外部元数据
**支持，而且是一等公民能力。**

1. **自动导入自己的 gamelists**：wiki 原文 *"During first-time setup, after Cocoon scans your ROM folders, it automatically checks for existing `gamelists/` in your data directory and imports any matching metadata."* 按**相对路径**匹配。
2. **ES-DE 迁移**：`Settings → Library & Data → ES-DE Migration`，指向 ES-DE 数据目录后可勾选：
   - *"Metadata — Game info from `gamelist.xml` files"*
   - *"Media — Artwork files from ES-DE's media directories"*
   媒体类型映射表（wiki 原文）：

   | ES-DE 类型 | Cocoon 类型 |
   |---|---|
   | Covers / Box Art | Icon（网格图块） |
   | Wheels / Logos | Logo（hero 叠加） |
   | Fan Art | Hero（hero 背景） |
   | Screenshots | Screenshot |

   限制（wiki 原文）：*"The import process matches games by their filename, so your ROMs need to have the same names in both setups."* 且 *"Importing won't overwrite data you already have in Cocoon. It only fills in what's missing."*
   来源：<https://cocoon-shell.com/wiki/es-de-migration/>
3. **单个游戏手动换图**：媒体选择器里可以 "upload your own image from your device"。
   来源：<https://cocoon-shell.com/wiki/scraping/>

**未找到**支持 Pegasus `metadata.pegasus.txt` 导入的证据。

### 3.4 是否支持导出
**支持。** `Settings → Library & Data → Export Metadata`，手动触发，**每个平台导出一个 ES-DE 兼容的 `gamelist.xml`**。
来源：<https://cocoon-shell.com/wiki/backup-and-restore/>

另有布局备份（JSON，写入 `backups/`）与主题/资源包（ZIP，Theme Studio 导出）。

### 3.5 平台/模拟器配置格式 —— **直接抄了 Daijishō**
`https://github.com/inssekt/CocoonFE/tree/main/platforms` 下每个平台一个 JSON，schema 与 Daijishō 的 platform JSON **完全同构**：

```json
{
  "databaseVersion": 14,
  "revisionNumber": 12,
  "platform": {
    "name": "Game Boy Advance",
    "uniqueId": "gba",
    "shortname": "gba",
    "acceptedFilenameRegex": "^(?!(?:\\._|\\.).*).*$",
    "scraperSourceList": ["LIBRETRO:Nintendo - Game Boy Advance", "DSESS:BOX_ART:TAGS(scraperKeyword):https://…"],
    "boxArtAspectRatioId": 0,
    "screenAspectRatioId": 3,
    "retroAchievementsConsoleIdList": [5]
  },
  "playerList": [
    { "name": "Pizza Boy GBA Basic",
      "uniqueId": "gba.it.dbtecno.pizzaboygba",
      "acceptedFilenameRegex": "^(.*)\\.(?:bin|gba|zip|7z)$",
      "amStartArguments": "-n it.dbtecno.pizzaboygba/… -e rom_uri {file.path} …" }
  ]
}
```
连 `databaseVersion: 14` 和 Daijishō 自创的 **DSESS** 抓取表达式都一模一样。README 致谢原文：
> "**Daijisho:** A great curated collection of platforms and players that we use as a base"
来源：<https://github.com/inssekt/CocoonFE#acknowledgments>

### 3.6 抓取源
LaunchBox、IGDB、HowLongToBeat、ScreenScraper、SteamGridDB（README）；wiki 抓取页重点讲 SteamGridDB + ScreenScraper，可按优先级排序。媒体分三类：Icon（方形封面）、Logo（透明标题艺术字）、Hero（宽幅背景）。
来源：<https://github.com/inssekt/CocoonFE#tools-and-integrations>、<https://cocoon-shell.com/wiki/scraping/>

### 3.7 是否复用已有标准
**是，强复用。** 元数据 = ES-DE `gamelist.xml`；媒体目录 = ES-DE `downloaded_media/<platform>/<mediatype>/` 布局（子目录名用自己的 `covers`/`logos`/`heroes`）；平台配置 = Daijishō platform JSON。

---

## 4. iiSU

### 4.1 它到底是什么
| 项 | 内容 | 来源 |
|---|---|---|
| 产品全名 | iiSU（"a visuals-first Android launcher designed for handhelds. Currently in Alpha."） | <https://github.com/iisu-network/iiSU> |
| 包名 | `com.iisulauncher` | README 中 Obtainium 链接参数，同上 |
| 开发者 | iisu-network（团队） | 同上 |
| 平台 | Android（Android-first） | README |
| 开源/闭源 | **闭源**。GitHub 仓库只有 README、`.github/` 资源、`updates/catalog.json`，**无源码、无 LICENSE** | 仓库根目录内容 + GitHub API `license: null` |
| 收费 | 免费（GitHub Releases 发 APK，另有 Ko-fi / Patreon 赞助） | <https://github.com/iisu-network/iiSU/releases> |
| 官网/文档 | <https://iisu.network/> ，wiki 在 `https://iisu.network/wiki/...`，另有资源库 <https://iidb.iisu.network/> | — |

中文社区可能写成 "iisu / IISU"，就是这个产品，确实是安卓掌机模拟器前端（XMB/PSP/3DS 风格 UI、WiiSU 模式、双屏支持、RetroAchievements）。

> ⚠️ 取材说明：`iisu.network` 在本次环境下**无法直接抓取**（TLS/反爬，WebFetch 与 curl 均失败）。因此下面所有结论优先引用 **GitHub 仓库 README 与 Release notes**（完全可核实的一手来源）；来自官网 wiki 的内容会标注"官网 wiki（本次仅通过检索摘要获得，未能逐字核验）"。

### 4.2 元数据存在哪、什么格式
**内部存储，格式未公开**（闭源，官方未发布数据库/文件格式说明）。README 提到有 "media storage location" 可由用户选择，onboarding 里有 "Asset Prep" 阶段 *"indexes your library and prepares your media"*。

### 4.3 是否支持导入外部元数据 —— **关键：ES-DE Metadata Linking**
一手来源：Release 0.0.6 发布说明 <https://github.com/iisu-network/iiSU/releases>

> **ES-DE Metadata Linking**
> - "Added an option to link an ES-DE install folder to enable asset sharing - making transitioning between emulators easy!"
> - "Note: ES-DE Linking does **not duplicate media files**, so any changes made in ES-DE will also sync to iiSU."
> - "You can still add or customise assets in iiSU whilst linking to ES-DE and it will not affect your ES-DE media"
> - "Added the ability to pre-cache ES-DE media to almost eliminate loading times when browsing your library"

后续版本继续强化：0.0.6.2 "Improved ES-DE sync and ES-DE related home flow"；0.0.7.2 "Added ES-DE metadata loading states"、"Improved ES-DE system/media matching across regional and family variants"、onboarding 覆盖 "…ROM folders, detected consoles, Retroachievements, scrapers, **ES-DE**, RomM, Discord…"。

README 的 onboarding 步骤 4（ROM Import）原文也确认：
> "you can select your ROM folder, and **link ES-DE metadata if you're changing frontends**. You can also change your iiSU media storage location… If you have a RomM server, you can link your server here as well. RomM support is currently an **experimental** feature."

其它导入通道：
- **Platform Packs**（平台级资源包，非游戏元数据）：ZIP，按平台文件夹组织，文件名 `icon.png` / `title.png` / `hero.png`；hero 也接受 `background.png/webp/jpg`；也支持 `nds_title.png` 这类命名。
  官网 wiki：<https://iisu.network/wiki/platform-packs>（本次仅通过检索摘要获得，未能逐字核验）
  一手佐证：Release 0.0.7.2 "Added platform pack import from ZIPs with layered icon/title/background assets."、0.0.6.2 "Platform Packs to allow you to import sets of platform assets alongside your themes."
- **模拟器配置文件手动导入**：0.0.7.2 "Added manual import for emulator config files with validation and rollback."
- **主题导入/导出**：0.0.4.1 "Theme Maker import / export — Export and import themes to share or move setups between devices."
- **RomM 服务器**（实验性）：0.0.6.2 "Experimental RomM integration <https://github.com/rommapp/romm>"

**未找到**支持 Pegasus 格式或直接吃裸 `gamelist.xml`（脱离 ES-DE 目录结构）的证据。

### 4.4 是否支持导出
**未找到权威来源**说明支持游戏元数据导出。已确认存在的只有：主题的导入/导出、日志导出（0.0.7.2 "Added log exports"）。

### 4.5 媒体资源目录约定与命名
- 平台级资源：见上面的 Platform Pack 约定（`<platform>/icon.png|title.png|hero.png`）。
- 游戏级媒体：官网有 `https://iisu.network/wiki/setting-up-assets`、`https://iisu.network/wiki/accepted-folder-names` 两页，但**本次环境无法抓取正文**，故**未能核验**具体的每游戏文件命名规则。
- RetroAchievements 相关：README 明确 ROM 被 RA 识别后，若无自定义图标则显示 RA 提供的 `ImageBoxArt.png`。

### 4.6 抓取源
ScreenScraper、SteamGridDB、TheGamesDB（0.0.7.4 起移除 IGDB）。
一手来源：Release 0.0.7.4 发布说明 "IGDB was removed; ScreenScraper, SteamGridDB, and TheGamesDB remain."

### 4.7 是否复用已有标准
**部分复用**：不是"导入一份拷贝"，而是**挂载 ES-DE 目录直接共享**其元数据与媒体（不复制文件、双向可见）。自身的持久化格式没有公开。

---

## 5. Daijishō（だいじしょう / 台字章）

### 5.1 它到底是什么
| 项 | 内容 | 来源 |
|---|---|---|
| 产品全名 | Daijishō | <https://github.com/TapiocaFox/Daijishou> |
| 包名 | `com.magneticchen.daijishou` | README 的 Play 链接 |
| 开发者 | TapiocaFox（+ Post-Mortem / Jetup13 / official-wizard） | README "The Team" |
| 平台 | Android | Play 商店 |
| 开源/闭源 | **闭源**。README 原文：*"Daijishō is currently **closed-source**. This repo is for assets and served as a main page."* 仓库的 MIT LICENSE 只覆盖这些资产文件，不覆盖 App | <https://github.com/TapiocaFox/Daijishou#about-this-repository> |
| 收费 | 免费（README：*"And Daijishō will always be free!"*） | 同上 |
| 官网/文档 | 仓库即主页；wiki：<https://github.com/TapiocaFox/Daijishou/wiki>；专门文档见 `docs/` | — |

> 注意：中文社区常见的 "Daijisho" 写法有两个域名 `daijisho.net` / `daijisho.com`，均**非**官方 GitHub 主页所指向的入口，本文只采信 GitHub 仓库与其 wiki。

### 5.2 元数据存在哪、什么格式
- **游戏级元数据：内部数据库，格式未公开**（闭源）。间接证据：platform JSON 顶层带 `"databaseVersion": 14` 字段，说明 App 侧存在一个有版本迁移的数据库；1.4 发布说明提到 *"Backup data migrations support has been introduced."* 但**官方未公布该数据库的具体形态**（SQLite/Room 是合理猜测，但**未找到权威来源**，不作结论）。
- **平台 + 播放器配置：公开的 platform JSON**，仓库 `platforms/*.json` 就是事实标准样例。结构见 §3.5（Cocoon 抄的就是它）。
  样例：<https://raw.githubusercontent.com/TapiocaFox/Daijishou/main/platforms/NintendoGameBoyAdvance.json>
- **播放器模板 `.dpt`**：纯文本，首行必须是 `# Daijishou Player Template` 或 `# DST`，正文为 `[tag_name] value`，用于给 `amStartArguments` 里的 `{tags.xxx}` 占位符供值（典型用途：Vita3k 的 `vita_game_id`、Tasker 任务名）。
  官方文档：<https://github.com/TapiocaFox/Daijishou/blob/main/docs/daijishou_player_template.md>
- **DSESS（Daijishō Search Engine Scraper Syntax）**：它自创的抓取表达式 DSL，`Headers : Template tags : DSESS URL`，Header 有 `DSESS:BOX_ART` / `SNAPSHOT` / `TITLE` / `YOUTUBE` / `DESCRIPTION` / `GENRES`；模板标签有 `scraperKeyword`、`platformName`、`localeLanguage` 等；URL 参数用 Jsoup CSS selector 抽取。**有完整官方文档**：<https://github.com/TapiocaFox/Daijishou/blob/main/docs/dsess.md>
- 隐藏偏好/调试控制台文档：<https://github.com/TapiocaFox/Daijishou/blob/main/docs/daijishou_console.md>

### 5.3 是否支持导入外部元数据 —— **支持 ES gamelist.xml**
一手来源：官方 wiki
- <https://github.com/TapiocaFox/Daijishou/wiki/Settings-Page>（Library → Import 区块）
- <https://github.com/TapiocaFox/Daijishou/wiki/How-to-Use-Daijish%C5%8D>（"Import Scraped Media" 章节）

| 入口 | 说明（wiki 原文/原意） |
|---|---|
| Import platform | 从生成的 JSON 文件导入平台（即 platform JSON） |
| Import favorite items | 从生成的 JSON 导入收藏项 |
| Import from Pegasus | *"Import generated config from 'Pegasus config generator'."* —— **注意：这是模拟器启动配置，不是游戏元数据** |
| **Import Metadata** | 路径：`Highlight Platform > 🖊 > Import Skraper UI generated gamelist.xml file or dat file > Select file` —— **直接吃 ES gamelist.xml，也吃 .dat** |
| **Import Preview Media** | 路径：`Highlight Platform > 🖊 > Import Preview Media > Select folder with images` —— 直接吃一整个图片文件夹 |

wiki 给出的官方推荐工作流就是：用 **Skraper UI**，Media 选 `Image` + `Box2D`，Game List 类型选 **`emulationstation gamelist.xml`**，生成后把 images 与 gamelist.xml 拷到设备再导入。

1.4 发布说明的一手佐证：*"You can now import external scraped files like one from Skraper for each platform."*
来源：<https://github.com/TapiocaFox/Daijishou/blob/main/release-notes/1_4_release_note.md>

### 5.4 是否支持导出
- **备份/还原**：1.4 引入模块化备份/还原、Google Drive 备份、备份数据迁移与还原策略；隐藏偏好 `use_lightweight_backup=true` 可跳过图片。**备份文件格式未公开**。
  来源：同上 1.4 release note；<https://github.com/TapiocaFox/Daijishou/blob/main/docs/daijishou_console.md>
- **未找到**导出 gamelist.xml / Pegasus 格式的能力。wiki 的 "Backup and Restore" 页标注 "(unfinished)"。

### 5.5 媒体资源目录约定与命名
- Daijishō 自己抓取的预览媒体存在 App 私有存储中（`Settings → Library → Remove all items preview media` 可整体清除；`Do not scrape preview media` 用于"自己已经准备好媒体"的场景）。**官方未公布该目录的路径与命名规则**（未找到权威来源）。
- 对外的媒体输入约定 = **Skraper/ES 那一套**：一个图片文件夹按 ROM 文件名匹配，通过 "Import Preview Media" 灌进去。

### 5.6 是否复用已有标准
**输入端复用**：ES gamelist.xml（Skraper 产出）+ 图片文件夹 + Logiqx `.dat`；模拟器配置可从 Pegasus config generator 导入。
**输出端不复用**：备份是自有格式，无标准导出。
**自有标准**：platform JSON、`.dpt`、DSESS ——**这三者都有公开文档/公开样例**，属于"闭源 App + 开放配置格式"。

---

## 6. Pegasus Frontend（Android 版）

### 6.1 它到底是什么
| 项 | 内容 | 来源 |
|---|---|---|
| 产品 | Pegasus Frontend | <https://pegasus-frontend.org/> |
| 开发者 | Mátyás Mustoha (mmatyas) | <https://github.com/mmatyas/pegasus-frontend> |
| 平台 | Windows / Linux / macOS / Raspberry Pi / **Android 5 (Lollipop) 及以上** | <https://pegasus-frontend.org/docs/user-guide/getting-started/> |
| 开源/闭源 | **开源**，GPLv3（含 §7 附加条款，限制商标/logo 商用） | <https://github.com/mmatyas/pegasus-frontend/blob/master/LICENSE.md> |
| 收费 | 免费 | 官网下载 |
| 官网/文档 | <https://pegasus-frontend.org/docs/> | — |

Android 安装方式：官网下 APK 手动安装（非 Play 商店），首次启动申请存储权限。

### 6.2 元数据存在哪、什么格式
- **游戏元数据 = `metadata.pegasus.txt`**（详见 §1.3），纯文本，放在游戏目录或 `<config dir>/metafiles`。
- **Android 上的配置目录**（源码 `src/backend/Paths.cpp`）：
  ```cpp
  #ifdef Q_OS_ANDROID
      const QString dir_path = QSP::writableLocation(QSP::GenericDataLocation)
                             + QStringLiteral("/pegasus-frontend");
  ```
  即 `/storage/emulated/0/pegasus-frontend`。此外 `configDirs()` 还会扫描每个存储根下的 `/pegasus-frontend`。
- 写出的文件（源码可查）：
  - `<config dir>/game_dirs.txt` —— ROM 目录列表（`AppSettings.cpp`）
  - `<config dir>/favorites.txt` —— 收藏（`providers/pegasus_favorites/Favorites.cpp`）
  - `<config dir>/stats.db` —— 游玩时长统计，**SQLite**（`providers/pegasus_playtime/PlaytimeStats.cpp`）

### 6.3 是否支持导入外部元数据 —— **Android 上有个大坑**
官方文档《Other sources》列出的第三方数据源：<https://pegasus-frontend.org/docs/user-guide/meta-sources/>

| 源 | 官方标注的可用平台 |
|---|---|
| Steam | Linux / Windows / macOS |
| GOG | Linux / Windows |
| **Android Apps** | **Android** |
| Lutris | Linux |
| **EmulationStation（读 `es_systems.cfg` + `gamelist.xml`）** | **Linux / Windows / macOS / 嵌入式** —— **不含 Android** |
| LaunchBox | Windows |
| **Skraper Assets** | **全平台** |

**源码验证**（这是最硬的证据）——`src/backend/providers/es2/CMakeLists.txt`：
```cmake
pegasus_add_provider(
    NAME "EmulationStation"
    CXXID ES2
    ...
    PLATFORMS
        WINDOWS
        MACOS
        X11
        EGLFS
)
```
旧的 qmake 构建同理：`win32|macx|defined(pclinux,var)|defined(armlinux,var): include(es2/es2.pri)` —— **`android:` 分支里没有 es2**。

结论：**Pegasus 的安卓版根本不编译 ES gamelist.xml 读取器，给它一份 gamelist.xml 是没用的。**

Android 上实际可用的 provider 只有：
- `pegasus_metadata`（`metadata.pegasus.txt`，ALL）
- `pegasus_media`（`media/<gamename>/<assettype>.<ext>`，ALL）
- `pegasus_favorites` / `pegasus_playtime`（ALL，内部）
- `android_apps`（ANDROID，扫描已安装 App）
- `logiqx`（Logiqx DAT，ALL）
- `skraper`（**ALL** —— 这是 Android 上唯一能吃"别人抓好的媒体"的通道）

### 6.4 Skraper Assets provider 的具体约定（安卓上可用）
源码 `src/backend/providers/skraper/SkraperAssetsProvider.cpp`：
- 会在每个 ROM 根目录下找这三个媒体目录之一：`/skraper/`、`/media/`、`/.media/`
- 目录名 → 资产类型映射（按优先级）：

| 子目录名 | 映射到 |
|---|---|
| `box2dfront` / `supporttexture` / `box3d` | BOX_FRONT |
| `box2dback` | BOX_BACK |
| `box2dside` | BOX_SPINE |
| `boxtexture` | BOX_FULL |
| `support` | CARTRIDGE |
| `wheel` / `wheelcarbon` / `wheelsteel` | LOGO |
| `screenshot` | SCREENSHOT |
| `screenshottitle` | TITLESCREEN |
| `screenmarquee` / `screenmarqueesmall` | ARCADE_MARQUEE |
| `fanart` | BACKGROUND |
| `steamgrid` | UI_STEAMGRID |
| `videos` | VIDEO |

- 匹配方式：按"去扩展名的绝对路径"与游戏文件对应。

### 6.5 是否支持导出
Pegasus **本身不导出**。官方文档提到有 ES ↔ Pegasus 的转换工具（见 <https://pegasus-frontend.org/docs/user-guide/meta-sources/> 与 issue <https://github.com/mmatyas/pegasus-frontend/issues/446>）。

### 6.6 是否复用已有标准
Pegasus 自己**就是**一个标准（`metadata.pegasus.txt` 有完整规范文档 + GPLv3 参考实现）。它在桌面端复用 ES gamelist.xml，在 **Android 端不复用**。

---

## 7. 汇总表

| 前端 | 元数据格式 | 是否复用已有标准 | 是否需要独立适配器 | 文档可得性 |
|---|---|---|---|---|
| **Beacon Game Launcher** | **未找到权威来源**（闭源、内部存储；仅知有单文件 Backup&Restore，格式未公开——二手来源） | **未找到证据**（既无 ES 也无 Pegasus 迹象） | **是**（且当前不可行：无格式、无逆向资料） | ❌ 无官网、无仓库、无文档、无社区逆向说明 |
| **Cocoon (Cocoon Shell)** | **ES-DE `gamelist.xml`**（`gamelists/<platform>/gamelist.xml`）+ `downloaded_media/<platform>/{covers,logos,heroes,…}`；布局备份 JSON；平台配置 = Daijishō platform JSON | **是（强复用）**：ES-DE 元数据 + ES-DE 媒体布局 + Daijishō 平台 JSON | **否** | ✅ 官方 wiki 明确写了导入/导出/目录结构；平台 JSON 在 GitHub 公开 |
| **iiSU** | 内部存储，格式未公开；对外通过 **ES-DE Metadata Linking** 直接挂载 ES-DE 目录（不复制媒体）；Platform Pack = ZIP（`<platform>/icon.png|title.png|hero.png`） | **是（部分）**：直接共享 ES-DE 的元数据与媒体；自身格式未公开 | **否**（走 ES-DE 布局即可） | ⚠️ 官方 wiki 存在但本次环境无法抓取；GitHub README + Release notes 可核实 |
| **Daijishō** | 游戏元数据：内部 DB，格式未公开；平台/播放器配置：**公开的 platform JSON**、`.dpt`、DSESS DSL | **是（输入端）**：吃 Skraper 产出的 **ES gamelist.xml** + 图片文件夹 + Logiqx `.dat`；可从 Pegasus config generator 导入模拟器配置。**输出端不复用** | **否**（元数据侧）；若还要生成"平台+启动器"配置，则需写 platform JSON（格式公开，成本低） | ✅ platform JSON / `.dpt` / DSESS / console 均有官方文档；备份格式无文档 |
| **Pegasus Frontend (Android)** | `metadata.pegasus.txt`（有完整规范）+ `media/<gamename>/<assettype>.<ext>`；配置在 `/storage/emulated/0/pegasus-frontend`（`game_dirs.txt`、`favorites.txt`、`stats.db`=SQLite） | **它自己就是标准**；**Android 版不读 ES gamelist.xml**（源码验证）；Android 可读 Skraper 媒体目录 | **是**（需要一个 Pegasus 格式写出器；但格式公开且简单） | ✅ 官方文档齐全 + GPLv3 源码可读 |

---

## 8. 结论：哪些能被"导出 ES gamelist.xml / 导出 Pegasus 格式"直接覆盖

### 8.1 一份 ES-DE 布局的导出就能覆盖三个前端
只要产出**标准 ES-DE 目录布局**：

```
<root>/
├── gamelists/<system>/gamelist.xml          # ES gamelist.xml
└── downloaded_media/<system>/<mediatype>/<rom基名>.<ext>
```

即可直接被：

1. **Cocoon** —— 两条路都通：① 把 `gamelists/` 放进 Cocoon 数据目录，首次扫描后自动导入（按相对路径匹配）；② 走 `Settings → Library & Data → ES-DE Migration` 指向该目录，勾选 Metadata + Media。
   - 媒体子目录名建议同时铺 ES-DE 标准名（`covers` / `marquees` / `fanart` / `screenshots`），Cocoon 的迁移向导有映射表把它们对到 Icon / Logo / Hero / Screenshot。
2. **iiSU** —— 走 "ES-DE Metadata Linking"，把该目录当作 ES-DE 安装目录挂上去，媒体不复制、直接共享。
3. **Daijishō** —— 走每平台的 "Import Skraper UI generated gamelist.xml"（+ "Import Preview Media" 灌图片文件夹）。注意 Daijishō 的官方推荐是 **Skraper 的 ES gamelist 输出**，所以 gamelist.xml 的字段和图片相对路径要按 Skraper/ES 惯例写。

> 换句话说：**Cocoon / iiSU / Daijishō 三个都不需要写专门的适配器**，一个「ES-DE 导出器」就全覆盖了。

### 8.2 必须单独写 Pegasus 导出器（但这是低成本、有文档的活）
**Pegasus 安卓版不读 gamelist.xml**（`es2` provider 在 Android 上根本不编译，见 §6.3 的源码证据）。所以想覆盖它，必须额外产出：
- `metadata.pegasus.txt`（字段见 §1.3），以及
- `media/<gamename>/<assettype>.png` 的媒体布局（或者退一步，铺一份 `media/box2dfront/` 之类的 **Skraper 布局**，Pegasus 的 Skraper provider 在 Android 上是启用的，这条路能省掉重命名成本）。

这是一个"多写一种输出格式"的工作，不是"逆向一个黑盒"的工作。

### 8.3 真正需要专门开发的：只有 Beacon —— 而且现在做不了
- 闭源、付费、无官网、无 GitHub、无公开文档、无社区逆向说明。
- Play 商店更新日志（1.8.x）里没有任何元数据导入/导出相关条目。
- 唯一已知的数据出入口是 `Settings → Backup & Restore` 的单个备份文件（**二手来源**），格式未公开。
- **建议**：暂不支持 Beacon。若一定要做，前置工作是：拿到该备份文件做格式分析 + 在设备上定位其数据库/媒体目录（需要真机 + 逆向），成本远高于其它四个之和。在拿到可靠格式说明之前，不要为它规划适配器。

### 8.4 额外建议（若还要覆盖"平台/模拟器启动配置"而不只是元数据）
- **Daijishō platform JSON** 是一个事实标准：**Cocoon 直接复用了它**（连 `databaseVersion: 14` 和 DSESS 都一样）。也就是说，写一个 Daijishō platform JSON 生成器，**同时覆盖 Daijishō 和 Cocoon 两家的平台/播放器配置**。
- iiSU 的 Platform Pack（`<platform>/icon.png|title.png|hero.png` 的 ZIP）是平台级美术资源，不是游戏元数据，可作为可选增强项。

---

## 9. 本次调研中未能取得权威来源的点（明确列出，不用推测填充）

1. **Beacon 的一切格式细节**：数据库位置/格式、媒体目录与命名、备份文件格式、是否支持任何形式的外部元数据导入 —— **全部未找到权威来源**。
2. **Beacon 的确切价格**：Play 商店页面正文本次抓取失败（反爬）；$2.99 来自多个二手来源，未逐字核验官方页面。
3. **iiSU 的游戏级媒体文件命名规则**：官网 `https://iisu.network/wiki/setting-up-assets` 与 `https://iisu.network/wiki/accepted-folder-names` 存在，但 `iisu.network` 在本次环境下无法抓取（TLS 握手失败 / 连接被关闭），**未能逐字核验**。建议后续在可访问网络环境下补齐这两页。
4. **iiSU 是否有任何元数据导出**：未找到权威来源（只确认了主题导出与日志导出）。
5. **Cocoon 内部数据库的文件位置与格式**：官方 wiki 未说明（但对接不需要它）。
6. **Daijishō 内部数据库与备份文件格式**：官方未公布；wiki 的 "Backup and Restore" 页标注 "(unfinished)"。
7. **Daijishō 自抓预览媒体在设备上的存放路径与命名**：官方未公布。
8. **Cocoon 的平台短名是否与 ES-DE 完全一致**：官方兼容性页只给了自己的 Folder Name 列表（122 个平台），**没有**声明与 ES-DE/RetroPie 一致；样例（`nes`/`snes`/`n64`/`gb`/`psx`/`ps2`/`psp`）看起来是同一套惯例，但若要逐平台映射，建议以官方表为准做一次比对。

---

## 10. 参考来源清单

**Beacon**
- Google Play：<https://play.google.com/store/apps/details?id=com.radikal.gamelauncher>
- 商店文案转录（apkcombo）：<https://apkcombo.com/beacon-game-launcher/com.radikal.gamelauncher/>
- 二手 · 设置指南：<https://www.joeysretrohandhelds.com/guides/beacon-game-launcher-setup-guide/>
- 二手 · 设置指南（备份/换图）：<https://hypercombogamer.com/how-to-set-up-beacon-game-launcher-on-android/>
- 二手 · XDA 报道：<https://www.xda-developers.com/this-app-turned-my-old-android-phone-into-a-gaming-handheld/>

**Cocoon**
- 仓库：<https://github.com/inssekt/CocoonFE>
- 平台 JSON 目录：<https://github.com/inssekt/CocoonFE/tree/main/platforms>
- Releases（仅 APK，无源码）：<https://github.com/inssekt/CocoonFE/releases>
- 官网：<https://cocoon-shell.com/>
- Wiki · Getting Started：<https://cocoon-shell.com/wiki/getting-started/>
- Wiki · Backup & Restore：<https://cocoon-shell.com/wiki/backup-and-restore/>
- Wiki · ES-DE Migration：<https://cocoon-shell.com/wiki/es-de-migration/>
- Wiki · Scraping：<https://cocoon-shell.com/wiki/scraping/>
- Wiki · Compatibility：<https://cocoon-shell.com/wiki/compatibility/>
- Wiki · Emulator Setup：<https://cocoon-shell.com/wiki/emulator-setup/>
- Wiki · FAQ：<https://cocoon-shell.com/wiki/faq/>
- 开发者关于开源的声明：<https://cocoon-shell.com/news/post-2-0/>

**iiSU**
- 仓库 / README：<https://github.com/iisu-network/iiSU>
- Releases（0.0.6 的 "ES-DE Metadata Linking"、0.0.7.x 的 ES-DE 改进）：<https://github.com/iisu-network/iiSU/releases>
- Issue tracker：<https://github.com/iisu-network/issues>
- 官网（本次无法抓取）：<https://iisu.network/> · FAQ <https://iisu.network/faq> · Wiki <https://iisu.network/wiki/setting-up-assets> · <https://iisu.network/wiki/accepted-folder-names> · <https://iisu.network/wiki/platform-packs>
- 资源库 iiDB：<https://iidb.iisu.network/>

**Daijishō**
- 仓库 / README：<https://github.com/TapiocaFox/Daijishou>
- 平台 JSON：<https://github.com/TapiocaFox/Daijishou/tree/main/platforms>
- 文档 · DSESS：<https://github.com/TapiocaFox/Daijishou/blob/main/docs/dsess.md>
- 文档 · Player Template (.dpt)：<https://github.com/TapiocaFox/Daijishou/blob/main/docs/daijishou_player_template.md>
- 文档 · Console：<https://github.com/TapiocaFox/Daijishou/blob/main/docs/daijishou_console.md>
- 1.4 发布说明：<https://github.com/TapiocaFox/Daijishou/blob/main/release-notes/1_4_release_note.md>
- Wiki · Settings Page：<https://github.com/TapiocaFox/Daijishou/wiki/Settings-Page>
- Wiki · How to Use（Import Scraped Media / Import Metadata）：<https://github.com/TapiocaFox/Daijishou/wiki/How-to-Use-Daijish%C5%8D>
- Wiki · FAQ：<https://github.com/TapiocaFox/Daijishou/wiki/Frequently-Asked-Questions>
- Play 商店：<https://play.google.com/store/apps/details?id=com.magneticchen.daijishou>

**Pegasus Frontend**
- 官网 / 文档：<https://pegasus-frontend.org/docs/>
- Metadata files：<https://pegasus-frontend.org/docs/user-guide/meta-files/>
- Asset files：<https://pegasus-frontend.org/docs/user-guide/meta-assets/>
- Other sources（第三方数据源与平台支持矩阵）：<https://pegasus-frontend.org/docs/user-guide/meta-sources/>
- Getting started（含 Android 安装）：<https://pegasus-frontend.org/docs/user-guide/getting-started/>
- 源码（GPLv3）：<https://github.com/mmatyas/pegasus-frontend>
  - provider 平台开关：`src/backend/providers/providers.pri`、`src/backend/providers/es2/CMakeLists.txt`
  - Android 配置目录：`src/backend/Paths.cpp`
  - Skraper 目录约定：`src/backend/providers/skraper/SkraperAssetsProvider.cpp`
  - `favorites.txt` / `stats.db`：`src/backend/providers/pegasus_favorites/Favorites.cpp`、`src/backend/providers/pegasus_playtime/PlaytimeStats.cpp`
- ES ↔ Pegasus 转换工具讨论：<https://github.com/mmatyas/pegasus-frontend/issues/446>

**基线格式**
- EmulationStation GAMELISTS.md：<https://github.com/Aloshi/EmulationStation/blob/master/GAMELISTS.md>
- ES-DE 用户手册（gamelists / downloaded_media 约定）：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>
