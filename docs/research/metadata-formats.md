# 主流前端/启动器的游戏元数据格式及其相互转换可行性

> 调研日期：2026-08-30
> 方法：仅采用一手来源——各项目的官方文档站、官方 Wiki，以及 GitHub/GitLab 上的**实际源码**。
> 凡是从源码读出的结论，均给出文件级链接；凡是无法从一手来源证实的，明确标注为「未证实」或「推断」。
>
> **事实 / 推断标记约定**
> - 【事实】：可在给出的官方文档或源码中直接读到。
> - 【推断】：由事实推导得出，但一手来源未明说。
> - 【未证实】：找过一手来源但没找到明确依据。

---

## 目录

- **A. Pegasus 元数据格式（重点）** —— 文件位置 / 词法语法 / collection 与 game 段 / 字段取值域 / 资源查找 / 内置 provider / **用户状态不在元数据文件里**
- **B. 其它主流格式逐个梳理**
  - B.1 EmulationStation 家族（原版 / RetroPie、ES-DE）
  - B.2 Batocera 与 Recalbox
  - B.3 LaunchBox
  - B.4 Playnite
  - B.5 RetroArch `.lpl` 与缩略图
  - B.6 Attract-Mode romlist
  - B.7 Steam `shortcuts.vdf` 与 Steam ROM Manager
  - B.8 Skraper
  - B.9 其它（Daijishō / ArkOS / HyperSpin / GameEx / Logiqx）
- **C. 互转可行性分析**（核心产出）
  - C.1 字段映射矩阵
  - C.2 表达能力差异
  - C.3 媒体资源转换
  - C.4 路径与转义
  - C.5 用户状态数据
  - C.6 结论：往返无损的边界
- **D. 对通用元数据转换工具的可执行建议**
- 附录：来源清单

---

## 摘要：如果你只读五条

1. ⚠️ **Pegasus 的 `metadata.pegasus.txt` 里没有任何用户状态字段。** 收藏在 `<config>/favorites.txt`，游玩统计在 `<config>/stats.db`（SQLite，**逐次会话**记录）。只处理主文件的转换工具会 100% 丢失这些数据。（→ A.9）
2. ⚠️ **有三个「只此一家」的能力**：Pegasus 的 `regex`/`ignore-regex` 扫描规则与逐次游玩会话、Playnite 的日期精度建模、Batocera 的未知 XML 标签保留。转出这些格式时必然有损。（→ C.1）
3. ⚠️ **媒体的匹配键在各格式间来回横跳**：ROM 文件名（ES-DE）/ 游戏标题（LaunchBox）/ 播放列表 label（RetroArch）。且 RetroArch 与 LaunchBox 的文件名净化是**多对一的单向函数**——**绝不能从文件名反推标题**。（→ C.3、C.4）
4. ⚠️ **两个被广泛误传的技术细节，本文以源码更正**：RetroArch 缩略图净化的是 **11 个字符**（libretro 官方文档漏了双引号 `"`）；Playnite 现在用 **LiteDB `.db`**，`library/games/*.json` 早在格式版本 3 就废弃了。（→ B.5、B.4）
5. ⚠️ **往返无损不是格式的性质，而是转换器的性质**——没有任何一对主流格式天然往返无损，必须靠 sidecar 快照兜底。（→ C.6、D.3）

---

## A. Pegasus 元数据格式（重点）

Pegasus Frontend 的元数据由**一个文本文件**承担了别的前端里两个文件的职责：它既像 ES 的 `es_systems.cfg`（描述平台、扫描规则、启动命令），又像 ES 的 `gamelist.xml`（描述单个游戏）。官方文档原话：

> "Compared to ES files, the metadata file is like a combination of `es_systems.cfg` (as it contains platform data) and `gamelist.xml` (as it contains game data)."
> —— <https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/meta-sources.md>

### A.1 文件位置与命名

【事实】按优先级/兼容顺序，Pegasus 在**用户在设置里添加的游戏目录**中查找：

| 文件名 | 说明 |
|---|---|
| `metadata.pegasus.txt` | 首选名 |
| `metadata.txt` | 若上面不存在则回退 |
| `*.metadata.pegasus.txt` | 同目录可放多个，例如 `MarioGames.metadata.pegasus.txt` |
| `*.metadata.txt` | 同上 |

此外，若 `<config dir>/metafiles` 目录存在，Pegasus 也会在其中查找元数据文件（用于把元数据与 ROM 分离存放）。

来源：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/meta-files.md>

【事实】config dir 各平台路径：

| 平台 | 路径 |
|---|---|
| Linux | `~/.config/pegasus-frontend/` |
| Windows | `C:\Users\[username]\AppData\Local\pegasus-frontend\` |
| macOS | `~/Library/Preferences/pegasus-frontend/` |
| Android | `<storage>/Android/data/org.pegasus_frontend.android/files/pegasus-frontend/` |
| 全平台 | `<程序所在目录>/config`（portable 模式的默认位置） |

来源：<https://pegasus-frontend.org/docs/user-guide/config-dirs/>

### A.2 词法语法（严格定义）

【事实】官方把格式定义为**基于 Debian control file 格式**的变体，首选编码 UTF-8，首选换行 LF（U+000A）。

> "A metadata file is a human-readable text file based on the [Debian control file format]. The preferred file name is `metadata.pegasus.txt`, the preferred encoding is UTF-8 and the preferred line ending sequence is the standard line feed character (U+000A)."
> —— <https://github.com/mmatyas/pegasus-docs/blob/master/docs/dev/meta-syntax.md>

逐行解析规则（官方开发者文档 + 解析器源码 `MetaFile.cpp` 双向印证）：

1. **注释**：行**以 `#` 开头**（第 0 列）则整行忽略。
   源码：`if (line.startsWith('#')) continue;`
   ⚠️【事实】判断的是**未去空白的原始行**。因此**缩进后的 `#` 不是注释**，它会被当作上一条目的续行值。这是一个实现细节层面的坑。
2. **空行**：trim 后为空的行被忽略，**并且会关闭当前条目**（`close_current_attrib()`）。
3. **新条目**：行以非空白字符开头 → 期望 `name: value` 格式。在**该行第一个 `:` 处**切分；两侧各自 trim。左侧为 name（不得为空、不得含 `:`），右侧若非空则作为第一个值。
4. **续行**：行以空白开头且 trim 后非空 → 把 trim 后的内容追加为**上一条目的下一个值**。
5. **段落分隔**：续行内容恰为单个 `.` 时，追加一个 null 值，语义为「空行 / 段落分隔」。
   源码：`constexpr auto EMPTY_LINE_MARK = QChar('.');`

来源（源码）：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/parsers/MetaFile.cpp>

【事实】**键名一律小写化**。源码 `MetaFile.cpp` 第 128 行：

```cpp
entry.key = trimmed_line.left(key_end).trimmed().toString().toLower();
```

因此文档所说「`title`、`Title`、`TitLe` 等价」在实现上是通过 `toLower()` 达成的——**包括 `assets.*` 的资源名部分也会被小写化**（见 A.5 的重要推论）。

【事实】**多行文本合并规则**（`merge_lines()`，用于 `summary` / `description` / `launch`）：

- 各行已 trim；
- null 值（即 `.` 行）→ 输出 `\n\n`；
- 否则：若当前输出不以换行结尾，先补一个空格，再接本行；
- 最终整体 trim。

即：**普通换行会被折叠成空格**，只有 `.` 行才产生段落分隔。

【事实】**手动换行转义**：`\n` 字面量会被替换为真实换行；`\\n` 会被替换为字面量 `\n`。
源码 `PegasusMetadata.cpp`：

```cpp
rx_unescaped_newline(QStringLiteral(R"((?<!\\)\\n)"))
...
text.replace(rx_unescaped_newline, QStringLiteral("\n"))     // '\n'  -> [newline]
    .replace(QLatin1String(R"(\\n)"), QLatin1String(R"(\n)")); // '\\n' -> '\n'
```

【事实】**值不需要引号**，可以包含空格；`file` 条目明确说明「The entries don't have to be wrapped in quotes and can contain spaces」。
【推断】格式**没有通用转义机制**：值中可自由包含 `:`（因为只按第一个 `:` 切分），但**值的首行无法以空白开头**（会被当续行），且**行首 `#` 无法表示**。这对转换工具是硬约束。

### A.3 Collection 段可用键

【事实】以下取自解析器源码中的 `m_coll_attribs` 映射表，与官方文档一致。左列为**实际接受的字面量**（已小写）。

| 键（含别名） | 内部语义 | 类型 |
|---|---|---|
| `collection` | 段起始，值为集合名（**必填**） | 文本 |
| `shortname` | 短名（如 `snes`、`mame`），小写 | 文本 |
| `launch` / `command` | 集合级默认启动命令 | 文本（多行合并） |
| `workdir` / `cwd` | 启动工作目录 | 文本 |
| `directory` / `directories` | 额外搜索目录 | 列表 |
| `extension` / `extensions` | 逗号分隔扩展名（不含 `.`） | 列表 |
| `file` / `files` | 显式文件 | 列表 |
| `regex` | PCRE 正则（不带首尾斜杠） | 文本 |
| `ignore-extension` / `ignore-extensions` | 排除扩展名 | 列表 |
| `ignore-file` / `ignore-files` | 排除文件 | 列表 |
| `ignore-regex` | 排除正则 | 文本 |
| `summary` | 一段式简介 | 文本 |
| `description` | 长描述 | 文本 |
| `sort-by` / `sort_by` / `sortby` | 排序名 | 文本 |
| `assets.<类型>` / `asset.<类型>` | 集合级默认资源 | 见 A.5 |
| `x-*` | 扩展键 | 列表 |

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_metadata/PegasusMetadata.cpp>（`m_coll_attribs`）

【事实】`ignore-` 前缀是**在同一张表里复用同一个 enum**，靠 `entry.key.startsWith("ignore-")` 分流到 exclude 组：

```cpp
FileFilterGroup& filter_group = entry.key.startsWith(QLatin1String("ignore-"))
    ? ps.filters.back().exclude
    : ps.filters.back().include;
```

【事实】**排除强于包含**：官方文档明确「If both the regular and the `ignore-` fields match for a file, it will be excluded.」

【事实】**同名 collection 会合并**：「Collections can span over multiple metadata files *if they have the same name*.」

### A.4 Game 段可用键

【事实】取自 `m_game_attribs`：

| 键（含全部别名） | 内部语义 | 类型 |
|---|---|---|
| `game` | 段起始，值为标题（**必填**） | 文本 |
| `file` / `files` | 游戏文件（可多个，如多碟） | 列表 |
| `developer` / `developers` | 开发商 | 列表 |
| `publisher` / `publishers` | 发行商 | 列表 |
| `genre` / `genres` | 类型 | 列表 |
| `tag` / `tags` | 标签 | 列表 |
| `players` | 人数 | 文本 |
| `summary` | 简介 | 文本 |
| `description` | 长描述 | 文本 |
| `release` | 发行日期 | 文本 |
| `rating` | 评分 | 文本 |
| `launch` / `command` | 游戏级启动命令（覆盖 collection 的） | 文本（多行合并） |
| `workdir` / `cwd` | 工作目录 | 文本 |
| `sort-by` / `sort_by` / `sortby` / `sort-title` / `sort_title` / `sorttitle` / `sort-name` / `sort_name` / `sortname` | 排序标题 | 文本 |
| `assets.<类型>` / `asset.<类型>` | 资源 | 见 A.5 |
| `x-*` | 扩展键 | 列表 |

⚠️ 注意：**`sort-by` 的别名多达 9 个**（源码注释为 "sort name variations"），转换工具读取时应全部接受，写出时建议统一用 `sort-by`。

⚠️ 注意：**没有 `title` 键**。标题只能由 `game:` 行本身给出。文档里"名称大小写不敏感"举的 `title` 例子只是在讲词法，不代表存在 `title` 属性。

【事实】**未识别的键会被警告并忽略**：
`print_warning(ps, entry, LOGMSG("Unrecognized game property `%1`, ignored").arg(entry.key));`

【事实】**游戏出现条件**：「Only games that belong to at least one collection and have at least one existing file will appear in Pegasus.」

⚠️【事实】**一个 `game` 会被加入到该文件中此前定义过的所有 collection**，而非仅最后一个。源码：

```cpp
// Add to the ones found so far
for (model::Collection* const coll : ps.all_colls)
    sctx.game_add_to(*ps.cur_game, *coll);
```

这是一条容易误解的语义：在同一个 metadata 文件里先写 `collection: A` 再写 `collection: B`，之后所有 `game:` 同时属于 A 和 B。

### A.5 字段语义与取值域（本节回答「rating 是 0-1 还是百分比」等问题）

全部取自解析器源码中的正则与转换逻辑，比文档更精确。

#### `rating`
【事实】源码：
```cpp
rx_percent(QStringLiteral("^\\d+%$"))
rx_float (QStringLiteral("^\\d(\\.\\d+)?$"))
```
- 匹配 `^\d+%$` → 取百分号前数字 **/100** 存为 float；
- 否则匹配 `^\d(\.\d+)?$` → 直接存为 float；
- 都不匹配 → 警告 "Failed to parse the rating value"，**不写入**。

⚠️ **内部存储恒为 0.0–1.0 的浮点数**（主题 API 文档：「Floating-point value between and including `0.0` and `1.0`」）。
⚠️ **浮点写法的正则只允许小数点前一位数字**（`^\d(...)`）。因此 `0.85`、`1.0` 合法，而 `10`、`85` 这类无 `%` 的整数**两条正则都不匹配**，会被丢弃。转换工具写出时**务必带 `%` 或写成 `0.xx`**。

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_metadata/PegasusMetadata.cpp>
主题 API：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/themes/api.md>

#### `release`
【事实】源码正则：`^(\d{4})(-(\d{1,2}))?(-(\d{1,2}))?$`，即 `YYYY` / `YYYY-MM` / `YYYY-MM-DD`（月日允许 1–2 位）。
解析后：`y=max(1,Y)`，`m=clamp(M,1,12)`，`d=clamp(D,1,31)`，构造 `QDate`。

⚠️ **精度信息在解析后丢失**：只写 `1985` 会被补成 `1985-01-01`，模型里没有「只精确到年」的标记。主题 API 暴露 `releaseYear` / `releaseMonth` / `releaseDay`，但无从区分「1 月 1 日」与「仅知道年份」。这是 Pegasus 侧的**有损点**。

#### `players`
【事实】源码正则 `^(\d+)(-(\d+))?$`，然后：
```cpp
ps.cur_game->setPlayerCount(std::max(a, b));
```
⚠️ **模型里只存一个整数（最大人数）**。`1-4` → `4`。主题 API：「`players` | Maximum number of players. If not set, defaults to 1.」
⚠️ 因此 **`players` 的区间信息是写入即丢失的**——即使源格式（如 ES）有 `1-4`，Pegasus→X 往返时下界必然丢失。

#### `x-*` 扩展键
【事实】源码 `apply_extra_entry_maybe()`：
- 仅当键以 `x-` 开头才进入；
- 去掉 `x-` 前缀后作为 key；空则警告丢弃；
- 值以 **QStringList（多行列表）** 存入 `extraMapMut()`；
- **collection 与 game 都支持**。

主题侧访问方式：`game.extra.something` 对应 `x-something`。
来源：主题 API「`extra` | An object containing any extra properties (ie. `x-`) set in metadata files.」

⚠️ 这是**转换工具保留不可映射字段的官方通道**，官方文档明说用途就是给 scraper 存程序私有数据。

#### 资源键 `assets.*`
【事实】源码正则：`rx_asset_key(QStringLiteral(R"(^assets?\.(.+)$)"))`
→ **`asset.` 与 `assets.` 前缀都接受**（文档只提 `assets.`）。

【事实】值可以是：
- `http://` / `https://` 开头 → 直接当远程 URL；
- 否则按相对（相对于元数据文件所在目录）或绝对路径解析为 `file://` URL。

【事实】**同一资源类型可以有多个值**（多行），主题侧通过 `boxFrontList` 等数组字段访问。

【事实】**未知资源类型会被警告并忽略**：`"Unknown asset type `%1`, entry ignored"`。

### A.6 资源类型全表与查找规则

#### 内部枚举（规范类型，共 22 种）
【事实】`src/backend/types/AssetType.h`：

```
BOX_FRONT, BOX_BACK, BOX_SPINE, BOX_FULL, CARTRIDGE, LOGO, POSTER,
ARCADE_MARQUEE, ARCADE_BEZEL, ARCADE_PANEL, ARCADE_CABINET_L, ARCADE_CABINET_R,
UI_TILE, UI_BANNER, UI_STEAMGRID, BACKGROUND, MUSIC,
SCREENSHOT, TITLESCREEN, VIDEO
```
（另有 `UNKNOWN`。）

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/types/AssetType.h>

#### 名称 → 类型映射（含全部别名）
【事实】`src/backend/PegasusAssets.cpp` 的 `str_to_type()`：

| 规范类型 | 接受的名称 | 主题 API 属性名 |
|---|---|---|
| BOX_FRONT | `boxfront`, `boxFront`, `box_front`, `boxart2D`, `boxart2d` | `boxFront` |
| BOX_BACK | `boxback`, `boxBack`, `box_back` | `boxBack` |
| BOX_SPINE | `boxspine`, `boxSpine`, `box_spine`, `boxside`, `boxSide`, `box_side` | `boxSpine` |
| BOX_FULL | `boxfull`, `boxFull`, `box_full`, `box` | `boxFull` |
| CARTRIDGE | `cartridge`, `disc`, `cart` | `cartridge` |
| LOGO | `logo`, `wheel` | `logo` |
| POSTER | `poster`, `flyer` | `poster` |
| ARCADE_MARQUEE | `marquee` | `marquee` |
| ARCADE_BEZEL | `bezel`, `screenmarquee`, `border` | `bezel` |
| ARCADE_PANEL | `panel` | `panel` |
| ARCADE_CABINET_L | `cabinetleft`, `cabinetLeft`, `cabinet_left` | `cabinetLeft` |
| ARCADE_CABINET_R | `cabinetright`, `cabinetRight`, `cabinet_right` | `cabinetRight` |
| UI_TILE | `tile` | `tile` |
| UI_BANNER | `banner` | `banner` |
| UI_STEAMGRID | `steam`, `steamgrid`, `grid` | `steam` |
| BACKGROUND | `background` | `background` |
| MUSIC | `music` | `music` |
| SCREENSHOT | `screenshot`, `screenshots` | `screenshot` |
| TITLESCREEN | `titlescreen` | `titlescreen` |
| VIDEO | `video`, `videos` | `video` |

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/PegasusAssets.cpp>

⚠️【事实】`str_to_type()` 在**精确匹配失败后会做前缀匹配**：

```cpp
for (const auto& it : map) {
    if (str.startsWith(it.first))
        return it.second;
}
```

因此 `boxFront01.png`、`screenshot-02.jpg` 之类**带序号后缀的文件名同样会被识别**。这正是「多资源」在 media 目录里的表达方式。
⚠️【推断】由于前缀匹配遍历的是 `HashMap`，**遍历顺序不确定**；当一个名称同时是多个键的前缀时（例如 `box` 是 `boxfront` 的前缀，`boxart2d`/`boxback`/`boxfull` 也都以 `box` 开头），精确匹配优先保证了 `box` → BOX_FULL，但形如 `boxa...` 的自定义名可能落到不确定的类型上。转换工具应只写规范名或规范名 + 数字后缀。

#### media 目录约定
【事实】`MediaProvider.cpp`：

- 扫描的子目录名：**`/media` 和 `/.media`**（文档只提 `media`，源码还支持隐藏目录 `.media`）。
- 递归扫描（`QDirIterator::Subdirectories | FollowSymlinks`）。
- 匹配键：把资源文件所在**目录路径**中的 `/media` 段去掉后，与下面两种 key 比对：
  - `<游戏文件所在目录>/<游戏文件不含扩展名的名字>`
  - `<游戏文件所在目录>/<游戏标题>`
- 资源类型由**文件名的 completeBaseName** 经 `str_to_type()` 判定，且扩展名须在白名单内。

即官方示例的结构：
```
NES/
├─ metadata.pegasus.txt
├─ Contra (U).zip
└─ media/
   └─ Contra (U)/
      ├─ boxFront.jpg
      ├─ logo.jpg
      └─ video.mp4
```

【事实】扩展名白名单（**源码比文档更宽**）：

| 类别 | 源码 `MediaProvider.cpp` | 文档 `meta-assets.md` |
|---|---|---|
| 图片 | `png`, `jpg`, **`webp`**, **`apng`** | PNG、JPG |
| 视频 | `webm`, `mp4`, `avi` | WEBM、MP4、AVI |
| 音频 | `mp3`, `ogg`, `wav` | MP3、OGG、WAV |

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_media/MediaProvider.cpp>

⚠️【推断】以文件名为准的匹配意味着：**media 目录里的文件名大小写敏感性取决于文件系统**，但 `str_to_type()` 本身对 `boxFront` / `boxfront` 都注册了条目，所以两种写法都工作。而在 **metadata 文件里**，由于键被 `toLower()`，`assets.boxFront` 实际查表用的是 `boxfront`——同样命中。两条路径都安全。

#### 资源优先级
【事实】官方列出的资源来源顺序：
1. 元数据文件中为该游戏手动指定的文件
2. 元数据文件中为 collection 指定的默认文件
3. `<directory>/media/<gamename>/` 下的约定命名文件
4. 第三方来源（EmulationStation、Steam、Skraper）

来源：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/meta-assets.md>

### A.7 启动命令占位符

【事实】

| 变量 | 含义 | 示例 |
|---|---|---|
| `{file.path}` | 绝对路径 | `/home/joe/games/mygame.bin` |
| `{file.uri}` | URI 形式绝对路径 | `file:///home/joe/games/mygame.bin` |
| `{file.name}` | 文件名 | `mygame.bin` |
| `{file.basename}` | 去扩展名文件名 | `mygame` |
| `{file.dir}` | 所在目录 | `/home/joe/games` |
| `{env.MYVAR}` | 环境变量 | — |

【事实】「the variables will be replaced as-is, without additional formatting. You might need to wrap them in quotes if necessary.」——**不自动加引号，不自动转义**。转换工具生成含空格路径的命令时必须自己加引号。

【事实】Android 破坏性变更：Alpha 15 起 `file://` 前缀参数不再可用（`FileUriExposedException`），应改用 `{file.path}` 或 `{file.uri}`。
来源：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/breaking-changes.md>

### A.8 Pegasus 的内置 provider（原生兼容性）

【事实】源码目录 `src/backend/providers/` 下的 provider 列表：

| 目录 | Provider | 说明 |
|---|---|---|
| `pegasus_metadata` | Pegasus Metadata | 解析 `metadata.pegasus.txt` |
| `pegasus_media` | Pegasus Media | 扫描 `media/`、`.media/` |
| `pegasus_favorites` | Pegasus Favorites | 收藏（内部） |
| `pegasus_playtime` | Pegasus Playtime | 游玩统计（内部） |
| `es2` | EmulationStation | 读 `es_systems.cfg` + `gamelist.xml` |
| `steam` | Steam | 已安装 Steam 游戏 |
| `gog` | GOG | Windows / Linux |
| `launchbox` | LaunchBox | Windows |
| `playnite` | Playnite | Windows |
| `lutris` | Lutris | Linux（官方标注 experimental） |
| `logiqx` | Logiqx | Logiqx DAT |
| `android_apps` | Android Apps | Android 已安装应用 |
| `skraper` | Skraper Assets | 识别 Skraper 的 media 目录布局 |

来源：<https://github.com/mmatyas/pegasus-frontend/tree/master/src/backend/providers>

#### 关于「Pegasus 能直接读 ES gamelist.xml 吗？」
【事实】**能，但是只读**，且有明确限制。官方原话：

> "If you have EmulationStation installed and set up, Pegasus will also check the directories set in `es_systems.cfg`, read the `gamelist.xml` files in them, and use the metadata and assets defined there. **Note that there are several, mutually incompatible EmulationStation variants out there; Pegasus supports only the last official ES release.**"

【事实】从 `Es2Metadata.cpp` 源码可读出 Pegasus **实际只识别这 15 个 ES 字段**：

`path`, `name`, `desc`, `developer`, `genre`, `publisher`, `players`, `rating`,
`playcount`, `lastplayed`, `releasedate`, `image`, `video`, `marquee`, `favorite`

其它 ES/ES-DE/Batocera 字段（如 `sortname`, `hidden`, `kidgame`, `region`, `lang`, `thumbnail`, `md5`, `gametime` 等）**不被读取**。

日期解析格式：`yyyyMMdd'T'HHmmss`
人数正则：`(\d+)(-(\d+))?`
`favorite` 判真：`yes` / `true` / `1`（前两者大小写不敏感）

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/es2/Es2Metadata.cpp>

#### Skraper 资源目录映射
【事实】`SkraperAssetsProvider.cpp` 中，Pegasus 扫描 `/skraper/`、`/media/`、`/.media/` 三个目录名，并按下表把 Skraper 的目录名映射到自己的资源类型（同一类型的多个目录名**按优先级排序**）：

| Pegasus 类型 | Skraper 目录名（按优先级） |
|---|---|
| ARCADE_MARQUEE | `screenmarquee`, `screenmarqueesmall` |
| BACKGROUND | `fanart` |
| BOX_BACK | `box2dback` |
| BOX_FRONT | `box2dfront`, `supporttexture`, `box3d` |
| BOX_FULL | `boxtexture` |
| BOX_SPINE | `box2dside` |
| CARTRIDGE | `support` |
| LOGO | `wheel`, `wheelcarbon`, `wheelsteel` |
| SCREENSHOT | `screenshot` |
| TITLESCREEN | `screenshottitle` |
| UI_STEAMGRID | `steamgrid` |
| VIDEO | `videos` |

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/skraper/SkraperAssetsProvider.cpp>

⚠️ 这张表本身就是一份**官方认可的跨格式资源映射**，转换工具可以直接复用。

### A.9 ⚠️ 关键：用户状态数据**不在** metadata 文件里

这是 Pegasus 格式最重要的结构性事实，直接决定了往返无损的边界。

【事实】**收藏**存在独立文件 `<config dir>/favorites.txt`：
- 纯文本，**每行一个路径**；
- 写出时带头注释 `# List of favorites, one path per line`；
- 非 portable 模式写绝对路径，portable 模式写相对于 config 目录的相对路径。

源码：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_favorites/Favorites.cpp>

【事实】**游玩统计**存在 SQLite 数据库 `<config dir>/stats.db`，schema 为：

```sql
CREATE TABLE paths(
  id INTEGER PRIMARY KEY,
  path TEXT UNIQUE NOT NULL
);
CREATE TABLE plays(
  id INTEGER PRIMARY KEY,
  path_id INTEGER NOT NULL REFERENCES plays(id),
  start_time INTEGER NOT NULL,
  duration INTEGER NOT NULL
);
```

即**每次游玩一条记录**（起始时间戳 + 时长秒数），`playCount` / `playTime` / `lastPlayed` 是聚合出来的。

源码：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_playtime/PlaytimeStats.cpp>

⚠️ 推论：
1. `metadata.pegasus.txt` 里**没有** `favorite` / `playcount` / `lastplayed` / `playtime` 这些键，写了也会被当作未识别属性忽略。
2. Pegasus 的游玩记录**粒度比其它所有格式都细**（逐次会话），转成 ES/LaunchBox 的 `playcount + lastplayed` 是**降采样有损**，反向则无法还原。
3. 键是**文件路径**，不是游戏 ID —— 路径一变（例如换盘符、改目录）统计即失联。

### A.10 Pegasus 官方转换工具

【事实】官方提供一个网页转换器 <http://pegasus-frontend.org/tools/convert/>，在两处被官方文档引用：
- `meta-sources.md`：「A tool for converting between ES and Pegasus files can be found HERE」——即 **ES ↔ Pegasus**；
- `breaking-changes.md`：用于把 **Pegasus (alpha 10) `metadata.txt` → 当前 Pegasus `metadata.txt`**（下拉项名为 `Pegasus (alpha 10) -- metadata.txt` 与 `Pegasus -- metadata.txt`）。

【未证实】该页面的格式下拉列表由 JS 动态填充，抓取静态 HTML 只能得到占位项，因此**完整的输入/输出格式清单未能从一手来源枚举**。

【事实】另有官方图形化元数据编辑器：<https://github.com/mmatyas/pegasus-metadata-editor>（由 `meta-files.md` 引用）。

---

## B. 其它主流格式逐个梳理

### B.1 EmulationStation 家族 `gamelist.xml`

四个主要变体共享同一个骨架，但**分歧极大**，尤其在「媒体如何定位」上。

```xml
<gameList>
    <game>
        <path>./mm2.nes</path>
        <name>Mega Man 2</name>
    </game>
    <folder>
        <path>./Multidisk</path>
        <name>Multi-disc games</name>
    </folder>
</gameList>
```

【事实】共性：
- 根元素 `<gameList>`（**大写 L，大小写敏感**）；
- 子元素只有 `<game>` 与 `<folder>`，其余被跳过；
- `<path>` 是条目标识，其余子元素是元数据，**一律以字符串编码**；
- **值等于默认值时不写出**；
- 四个代码库**都用 pugixml** 解析；
- ⚠️ **四者都没有官方 XSD / DTD / RELAX NG**。格式只由各自的 `MetaData.cpp` 与散文文档定义。

#### 原版 / RetroPie EmulationStation

来源：<https://github.com/RetroPie/EmulationStation/blob/master/es-app/src/MetaData.cpp>、<https://github.com/RetroPie/EmulationStation/blob/master/GAMELISTS.md>

字段声明结构：`{ key, MetaDataType, defaultValue, isStatistic, displayName, displayPrompt }`
类型枚举：`MD_STRING`、`MD_INT`、`MD_FLOAT`、`MD_BOOL`、`MD_MULTILINE_STRING`、`MD_PATH`、`MD_RATING`、`MD_DATE`、`MD_TIME`
（`MD_FLOAT` 已声明但**无任何字段使用**。）

**`<game>` —— 18 个字段（序列化顺序）**

| # | 标签 | 类型 | 默认值 | statistic |
|---|---|---|---|---|
| 1 | `name` | MD_STRING | `""` | 否 |
| 2 | `sortname` | MD_STRING | `""` | 否 |
| 3 | `desc` | MD_MULTILINE_STRING | `""` | 否 |
| 4 | `image` | MD_PATH | `""` | 否 |
| 5 | `video` | MD_PATH | `""` | 否 |
| 6 | `marquee` | MD_PATH | `""` | 否 |
| 7 | `thumbnail` | MD_PATH | `""` | 否 |
| 8 | `rating` | MD_RATING | `"0"` | 否 |
| 9 | `releasedate` | MD_DATE | `"not-a-date-time"` | 否 |
| 10 | `developer` | MD_STRING | `"unknown"` | 否 |
| 11 | `publisher` | MD_STRING | `"unknown"` | 否 |
| 12 | `genre` | MD_STRING | `"unknown"` | 否 |
| 13 | `players` | MD_INT | `"1"` | 否 |
| 14 | `favorite` | MD_BOOL | `"false"` | 否 |
| 15 | `hidden` | MD_BOOL | `"false"` | 否 |
| 16 | `kidgame` | MD_BOOL | `"false"` | 否 |
| 17 | `playcount` | MD_INT | `"0"` | **是** |
| 18 | `lastplayed` | MD_TIME | `"0"` | **是** |

**`<folder>` —— 13 个字段**：`name`、`sortname`、`desc`、`image`、`thumbnail`、`video`、`marquee`、`rating`、`releasedate`、`developer`、`publisher`、`genre`、`players`
（无 `favorite`/`hidden`/`kidgame`/`playcount`/`lastplayed`；`developer`/`publisher`/`genre` 默认为 `""` 而非 `"unknown"`。）

**取值编码**
- `rating`：float **0.0–1.0**，源码只 clamp **不取整**，所以四分之一星等任意小数可以保留；
- `releasedate` / `lastplayed`：`"%Y%m%dT%H%M%S"`，如 `19950311T000000`；
- 布尔：字面 `"true"` / `"false"`；
- ⚠️ **本分支没有「游玩时长」字段**，只有 `playcount`；
- 路径字段读入时解析、写出时尽量改写为 `./x.png`（相对系统 ROM 目录）或 `~/x.png`。

**gamelist.xml 查找顺序**（`SystemData::getGamelistPath`）
1. `[SYSTEM_PATH]/gamelist.xml`（与 ROM 同目录）
2. `~/.emulationstation/gamelists/[SYSTEM_NAME]/gamelist.xml`
3. `/etc/emulationstation/gamelists/[SYSTEM_NAME]/gamelist.xml`

⚠️ **读写不对称**：读优先 ROM 目录，但**写入默认去 `~/.emulationstation/gamelists/`**，除非 ROM 目录里已存在文件。

⚠️【事实】**`<provider>` 元素在 RetroPie ES 中并不存在**（全仓库 grep 零命中）。若在野外遇到，是第三方工具写的；ES 读取时忽略，**重写时静默丢弃**——因为 `updateGamelist()` 会整个重建 `<game>` 节点。

【事实】系统定义文件是 **`es_systems.cfg`（.cfg 扩展名，内容是 XML）**，查找 `~/.emulationstation/es_systems.cfg` 然后 `/etc/emulationstation/es_systems.cfg`。元素：`<name>`、`<fullname>`、`<path>`、`<extension>`、`<command>`、`<platform>`、`<theme>`，外加 RetroPie 特有的 `<defaultCore>`。命令变量：`%ROM%`、`%BASENAME%`、`%ROM_RAW%`。

【事实】自定义收藏：`~/.emulationstation/collections/custom-<名字>.cfg`，**每行一个绝对路径**——⚠️ **本分支没有 `%ROMPATH%` 变量**（这点与 ES-DE 不同）。


#### ES-DE（EmulationStation Desktop Edition）

| 项目 | 内容 |
|---|---|
| gamelist 位置 | `~/ES-DE/gamelists/<system name>/gamelist.xml`（**集中存放，不与 ROM 同目录**） |
| 应用数据目录 | `~/ES-DE`，非 Windows 下可用环境变量 `ESDE_APPDATA_DIR` 覆盖 |
| 媒体目录 | `~/ES-DE/downloaded_media/<system name>/<media type>/` |
| 系统定义 | 内置 `es_systems.xml` + 用户覆盖 `~/ES-DE/custom_systems/es_systems.xml` |
| 自定义收藏 | `~/ES-DE/collections/custom-<名字>.cfg` |
| 官方 schema | 无 XSD；但 `INSTALL.md` 有**正式的字段参考章节** |

来源：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/INSTALL.md#gamelistxml>、<https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>

⚠️【事实】**ES-DE 的 gamelist.xml 里不再包含媒体路径**：

> "As of the fork to ES-DE, game media information no longer needs to be defined in the gamelist.xml files. Instead the application will look for any media matching the ROM filename."

这是 ES-DE 与其它 ES 变体最大的结构性差异——媒体靠**文件名约定**而非 XML 中的路径。

##### 数据类型

`string`、`float`、`integer`、`datetime`（ISO 字符串 `%Y%m%dT%H%M%S`，如 `19950311T000000`）、`bool`。
标记为 "statistic" 的字段由 ES-DE 自行维护、不出现在元数据编辑器中。

##### `<game>` 字段全表

| 字段 | 类型 | 说明 |
|---|---|---|
| `path` | string | 相对 `%ROMPATH%` 或绝对路径 |
| `name` | string | 显示名 |
| `sortname` | string | 系统列表排序用 |
| `collectionsortname` | string | **ES-DE 独有**，自定义收藏内排序用 |
| `desc` | string | 描述 |
| `rating` | float | **0–1 浮点**，处理时会**四舍五入到 0.1 的整数倍**（半星） |
| `releasedate` | datetime | 只显示日期部分，时间被忽略 |
| `developer` / `publisher` / `genre` | string | 单一字符串（**不是列表**） |
| `players` | integer | 官方参考写的是 integer，但官方示例里出现 `1-2` |
| `favorite` / `completed` / `kidgame` / `hidden` / `broken` | bool | |
| `nogamecount` | bool | **ES-DE 独有**，排除出游戏计数与收藏 |
| `nomultiscrape` | bool | **ES-DE 独有**，排除出批量刮削 |
| `hidemetadata` | bool | **ES-DE 独有**，列表视图中隐藏大部分元数据 |
| `playcount` | integer | |
| `playtime` | integer | **秒**（ES-DE 独有，原版 ES 无此字段） |
| `controller` | string | **ES-DE 独有**，显示手柄徽章 |
| `altemulator` | string | **ES-DE 独有**，按游戏覆盖模拟器/启动命令 |
| `lastplayed` | statistic, datetime | |

⚠️ 注意 `players` 在官方参考中标为 `integer`，但同一文档的示例给的是 `<players>1-2</players>`。【推断】实现上应按字符串宽松处理；转换工具读取时要容忍区间写法。

##### `<folder>` 字段

`path`、`name`、`desc`、`rating`、`releasedate`、`developer`、`publisher`、`genre`、`players`、`favorite`、`completed`、`hidden`、`broken`、`nomultiscrape`、`hidemetadata`、`controller`、`lastplayed`，外加 **`folderlink`**（指向文件夹内某个文件，启动它而不是进入文件夹）。
⚠️ folder **没有** `kidgame`、`nogamecount`、`playcount`、`playtime`、`sortname`、`altemulator`。

##### 其它官方声明的行为

- **等于默认值的字段不会被写出**（如 genre 为空则不写空标签）；
- ES-DE **不会自动清理**找不到 ROM 的条目；
- 「Orphaned data cleanup」工具的文档明确警告：
  > "Note that there are no guarantees that any processed gamelist.xml files will be usable in any other applications than ES-DE. An attempt is made to retain the file structure but **data unknown to ES-DE may get purged** during cleanup."

  ⚠️ 这是官方明说的**有损风险**：ES-DE 可能删掉它不认识的自定义 XML 元素。

##### `downloaded_media` 目录结构（**官方完整清单，12 个**）

```
3dboxes  backcovers  covers  custom  fanart  manuals
marquees  miximages  physicalmedia  screenshots  titlescreens  videos
```

- `miximages` 由 ES-DE 自行生成（可用离线生成器批量生成）；
- `custom` 目录**不会自动创建**，是可选的「媒体查看器最后一项」（例如手柄映射图）。

**命名规则**【事实】：媒体文件路径必须**精确镜像 ROM 相对系统目录的路径**，文件名为**去掉扩展名的 ROM 文件名**。官方示例：

```
ROM:   ~/ROMs/c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u
媒体:  ~/ES-DE/downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg
       ~/ES-DE/downloaded_media/c64/videos/Multidisk/Last Ninja 2/Last Ninja 2.mp4
```

例外：使用「Directories interpreted as files」功能时，加在目录上的「扩展名」也会包含在媒体文件名里（如 ScummVM 的 `dig.scummvm` → `dig.scummvm.png`）。

支持的扩展名：图片 `.jpg` `.png` `.webp`；视频 `.mp4` `.mkv` `.avi` `.wmv` `.mov` `.webm`。
⚠️ 官方提醒：Linux 下文件名**大小写敏感**，且**扩展名必须小写**（`.PNG` 找不到）。

##### ⚠️ 源码补充（官方文档未载）

以下取自 `es-app/src/MetaData.cpp` 与 `GamelistFileParser.cpp`，**比 `INSTALL.md` 的参考章节更全**：

来源：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/es-app/src/MetaData.cpp>

1. **`<game>` 实际有 24 个字段**，文档漏了 **`screen`**（`MD_SCREEN`，Android/双屏专用，取值 `""` / `"other"` / `"primary"`）。
2. **`players` 在源码里是 `MD_STRING`、默认 `"unknown"`**，不是文档写的 integer。`USERGUIDE.md` 的说法才对：「This could be an absolute number such as 1 or 3, or it could be a range, such as 2-4.」→ **按自由文本处理**。
3. **`releasedate` 默认值是 `"19700101T000000"`**，该字面量被特判为 epoch 0。
4. **`rating` 载入时取整**：`mValue = std::round(stof(value) / 0.1f) / 10.0f;` —— 确证了「四舍五入到 0.1」。序列化用 `std::stringstream`，即最短往返形式（写 `0.7` 而非 `0.700000`）。
5. **`<path>` 必须带前导 `./`**：`createRelativePath()` 返回 `"./" + relativePath`。`USERGUIDE.md` 明确警告：「the path tag requires a leading `./` in ES-DE while that may not be present in files coming from EmulationStation.」
6. **`controller` 的取值是 38 个固定短名**（`BadgeComponent.cpp`），如 `gamepad_nintendo_snes`、`lightgun_nintendo`、`steering_wheel_generic`、`unknown` 等——**不是自由文本**。
7. **`altemulator` 的值必须是 `es_systems.xml` 中某个 `<command label="…">` 的 label**；不匹配时 ES-DE 内部存为 `<INVALID>…` 并记警告。
8. **两个文档完全未提及的系统级元素**：
   ```xml
   <alternativeEmulator>
       <label>Snes9x - Current</label>
   </alternativeEmulator>
   <gameList>
       <launchOnOtherScreen>false</launchOnOtherScreen>
       <game>…</game>
   </gameList>
   ```
   ⚠️⚠️ **`<alternativeEmulator>` 目前被写在 `<gameList>` 之外**，导致文件有**两个根元素、技术上不是合法 XML**。源码注释承认这点：「The long term plan is to move the alternativeEmulator element so it becomes a child to the gameList root element in order to become fully XML standards compliant.」
   **任何用标准 XML 解析器读 ES-DE gamelist 的转换工具都必须专门处理这一点**（Skyscraper 的 `esde.cpp` 就有一段 `Coding Horror` 注释在做这个 workaround）。
9. **视频扩展名源码支持 7 个**：`.mp4`、`.mkv`、`.avi`、`.wmv`、`.mov`、`.webm`、**`.m4v`**——文档只列了 6 个，漏了 `.m4v`。
10. **gamelist 只有一个位置**：`<AppDataDirectory>/gamelists/<system>/gamelist.xml`。ROM 目录旁的 gamelist.xml **会被故意忽略并记警告**，除非在 `es_settings.xml` 里把隐藏设置 `LegacyGamelistFileLocation` 设为 `true`。

⚠️【事实】**ES-DE 官方明说迁移是单向的**：

> "It's not a goal for ES-DE to be compatible with EmulationStation and although you may be able to transfer some data from a legacy installation to ES-DE, the opposite is often not true and it's for example **a one-way ticket for your gamelist.xml files and your custom collection files**."
> —— <https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>

##### 自定义收藏文件

`~/ES-DE/collections/custom-<名字>.cfg`，**每行一条游戏路径**，使用 `%ROMPATH%` 变量：

```
%ROMPATH%/amiga/Flashback_v3.2_1163.hdf
%ROMPATH%/c64/Bionic Commando.d64
```

【事实】从旧版 ES 迁移来的绝对路径，会在第一次修改该收藏时被重写为 `%ROMPATH%` 形式。

##### `es_systems.xml` 要点

`<systemList>` → `<system>`，子标签：`name`（短名）、`fullname`、`systemsortname`（可选）、`path`（支持 `%ROMPATH%`、`~`）、`extension`（**空白分隔、大小写敏感、必须带点**，单个 `.` 表示无扩展名文件）、`command`、`platform`、`theme`。
`<command>` 可出现多次以定义**备选模拟器**，此时 `label` 属性必填，第一条为默认；这与游戏级的 `altemulator` 字段配套。
自定义文件放 `~/ES-DE/custom_systems/es_systems.xml`，是**增量覆盖**；加 `<loadExclusive/>` 才会禁用内置文件。

### B.2 Batocera 与 Recalbox

#### Batocera

【事实】**是同一个 `gamelist.xml`**（`<gameList>` 根、`<game>`/`<folder>`、`<path>` 首位、`./` 相对路径），Batocera 是一个**超集**：它能读原版 ES、RetroPie 和 Recalbox 的 gamelist，并additionally 加了 20 多个自有字段。

来源：<https://github.com/batocera-linux/batocera-emulationstation/blob/master/es-app/src/MetaData.cpp>

字段声明多了两列：`{ id, key, type, default, isStatistic, displayName, displayPrompt, visibleForFolder, isAttribute }`
类型枚举比原版多一个 **`MD_LIST`**。

**共 44 个字段。** Batocera 独有的（相对原版 ES）：

| 标签 | 类型 | 说明 |
|---|---|---|
| `tags` | MD_STRING | 标签 |
| `emulator` / `core` | MD_LIST | 每游戏指定模拟器/核心（**为兼容 Recalbox 而存在**，源码注释明说） |
| `fanart`、`titleshot`、`manual`、`magazine`、`map`、`bezel`、`cartridge`、`boxart`、`boxback`、`wheel`、`mix` | MD_PATH | 大量额外媒体路径 |
| `family` | MD_STRING | 游戏系列 |
| `genres` | MD_STRING | ⚠️ **从不序列化**（源码 `// Don't save GenreIds`），载入时重算 |
| `arcadesystemname` | MD_STRING | 街机系统名 |
| `crc32` / `md5` | MD_STRING | 校验和 |
| `gametime` | MD_INT | **累计游玩秒数** |
| `lang` | MD_STRING | 语言（枚举名是 `Language`，标签是 `lang`） |
| `region` | MD_STRING | 地区 |
| `cheevosHash` / `cheevosId` | MD_STRING / MD_INT | RetroAchievements（**驼峰大小写**） |
| `id` | MD_INT | ScreenScraper 游戏 ID，⚠️ **写成 XML 属性 `<game id="12345">` 而非子元素** |
| `multidisk` | MD_STRING | 多碟标记 |

⚠️ 注意 UI 标签与标签名不一致：`marquee` 显示为 "Logo"，`thumbnail` 显示为 "Box"。
⚠️ **`krating` 不存在**（全仓库零命中）；最接近的概念是 `kidgame`。
⚠️ Batocera **只有一个字段声明数组**，folder 用同样的 44 个字段，`visibleForFolder` 只影响编辑器显示。

**两个额外的序列化结构**

1. **刮削来源溯源** `<scrap>`：
   ```xml
   <scrap name="ScreenScraper" date="20240115T120000"/>
   ```
   已知 name：`ScreenScraper`、`TheGamesDB`、`HfsDB`、`ArcadeDB`。
   ⚠️ 这是 ES 家族里**最接近「provider」概念**的东西，但标签名是 `scrap`。
2. ⚠️⚠️ **未知标签保留**：Batocera 会把不认识的元素**和属性**原样往返（`mUnKnownElements`）。**这是四个变体里独一份的**——RetroPie ES、ES-DE、Recalbox 都会丢弃未知标签。
   → **推论：Batocera 是唯一一个可以安全塞自定义 XML 元素的 ES 变体。**

**位置**
- gamelist 查找：① `<系统 ROM 目录>/gamelist.xml` ② `<userES>/gamelists/<system>/gamelist.xml`；都不存在且需要写时**返回 ①**。
  ⚠️ **即在 Batocera 上新 gamelist 写在 ROM 目录旁**（`/userdata/roms/<system>/gamelist.xml`）——与 RetroPie 的写入行为相反。
- 系统配置：`es_systems.cfg`，查找 `es_systems_custom.cfg` → `es_systems.cfg` → `/usr/share/...` → `/etc/emulationstation/...`，并支持 `es_systems_*.cfg` 覆盖合并。额外元素：`<manufacturer>`、`<release>`、`<hardware>`、`<group>`，以及 `<emulators><emulator name= command=><cores><core netplay= default=>`。
  来源：<https://wiki.batocera.org/emulationstation:customize_systems>

**媒体目录**⚠️【事实，源码验证】常见说法「Batocera 媒体在 `<romdir>/<system>/media/...`」**是错的**。实际是 `Scraper::getSaveAsPath()`：

```
<系统 ROM 目录>/<folder>/<ROM 主名>-<后缀><扩展名>
```

只有**四个目录**，媒体子类型编码在**文件名后缀**里：

| 目录 | 后缀示例 |
|---|---|
| `images` | `-image`、`-thumb`、`-marquee`、`-fanart`、`-boxback`、`-box`、`-wheel`、`-titleshot`、`-map`、`-cartridge`、`-bezel` |
| `videos` | `-video` |
| `manuals` | `-manual` |
| `magazines` | `-magazine` |

`media/` 子目录**读取时被容忍但从不写入**。
⚠️【未证实】Batocera wiki 未记载媒体目录布局（`wiki.batocera.org/scrape_from` 只讲刮削源与凭据），本段**仅源码验证**。

#### Recalbox

⚠️【事实】**仓库位置需要更正**：`github.com/recalbox/recalbox-emulationstation` **已归档**（最后推送 2018-03-29），权威源是 GitLab 单体仓库 <https://gitlab.com/recalbox/recalbox> 的 `projects/frontend/`。

Recalbox 的前端已重写，**不再有 `MetaData.cpp`**，权威是 `MetadataDescriptor`。
来源：<https://gitlab.com/recalbox/recalbox/-/blob/master/projects/frontend/es-app/src/games/MetadataDescriptor.cpp>

⚠️⚠️ **Recalbox 最重要的结构差异：`isUserData` 分流。**
源码注释：*"If true, data goes into the userdata gamelist, not in the main gamelist."*
这些字段被写进 **`gamelist-userdata.ini`**，**不在 `gamelist.xml` 里**（但读取时仍会从 XML 读，再被 userdata 覆盖）。

**`<game>` —— 32 个字段**，其中 **userdata 字段 8 个**：`favorite`、`hidden`、`emulator`、`core`、`ratio`、`playcount`、`lastplayed`、`timeplayed`
**`<folder>` —— 只有 5 个**：`path`、`name`、`hidden`(userdata)、`desc`、`image`

Recalbox 独有标签：`aliases`、`licences`、`box`、`maps`（**嵌套子树**）、`logo`、`tips`、`lastPatch`（驼峰）、`rotation`、`adult`、`genreid`、`ratio`、`timeplayed`
Recalbox **没有**：`thumbnail`、`marquee`、`sortname`、`boxback`、`md5`、`crc32`、`kidgame`、`tags`、`family`、`arcadesystemname`、`bezel`、`cheevos*`

**取值编码**
- `rating`：float **0.00–1.00，两位小数**；
- `players`：`"N"` / `"min-max"` / `"N+"` —— ⚠️ **这是 ES 家族里唯一显式建模「人数区间」的**；
- `hash`：**8 位大写十六进制 CRC32**；
- `region`：逗号分隔的地区码；
- `timeplayed`：整数秒。

`<maps>` 是嵌套结构而非标量：
```xml
<maps>
  <map title="World 1" path="./maps/w1.png"/>
</maps>
```

**节点属性与顺序**
```xml
<game source="Recalbox" timestamp="…">
```
⚠️⚠️ **`<path>` 被写在最后而不是最前**（序列化循环是倒序 `for (; --count >= 0; )`）。读取按 key 不受影响，但**Recalbox 写出的 gamelist 违反了「path 在前」的惯例**。

**位置**
- `<系统 ROM 目录>/gamelist.xml` —— **单一位置，无回退链**；设备上即 `/recalbox/share/roms/<system>/gamelist.xml`；
- `<romPath>/gamelist-userdata.ini`。

⚠️ **`gamelist-userdata.ini` 尽管扩展名是 .ini，并不是标准 INI**：
```
<相对系统根的 ROM 路径>:key=value,key=value
```
每行一个游戏/文件夹；`:` 分隔路径与载荷，`,` 分隔键值对，`=` 分隔键与值。

**媒体目录**⚠️【事实】对 Recalbox 而言，「`<romdir>/<system>/media/...`」的说法**是对的**：
`/recalbox/share/roms/<system>/media/{images,boxes,videos,wheels,marquees,manuals,maps,tipsandtricks}/`
文件名模式：`<ScreenScraper 游戏名> <md5>.<格式>`。
⚠️ `marquees` 会被下载但**不写回任何元数据路径**（`pathSetter = nullptr`）。
⚠️【未证实】Recalbox wiki 未记载 gamelist 路径、`gamelist-userdata.ini` 或 media 子目录名（全站 807 个英文页枚举后搜 "gamelist" 零命中），本段**仅源码验证**。
⚠️ 仓库内的 `projects/frontend/GAMELISTS.md` **已过时**，描述的还是 fork 前的上游行为，不要采信。

#### ES 家族紧凑对照表

`•` = 有 · `—` = 无 · `U` = 有但存在 Recalbox 的 `gamelist-userdata.ini` · `A` = XML 属性

| 标签 | RetroPie | ES-DE | Batocera | Recalbox |
|---|:--:|:--:|:--:|:--:|
| `path` | • | • | • | • (**写在最后**) |
| `name` / `desc` | • | • | • | • |
| `sortname` | • | • | • | — |
| `collectionsortname` | — | • | — | — |
| `rating` | • 0–1 | • 0–1，×0.1 取整 | • 0–1 | • 0–1，2 位小数 |
| `players` | int | **string** | string | **range** |
| `image` / `video` | • | **—** | • | • |
| `marquee` / `thumbnail` | • | **—** | • | — |
| `favorite` / `hidden` | • | • | • | U |
| `kidgame` | • | • | • | — |
| `playcount` / `lastplayed` | • | • | • | U |
| 游玩**时长** | **—** | `playtime`(秒) | `gametime`(秒) | `timeplayed`(秒) U |
| `completed` / `broken` / `nogamecount` / `nomultiscrape` / `hidemetadata` / `controller` / `altemulator` / `screen` / `folderlink` | — | • | — | — |
| `emulator` / `core` | — | — | • | U |
| `ratio` | — | — | — | U |
| `crc32` / `md5` | — | — | • | — (有 `hash`) |
| `region` | — | — | • | • |
| `lang` | — | — | • | — |
| `tags` / `family` / `arcadesystemname` / `multidisk` / `bezel` / `magazine` / `mix` / `boxart` / `boxback` / `wheel` / `cartridge` / `fanart` / `titleshot` | — | — | • | — |
| `box` / `logo` / `tips` / `aliases` / `licences` / `lastPatch` / `rotation` / `adult` / `genreid` | — | — | — | • |
| `cheevosHash` / `cheevosId` | — | — | • | — |
| ScreenScraper id | — | — | • **A** | — |
| `source` / `timestamp` | — | — | — | • **A** |
| **未知标签重写后** | 丢弃 | 丢弃 | **保留** | 丢弃 |
| **gamelist 位置数** | 3 | **1** | 2（写 ROM 目录） | **1** + .ini 旁路 |
| **官方 schema** | 无 | 无 | 无 | 无 |



### B.3 LaunchBox

⚠️【未证实 / 重要】调研期间 **`docs.launchbox-app.com` 全程返回 HTTP 503**（Cloudflare 回源失败），且 Wayback 对该域名**零快照**。因此本节不依赖 LaunchBox 的最终用户文档，而是用三个替代的一手来源：

1. **官方插件 API 文档** <https://pluginapi.launchbox-app.com/>（由 `Unbroken.LaunchBox.Plugins.dll` v13.5.0.0 生成）；
2. **官方游戏数据库导出** `http://gamesdb.launchbox-app.com/Metadata.zip`（内含 `Metadata.xml`、`Platforms.xml` 等真实厂商数据）；
3. **Pegasus 的 LaunchBox 导入器源码**——它枚举了真实磁盘 XML 的**字面元素名**。

⚠️⚠️ **关键警告：插件 API 的属性名 ≠ XML 元素名。** 已证实至少三处不一致：
`Id`(API) → `<ID>`(XML)、`GenresString`(API) → `<Genre>`(XML)、`EmulatorId`(API) → `<Emulator>`(XML)。
因此下面把字段分成「已证实是 XML」和「仅 API、推测是 XML」两类。

#### 目录布局

| 路径 | 状态 |
|---|---|
| `LaunchBox/Data/Platforms.xml` | ✅ 已证实 |
| `LaunchBox/Data/Platforms/<Platform>.xml` | ✅ 已证实。文件名是平台 `<Name>` **原样**，不是 slug |
| `LaunchBox/Data/Emulators.xml` | ✅ 已证实 |
| `LaunchBox/Images/<Platform>/<ImageType>/` | ✅ 已证实 |
| `LaunchBox/Music/<Platform>/` | ✅ 已证实 |
| `LaunchBox/Videos/<Platform>/` | ✅ 已证实 |
| `LaunchBox/Manuals/<Platform>/` | ⚠️ 未证实（由 `IPlatform.ManualsFolder` 推断） |
| `LaunchBox/Data/Playlists/*.xml` | ⚠️ **未证实**。`IPlaylist` 确实是一等持久化对象，但无一手来源说明文件路径 |
| `LaunchBox/Data/Settings.xml` | ⚠️ 未证实 |

【事实】**所有 LaunchBox XML 的根元素都是 `<LaunchBox>`**。
【事实】官方 API 确认存储就是 XML 文件：`IDataManager.ForceReload` 的文档写道「Forces all data to be reloaded from **XML files**」。
⚠️【事实】**媒体目录可按平台覆盖**：`IPlatform` 暴露 `FrontImagesFolder`、`BackImagesFolder`、`ClearLogoImagesFolder`、`ScreenshotImagesFolder`、`FanartImagesFolder`、`BannerImagesFolder`、`SteamBannerImagesFolder`、`ManualsFolder`、`MusicFolder`、`VideosFolder`，还有 `GetAllPlatformFolders`。**所以 `Images/<Platform>/<Type>/` 只是默认值，不是不变量。**

#### `<Game>` —— 已证实为真实 XML 的元素

来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/launchbox/LaunchBoxGamelistXml.cpp>

| XML 元素 | 语义 |
|---|---|
| `ID` | 必填。与 `<AdditionalApplication>` 的关联键 |
| `ApplicationPath` | 必填，相对 LaunchBox 根目录。Steam 条目里是 `steam://rungameid/<id>` |
| `Title` / `SortTitle` | |
| `Developer` / `Publisher` | **单个字符串**，不是列表 |
| `ReleaseDate` | 按 ISO 8601 解析 |
| `Notes` | 描述 |
| `PlayMode` | ⚠️ **`;` 分隔的列表** |
| `Genre` | ⚠️ **单数**，不是 `Genres` |
| `CommunityStarRating` | float，**0–5 标度** |
| `Emulator` | ⚠️ 存的是**模拟器 ID**，不是名字 |
| `CommandLine` | 每游戏的模拟器参数覆盖 |
| `Platform` | 同时用作模拟器平台查找键 |
| `VideoPath` / `MusicPath` | 相对 LaunchBox 根目录 |
| `Source` | 值为 `Steam` 时触发 Steam URI 处理 |

#### `<Game>` —— 仅 API 可见（很可能也是 XML，但未直接证实）

`Region`、`Status`、`Version`、`Series`（`;` 分隔）、`MaxPlayers`、`Broken`、`Portable`、`Hide`、`Completed`、`Favorite`、`UseDosBox`、`PlayCount`、**`PlayTime`（秒）**、`LastPlayedDate`、`DateAdded`、`DateModified`、`ManualPath`、`ConfigurationPath`、`StarRatingFloat`、`CommunityStarRatingTotalVotes`、`WikipediaUrl`、`Installed`、`ReleaseType`、`CloneOf`、`RootFolder`、`LaunchBoxDbId`、`VideoUrl`、ScummVM/DosBox 相关项、各种启动屏覆盖项。

⚠️ 两个重要的类型纠正：
- **`Rating` 是 `String`，不是数值**——它是**内容分级（ESRB 等）**，不是评分。
- **`StarRating` 是 `Int32` 且已废弃**，官方注释：「Deprecated; please use `StarRatingFloat` instead.」

⚠️ 以下是**派生属性、几乎肯定不是 XML 元素**，不要写出：`Developers`、`Publishers`、`PlayModes`、`SeriesValues`、`Genres`、`SortTitleOrTitle`、`ReleaseYear`，以及所有 `*ImagePath`（图片是从**文件系统**解析的，不在 XML 里）。

#### `<AdditionalApplication>`（多碟 / 备选启动）

⚠️【事实】**大小写三种风格并存**：

| XML 元素 | 备注 |
|---|---|
| `Id` | ⚠️ **`Id`**，与 `<Game>` 的 `ID` 不一致 |
| `GameID` | ⚠️ **`GameID`**，第三种写法。外键指向 `<Game><ID>` |
| `ApplicationPath` | 必填 |
| `Name` | |

官方 `IAdditionalApplication` 的完整属性还包括：`AutoRunAfter`、`AutoRunBefore`、`CommandLine`、`Developer`、**`Disc`**、`EmulatorId`、`Installed`、`LastPlayed`、`PlayCount`、`PlayTime`（秒）、`Priority`（排序）、`Publisher`、`Region`、`ReleaseDate`、**`SideA`**、**`SideB`**、`Status`、`UseDosBox`、`UseEmulator`、`Version`、`WaitForExit`。

⚠️ `Disc` / `SideA` / `SideB` —— **这正是 LaunchBox 表达多碟/多面的机制**，比 Pegasus 的 `files:` 列表语义更丰富。

#### `<CustomField>` —— 官方扩展机制

官方 `ICustomField` 只有三个属性：`GameId`、`Name`、`Value`。
另有一个同类记录 `IAlternateName`（`GameId`、`Name`、`Region`）。
⚠️ XML 中的实际大小写未证实（鉴于 `<AdditionalApplication>` 用的是 `GameID`，这里也可能是 `GameID`）。

#### 图片类型目录名（权威清单）

两个独立的一手来源互相印证。

**(a) 官方游戏数据库 `Metadata.xml` 中 `<GameImage><Type>` 的全部取值**（全文件扫描，括号内为出现次数）：

`Screenshot - Gameplay`(247092)、`Box - Front`(205073)、`Clear Logo`(143085)、`Screenshot - Game Title`(108449)、`Box - Back`(104699)、`Box - 3D`(92523)、`Fanart - Background`(70695)、`Disc`(51407)、`Fanart - Box - Front`(49057)、`Box - Spine`(48407)、`Cart - Front`(40169)、`Banner`(31693)、`Advertisement Flyer - Front`(27848)、`Box - Front - Reconstructed`(20879)、`Screenshot - Game Select`(16090)、`Fanart - Cart - Front`(12210)、`Arcade - Marquee`(8428)、`Screenshot - Game Over`(6368)、`Arcade - Cabinet`(5766)、`Fanart - Disc`(5280)、`Square`(4705)、`Advertisement Flyer - Back`(4526)、`Screenshot - High Scores`(3387)、`Arcade - Controls Information`(3167)、`Cart - 3D`(2946)、`Arcade - Control Panel`(1798)、`Fanart - Box - Back`(1788)、`Box - Back - Reconstructed`(1688)、`Poster`(1629)、`Arcade - Circuit Board`(1366)、`Cart - Back`(1185)、`Icon`(909)、`Fanart - Cart - Back`(20)

一条 `<GameImage>` 记录含：`<DatabaseID>`、`<FileName>`（GUID + 扩展名）、`<Type>`、可选 `<Region>`、`<CRC32>`。

**(b) 官方 `ImageTypes` 类**的常量标识符，额外包含商店集成类型：`AmazonBackground/Poster/Screenshot`、`EpicGamesBackground/Poster/Screenshot`、`GogPoster/Screenshot`、`OriginBackground/Poster/Screenshot`、`SteamBanner/Poster/Screenshot`、`UplayBackground/Thumbnail`、`BoxFull` 等。

⚠️ **磁盘目录名用的是人类可读的 `<Type>` 字符串原样**（`Box - Front`、`Screenshot - Gameplay`，分隔符是 ` - `），**不是** CamelCase 常量名。

#### 图片文件名规则

【事实】**标题净化**（源码）：
```cpp
const QRegularExpression rx_invalid(QStringLiteral(R"([<>:"\/\\|?*'])"));
title.replace(rx_invalid, underscore);   // underscore = "_"
```
`< > : " / \ | ? *` **以及撇号 `'`**，每个都替换成一个 `_`。
⚠️ **撇号值得注意**：它在 Windows/NTFS 上是合法字符，所以这是 LaunchBox 自己的选择，不是操作系统约束。

【事实】**序号后缀**：正则 `-[0-9]{2}$`，即 `<净化后的标题>.png` 与 `<净化后的标题>-NN.png`（**恰好两位数字**）都有效。

⚠️【未证实】**region 放在哪里**。已知两个事实：
- 官方 API 方法签名是 `GetNextAvailableImageFilePath(string extension, string imageType, string region)`——region **确实是路径生成的输入**；`ImageDetails` 也带 `{ FilePath, ImageType, Region }`。
- Pegasus **递归**扫描每个图片类型目录，却只按**基名**匹配——这只有在 LaunchBox 把文件嵌套在子目录里时才说得通。

→【推断】region 很可能是**子目录**（`Images/<Platform>/Box - Front/<Region>/<Title>-01.jpg`）而非文件名 token，但**这是推断，未经证实**。region 取值可从官方 `Metadata.xml` 枚举：`North America`、`Europe`、`Japan`、`World`、`Brazil`、`Spain` 等。

#### 是否有官方 XSD？

⚠️【事实】**没有。** 官方 `Metadata.zip` 里四个 XML 全部以 `<?xml version="1.0" standalone="yes"?>` 开头，**无 `xsi:schemaLocation`、无 DOCTYPE、无命名空间**。插件 API 文档也未引用任何 schema。`standalone="yes"` + 无命名空间是 .NET `XmlSerializer` / `DataSet.WriteXml` 输出的典型特征。

#### .NET `XmlSerializer` 带来的后果

- **日期**：官方 `Metadata.xml` 中全部 113,199 个时间戳形如 `yyyy-MM-ddTHH:mm:ss+00:00`（ISO-8601 `xsd:dateTime`，带显式 UTC 偏移，**该数据集中无小数秒**）。
  ⚠️【推断】本地 `Data/Platforms/<Platform>.xml` 的 `DateAdded`/`LastPlayedDate` 走 `XmlDateTimeSerializationMode.RoundtripKind`，**预期会有小数秒**（最多 7 位、去掉尾随零），后缀取决于 `DateTime.Kind`。**未用真实用户文件验证。**
  → **解析器建议**：接受 `yyyy-MM-ddTHH:mm:ss` + 可选 `.fffffff` + 可选 `Z` 或 `±HH:mm`。
- **布尔**：`xsd:boolean` 词法形式，**小写 `true` / `false`**。不要期待 `1`/`0`，也不要写 `True`/`False`。
- **元素顺序跟随 C# 类的声明顺序，且跨版本不稳定**——⚠️ **必须按名字解析，绝不能按位置**。
- `null` 引用类型成员**整个省略**；空集合写成自闭合标签（`<Genres />`）。
- 数据全用子元素，**不用属性**。
- 数字用不变文化（`.` 小数点，double 全精度，无千位分隔符）。

### B.4 Playnite

来源：<https://github.com/JosefNemec/Playnite>、<https://api.playnite.link/docs/>

#### ⚠️ 库存储格式：是 LiteDB，不是每对象 JSON

这是一个**极易搞错**的点。当前版本用的是 **LiteDB v4**，每个集合一个 `.db` 文件。

源码 `ItemCollection.cs` 注释原文：
> `// We currently use LiteDB for permanent storage.`
> `// We don't use latest LiteDB 5, but instead latest LiteDB 4, because V5 has some issues:`

```csharp
var dbPath = path + ".db";
liteDb = new LiteDatabase($"Filename={dbPath};Mode=Exclusive;Cache Size=0", mapper);
```

⚠️ 源码里确实还有一个 `GetItemFilePath(Guid id) => Path.Combine(storagePath, $"{id}.json");`，但**全仓库零调用点**——是 JSON 时代遗留的死代码。**不要据此开发。**

**格式版本沿革**（取自 `GameDatabaseMigration.cs`，`NewFormatVersion = 4`）：

| 迁移 | 做了什么 |
|---|---|
| 1 → 2 | 写成每对象 JSON：`<collection>/<Id>.json` |
| **2 → 3** | ⚠️ **把 JSON 目录塌回 LiteDB**，然后 `Directory.Delete(dir, true)` |
| 3 → 4 | `files` 目录内的清理 |

→ **`library/games/*.json` 只在格式版本 2（大致 Playnite 5–8 时代）存在过**，从格式 3 起已废弃。

**磁盘布局**（根目录 `%AppData%\Playnite\library`，portable 模式为 `<程序目录>\library`）：

| 文件 | 内容 |
|---|---|
| `library/database.json` | ⚠️ **唯一的 JSON**，内容是 `{ "Version": 4 }` |
| `library/games.db` | 游戏（LiteDB v4 BSON） |
| `library/platforms.db`、`emulators.db`、`genres.db`、`companies.db`、`tags.db`、`features.db`、`categories.db`、`series.db`、`ageratings.db`、`regions.db`、`sources.db`、`tools.db`、`scanners.db`、`filterpresets.db`、`importexclusions.db`、`completionstatuses.db` | 各查找表 |
| `library/files/` | 媒体 |

#### `Game` 模型 —— 持久化字段

来源：<https://github.com/JosefNemec/Playnite/blob/master/source/PlayniteSDK/Models/Game.cs>

| 字段 | 类型 | 说明 |
|---|---|---|
| `Id` | `Guid` | |
| `Name` | `string` | |
| `SortingName` | `string` | |
| `Description` | `string` | ⚠️ 官方文档原文：「Gets or sets **HTML** game description.」**是 HTML，不是纯文本** |
| `Notes` | `string` | 用户笔记 |
| `GameId` | `string` | 来源方 ID，如 Steam ID |
| `PluginId` / `SourceId` | `Guid` | |
| `GenreIds`、`DeveloperIds`、`PublisherIds`、`TagIds`、`FeatureIds`、`CategoryIds`、**`PlatformIds`**、`SeriesIds`、`AgeRatingIds`、`RegionIds` | `List<Guid>` | ⚠️ **`PlatformIds` 自 Playnite 9 起是复数**——一个游戏可属于多个平台 |
| `CompletionStatusId` | `Guid` | |
| `ReleaseDate` | `ReleaseDate?` | 见下 |
| `Playtime` | `ulong` | **秒** |
| `PlayCount` | `ulong` | |
| `LastActivity` / `Added` / `Modified` / `LastSizeScanDate` | `DateTime?` | |
| `InstallSize` | `ulong?` | **字节** |
| `IsInstalled` / `OverrideInstallState` / `Hidden` / `Favorite` | `bool` | |
| `InstallDirectory` | `string` | |
| `UserScore` / `CriticScore` / `CommunityScore` | `int?` | ⚠️ **三个独立评分** |
| `Version` | `string` | |
| `Icon` / `CoverImage` / `BackgroundImage` | `string` | 「Local file path, HTTP URL or database file ids are supported.」 |
| `Manual` | `string` | |
| `Links` | `ObservableCollection<Link>` | `Link { Name, Url }` |
| `Roms` | `ObservableCollection<GameRom>` | `GameRom { Name, Path }` |
| `GameActions` | `ObservableCollection<GameAction>` | 见下 |
| `EnableSystemHdr`、`PreScript`/`PostScript`/`GameStartedScript`、`UseGlobal*Script` | 混合 | |

⚠️【事实】以下带 `[DontSerialize]`，是**只读投影、不存储**：
`Genres`、`Developers`、`Publishers`、`Tags`、`Features`、`Categories`、`Platforms`、`Series`、`AgeRatings`、`Regions`、`Source`、`CompletionStatus`、**`ReleaseYear`**、各 `*ScoreRating`/`*ScoreGroup`、`PlaytimeCategory`、`InstallSizeGroup`、`IsCustomGame`、`InstallationStatus`。
**真正存的是 `*Ids` 的 `List<Guid>`**，通过 `DatabaseReference` 去兄弟 `.db` 集合解析。

`GameAction`：`Type`（`File`/`URL`/`Emulator`/`Script`）、`Name`、`Path`、`Arguments`、`AdditionalArguments`、`OverrideDefaultArgs`、`WorkingDir`、`IsPlayAction`、`EmulatorId`、`EmulatorProfileId`、`TrackingMode`、`TrackingPath`、`Script`、`InitialTrackingDelay`、`TrackingFrequency`。

#### 媒体存储

`library/files/<父对象 Guid>/<文件名>`
写回 `Icon`/`CoverImage`/`BackgroundImage` 的值是**相对路径** `<GameId-Guid>\<文件名>`（相对 `library/files/`），**不是绝对路径也不是裸 id**。
- HTTP 来源的媒体，文件名是**新生成的 Guid + 原扩展名**，且图片会过一遍 `Images.ConvertToCompatibleFormat`（可能改扩展名）；
- `parentId` 是拥有者的 `Id`——平台/模拟器的媒体也在同一个 `files/` 树下；
- `BackgroundImage` 可以直接是 **HTTP URL**，此时走 `HttpFileCache` 而不落 `files/`。

#### ⚠️ `ReleaseDate` 结构 —— 唯一原生支持日期精度的格式

```csharp
public readonly DateTime Date;      // 总是被具体化
public int? Day   { get; private set; }
public int? Month { get; private set; }
public int  Year  { get; private set; }   // 非 nullable
```

缺失的部分在 `Date` 里被回填为 `1`，**所以只看 `Date` 会静默丢失精度**，必须读 `Day`/`Month` 是否为 null 才知道真实精度。

**序列化格式**：

| 精度 | 字符串 | 示例 |
|---|---|---|
| 精确到日 | `{Year}-{Month}-{Day}` | `1998-11-8` |
| 精确到月 | `{Year}-{Month}` | `1998-11` |
| 只有年 | `{Year}` | `1998` |

⚠️⚠️ **不补零**：`Serialize()` 用原始 int 插值，所以 11 月 8 日写作 `1998-11-8`，3 月写作 `1998-3`。
`TryDeserialize` 用的格式是 `"yyyy-M-d"`、`"yyyy-M"`、`"yyyy"`（不变文化）——补零的 `1998-11-08` 也能解析，但**不是 Playnite 写出的形式**。转换工具两种都要接受。
这个结构注册了自定义 LiteDB 映射器，**所以 `games.db` 里发行日期是上述三种形状之一的 BSON 字符串，不是 BSON date**。

#### 导入 / 导出能力

⚠️【事实】**没有官方的库导出功能。** 拉取 DocFX 全文索引（654 条）搜 "export"，唯一真正的命中是 SDK 方法 `IGameDatabaseAPI.SaveFile(string id, string path)`——「Exports file from database.」，**导出的是单个媒体文件，不是库**。

最接近「导出」的官方功能是**备份**：`--backup` / `--restorebackup` 命令行参数配 JSON 配置，产出 `PlayniteBackup-yyyy-MM-dd-HH-mm-ss.zip`。备份项 id：`0` 设置、`1` 库、`2` 库媒体、`3` 扩展、`4` 主题、`5` 扩展数据。
⚠️ 官方注意事项：「Playnite can create and restore backups **only during application startup**.」

**扩展点**：`LibraryPlugin`（`Id`、`Name`、`GetGames` 必需）与 `MetadataPlugin`。

⚠️【事实】**脚本只支持 PowerShell**：
> "**PowerShell** is currently the only supported scripting language." … "requires PowerShell 5.1 … Playnite currently doesn't support newer PowerShell Core runtime (PowerShell versions 6 and newer)."

**IronPython 已被移除**（Playnite 9 世代）。扩展以 PowerShell **模块**（`.psm1`/`.psd1`）导入，函数必须是模块作用域（写 `function global:OnGameStarted()` **不会**正确导出）。API 通过 `$PlayniteApi` 变量暴露。
→ **推论：从 Playnite 导出数据，实践上唯一官方支持的路径就是写一个 PowerShell 脚本遍历 `$PlayniteApi.Database.Games` 自己写文件。**



### B.5 RetroArch 播放列表 `.lpl` 与缩略图

| 项目 | 内容 |
|---|---|
| 位置 | `<config>/playlists/*.lpl`（Linux 下 `$XDG_CONFIG_HOME/retroarch` 或 `~/.config/retroarch`） |
| 格式 | **JSON**（现代）；另有已废弃的「每游戏 6 行」纯文本格式 |
| 官方 schema | 无 |
| 特殊列表 | `content_favorites.lpl`、`content_history.lpl`、`content_image_history.lpl`、`content_music_history.lpl`、`content_video_history.lpl` |

来源：<https://github.com/libretro/RetroArch/blob/master/file_path_special.h>、<https://github.com/libretro/RetroArch/blob/master/frontend/drivers/platform_unix.c>

#### 顶层键（写出顺序，取自 `playlist_write_file()`）

`version`（master 上为 `"1.5"`）、`default_core_path`、`default_core_name`、`base_content_directory`（条件）、`label_display_mode`、`right_thumbnail_mode`、`left_thumbnail_mode`、`thumbnail_match_mode`、`sort_mode`、`scan_content_dir`、`scan_file_exts`、`scan_dat_file_path`、`scan_database_name`、`scan_search_recursively`、`scan_search_archives`、`scan_filter_dat_content`、`scan_omit_db_ref`、`scan_overwrite_playlist`、`scan_db_usage`、`items`

⚠️ 键名是 **`scan_search_recursively`**，不是常见误写的 `scan_recursive`。
⚠️ 读取端是按首字符分派的 `strcmp`，**未知键被静默忽略**——可以塞私有键，但**重写时会丢失**。

来源：<https://github.com/libretro/RetroArch/blob/master/playlist.c>

#### 每条目键

**总是写出**（顺序固定）：`path`、`label`、`core_path`、`core_name`、`crc32`、`db_name`
**条件写出**：`entry_slot`、`subsystem_ident`、`subsystem_name`、`subsystem_roms`（字符串数组）

⚠️【事实】`runtime_hours/minutes/seconds` 与 `last_played_*` **读取端仍兼容，但主写出函数已不再写入**。现行游玩数据存放于独立的 `.lrtl` 文件。
【事实】`crc32` 带后缀，形如 `"01ACE2AB|crc"`；`DETECT` 是 `core_path`/`core_name`/`crc32` 未知时的哨兵值。

#### 废弃的 6 行格式

每游戏 6 行，顺序为 `path`、`label`、`core_path`、`core_name`、`crc32`、`db_name`；元数据以 `key = "value"` 形式**追加在所有条目之后**（这样旧版本会忽略）。注意该格式用**单个** `thumbnail_mode = "right|left"`，而 JSON 用两个独立键。

#### 压缩包内路径

语法 `/path/to/archive.zip#file.rom`。`#` 仅在**紧跟已知压缩扩展名**时才被当作分隔符，因此文件名中可含 `#`。识别的扩展名（大小写不敏感）：`.7z`、`.zip`、`.zst`、`.apk`。
来源：<https://github.com/libretro/RetroArch/blob/master/libretro-common/file/file_path.c>

#### ⚠️ 缩略图命名规则（转换的关键难点）

目录布局：
```
<thumbnails>/<db_name 去掉 .lpl>/<Named_Boxarts|Named_Snaps|Named_Titles|Named_Logos>/<净化后的名字>.png
```

**db_name → 目录名的三条非显然规则**：
1. **去掉 `.lpl` 扩展名**；
2. **MAME 塌缩**：任何以 `MAME` 开头的 db_name 一律改写为 `MAME`（源码注释：「Hack: There is only one MAME thumbnail repo」）。所以 `MAME 2003-Plus.lpl` 找的是 `thumbnails/MAME/`；
3. **竖线截断**：db_name 含 `|` 时只取第一个 `|` 之前的部分。

**文件名净化规则（源码原文）**：
```c
/* Scrub characters that are not cross-platform and/or violate the
 * No-Intro filename standard ...
 * Replace these characters in the entry name with underscores */
while ((scrub_char_ptr = strpbrk(s, "&*/:`\"<>?\\|")))
   *scrub_char_ptr = '_';
```

解开 C 转义后，被替换为 `_` 的是 **11 个字符**：

```
&  *  /  :  `  "  <  >  ?  \  |
```

⚠️⚠️ **libretro 官方文档漏了双引号 `"`**（文档只列了 10 个：``&*/:`<>?\|``）。**以源码为准：`"` 也会被替换。** 这是转换工具最容易踩的坑。
来源（源码，权威）：<https://github.com/libretro/RetroArch/blob/master/gfx/gfx_thumbnail.c>
来源（文档，有误）：<https://github.com/libretro/docs/blob/master/docs/guides/roms-playlists-thumbnails.md>

其它性质：纯字节级 `strpbrk`，**无 Unicode 归一化、无空白裁剪、无大小写折叠**；撇号、逗号、括号、方括号、`+`、`#`、`!` 等**原样保留**。

**三级文件名匹配**（RetroArch ≥ 1.17.0 的「灵活名称匹配」）：
1. 由播放列表 `label` 净化而来；
2. 由 **ROM 文件名**（去扩展名）净化而来；
3. label 在**第一个 `" ("`（空格+左括号）处截断**后净化而来。

所以 `Q-Bert's Qubes (USA) (1983) [h]` 也能匹配到 `Q-Bert's Qubes.png`。

**非 PNG 扩展名**（受 `playlist_allow_non_png` 设置控制，`.png` 恒在最前）：`.png`、`.jpg`、`.jpeg`、`.bmp`、`.tga`、`.webp`、`.webm`、`.mp4`。

#### 游玩数据 `.lrtl`

位置：`<playlists>/logs/<内容名>.lrtl`（聚合模式）或 `<playlists>/logs/<核心名>/<内容名>.lrtl`（分核心模式）。
格式（**所有值都是字符串**，包括数字）：
```json
{
  "version": "1.0",
  "runtime": "H:MM:SS",
  "last_played": "YYYY-MM-DD HH:MM:SS",
  "play_count": "N",
  "state_slot": "N"
}
```
来源：<https://github.com/libretro/RetroArch/blob/master/runtime_file.c>

#### 整数枚举含义

- `label_display_mode`：0 DEFAULT，1 REMOVE_PARENTHESES，2 REMOVE_BRACKETS，3 REMOVE_PARENTHESES_AND_BRACKETS，4 KEEP_REGION，5 KEEP_DISC_INDEX，6 KEEP_REGION_AND_DISC_INDEX
- `right/left_thumbnail_mode`：0 DEFAULT，1 OFF，2 SCREENSHOTS，3 TITLE_SCREENS，4 BOXARTS，5 LOGOS
  ⚠️ 存在 **off-by-one**：取目录名时先算 `type = mode - 1`，故播放列表值 `2`(SCREENSHOTS) → `Named_Snaps`，`4`(BOXARTS) → `Named_Boxarts`。
- `thumbnail_match_mode`：0 WITH_LABEL，1 WITH_FILENAME
- `sort_mode`：0 DEFAULT，1 ALPHABETICAL，2 OFF

来源：<https://github.com/libretro/RetroArch/blob/master/playlist.h>

### B.6 Attract-Mode romlist

⚠️【未证实 / 重要】官方文档站 `attractmode.org/docs/Configuration.html` 与 `Layouts.html` 现在都返回 **HTTP 404**，Wayback 也无 `Configuration.html` 快照。因此本节的 emulator 配置部分**只能依据源码**；`Layouts` 部分以仓库内 <https://github.com/mickelson/attract/blob/master/Layouts.md> 为准。

| 项目 | 内容 |
|---|---|
| 配置根 | Linux/macOS `$HOME/.attract/`；Windows `./`；Android `$HOME/` |
| romlist | `romlists/<名字>.txt`，分号分隔 |
| 模拟器配置 | `emulators/<名字>.cfg` |
| 统计 | `stats/<emulator>/<romname>.stat` |
| 收藏 | `romlists/<romlist名>.tag` |
| 标签 | `romlists/<romlist名>/<tag名>.tag` |

来源：<https://github.com/mickelson/attract/blob/master/src/fe_settings.cpp>、<https://github.com/mickelson/attract/blob/master/src/fe_romlist.cpp>

#### romlist 字段顺序（**24 个**）

取自 `FeRomInfo::indexStrings[]`：

```
#Name;Title;Emulator;CloneOf;Year;Manufacturer;Category;Players;Rotation;Control;Status;DisplayCount;DisplayType;AltRomname;AltTitle;Extra;Buttons;Series;Language;Region;Rating;DisplayWidth;DisplayHeight;DisplayRefresh
```

⚠️ 常见的「21 字段、止于 `Rating`」说法**已过时**：master 追加了 `DisplayWidth`、`DisplayHeight`、`DisplayRefresh`。
⚠️ 枚举里 `Favourite` 之后的字段（`Favourite`、`Tags`、`PlayedCount`、`PlayedTime`、`FileIsAvailable`）**只存在于内存，永不写入 romlist**——源码注释：`// everything from Favourite on is not loaded from romlist`。它们来自 `.tag` 与 `.stat` 文件。
【事实】旧版 21 字段 romlist 仍能解析，缺失的尾部字段留空。
【事实】首个 token 以 `#` 开头的行被跳过（这就是表头被忽略的机制）。

来源：<https://github.com/mickelson/attract/blob/master/src/fe_info.cpp>

#### 转义规则（**不对称，有坑**）

```cpp
std::string FeRomInfo::get_info_escaped( int i ) const
{
	if ( m_info[i].find_first_of( ';' ) != std::string::npos )
	{
		std::string temp = m_info[i];
		perform_substitution( temp, "\"", "\\\"" );
		return ( "\"" + temp + "\"" );
	}
	else
		return m_info[i];
}
```

1. 值**不含** `;` → 原样写出；
2. 值**含** `;` → 用双引号包裹，内部 `"` 转义为 `\"`。

⚠️【事实+推断】这是**不对称的**：含 `"` 但不含 `;` 的值会被**不加引号、不转义**地写出。读取端只在 token 首个非空白字符是 `"` 时才进入引号模式，因此形如 `"Hello" World` 且无分号的值**无法正确往返**。这是格式层面的已知缺陷，转换工具应主动规避（例如把值中的 `"` 或 `;` 预先替换）。
⚠️【事实】除非被引号包裹，**字段值的首尾空格不会被保留**（读取时统一 trim）。

#### emulator 配置键

`name`、`executable`、`args`、`workdir`、`rompath`、`romext`、`system`、`info_source`、`import_extras`、`nb_mode_wait`、`exit_hotkey`、`pause_hotkey`

- `name` **从不写入文件**——由文件名 `emulators/<name>.cfg` 推导。
- `rompath` / `romext` 是**分号分隔的多值列表**。
- `artwork` **不在** `indexStrings` 中，单独处理，语法为 `artwork <label> <路径列表>`：label 按空白切分，其余到行尾为路径列表（再按 `;` 切分）。label 是 `map` 的键，**任意字符串，不是固定枚举**；同名 `artwork` 行会累加而非覆盖。
- `minimum_run_time` 是 `nb_mode_wait` 的向后兼容别名（≤ v2.2）。

【事实】官方 `Layouts.md` 称标准 artwork label 为：`snap`、`marquee`、`flyer`、`wheel`、`fanart`；`FE_DEFAULT_ARTWORK = "snap"`。

#### 资源文件名匹配

候选基名按序尝试：**`Romname` → `AltRomname` → `CloneOf` → `Emulator` 名**（最后一项是共享兜底）。
扩展名：`.png`、`.jpg`、`.jpeg`、`.bmp`、`.tga`；其余同名文件按视频候选处理。
⚠️ 微妙之处：**视频短路、图片不短路**——匹配 `Romname` 的视频立即返回，但图片是三级全部收集后再按优先级返回，因此 `AltRomname` 的**视频**会胜过 `Romname` 的**图片**。
⚠️ **Attract-Mode 对资源文件名不做任何字符净化**（与 RetroArch 相反），因为匹配用的是 ROM 基名，本身已是文件系统安全的。
【事实】若 `<art_path>/<target_name>/` 是**目录**，则收集其中全部媒体并 `random_shuffle`，实现每局随机美术。

#### 用户状态

`stats/<emulator>/<romname>.stat`：**恰好两行**，第 1 行 `PlayedCount`（次数），第 2 行 `PlayedTime`（**秒**）。
`.tag` 文件：每行一个 `Romname`；收藏文件是排序过的（内部用 `std::set`）。集合变空时**删除文件**而非写空文件。
内存中的 `Tags` 字段格式为**每个标签两侧都带分隔符**，如 `;action;shooter;`，便于用 `find(";"+tag+";")` 做整词匹配。

### B.7 Steam 快捷方式（shortcuts.vdf）与 Steam ROM Manager

来源：<https://github.com/SteamGridDB/steam-rom-manager>、<https://github.com/tirish/steam-shortcut-editor>

#### 文件位置

| 用途 | 路径 |
|---|---|
| 快捷方式 | `<Steam>/userdata/<accountID>/config/shortcuts.vdf` |
| 网格美术 | `<Steam>/userdata/<accountID>/config/grid/` |
| 截图 VDF | `<Steam>/userdata/<accountID>/760/screenshots.vdf` |

`<accountID>` 是 SteamID3 / 32 位 account ID。

#### 格式 = **二进制 VDF**

【事实】`shortcuts.vdf` 是**二进制** VDF，不是文本 VDF。证据：SRM 用两个不同的解析器——`shortcuts.vdf` 走 `steam-shortcut-editor` 的 `parseBuffer(data)`（原始 Buffer），而 `screenshots.vdf` 走 `@node-steam/vdf` 的 `parse(data)`（UTF-8 字符串）。

二进制编码：
```js
types:   { object: 0x00, string: 0x01, int: 0x02 }
special: { objectEnd: 0x08, stringEnd: 0x00, propertyNameEnd: 0x00 }
```
类型字节 → NUL 结尾的键名 → 值（NUL 结尾字符串，或 `Int32LE`），对象以 `0x08` 结束。**数组写成键为 `"0"`、`"1"`… 的对象。** 布尔写成 int `1`/`0`。

#### 条目键名

【事实】从 `steam-shortcut-editor` 仓库里 **6 个真实的 Steam 产出的 `shortcuts.vdf` 测试样本**中 `strings` 出来的键名：
`AppName` / `appname`、`exe`、`StartDir`、`icon`、`ShortcutPath`、`LaunchOptions`、`IsHidden`、`AllowDesktopConfig`、`AllowOverlay`、`OpenVR`、`LastPlayTime`、`tags`。顶层容器键是 `shortcuts`。

⚠️⚠️ **大小写确实不一致**。`steam-shortcut-editor` 的 README 就 `AppName` 注明：「(the casing seems to vary, have seen it appear as `appname`)」，并加粗声明「This library does not make any property name/value assumptions.」SRM 源码注释也印证：
> `// Steam writes shortcuts.vdf keys with inconsistent casing (e.g.`
> `// "AppName"/"Exe" when a game is added through Steam itself)`

**SRM 自己写出的键**（注意是**小写** `appname`/`exe`，还有一个未被广泛记录的 `sortas`）：
```ts
{ appid, appname, exe, StartDir, LaunchOptions, icon, tags, sortas? }
```
SRM **创建时不写** `IsHidden`、`AllowDesktopConfig`、`AllowOverlay`、`OpenVR`、`ShortcutPath`、`LastPlayTime` 或任何 `Devkit*`——更新时保留 Steam 已有的值。

⚠️【未证实】`Devkit`、`DevkitGameID`、`DevkitOverrideAppID`、`FlatpakAppID` **无法从任何 Valve 来源证实**。**Valve 没有发布 `shortcuts.vdf` 的 schema。** 这些键在 itch.io 桌面端等多个开源实现中一致出现，属于**广泛实现的约定，非官方文档**。

#### 网格图片命名

【事实】后缀表来自 SRM 源码 `available-artwork-types.ts`：
```ts
export const artworkTypes = ["tall", "long", "hero", "logo", "icon"] as const;
export const artworkIdDict: Record<ArtworkType, string> = {
  tall: "p", long: "", hero: "_hero", logo: "_logo", icon: "_icon",
};
```

在 `grid/` 目录下，`<S>` = **短** app id：

| 文件 | 类型 | SRM 的尺寸 |
|---|---|---|
| `<S>.png` | `long`（横幅/header） | `1196x559`（注释：`// 920x430 x 1.3`） |
| `<S>p.png` | `tall`（竖版胶囊） | `600x900` |
| `<S>_hero.png` | `hero` | `1920x620` |
| `<S>_logo.png` | `logo` | `960x540` |
| `<S>_icon.png` | `icon` | `600x600` |

⚠️ **常见误解纠正**：`<appid>.png` 是**横幅**，不是 600×900 的竖版胶囊；竖版是 `<appid>p.png`。**不存在 `<appid>b.png`。**
【事实】扩展名归一化：`jpg`、`png`、`tga`、`ico`；**`webp` 会被映射成 `png`**。
【事实】SRM 还会为大屏模式建一个软链接：若文件名纯数字，则 `<长 appid>.<ext>` → `<短 appid>.<ext>`。

#### 非 Steam 游戏的 AppID 算法

【事实】源码原文（`generate-app-id.ts`）：
```ts
function generatePreliminaryId(exe: string, appname: string) {
  const key = exe + appname;
  const top = BigInt(crc.crc32(key)) | BigInt(0x80000000);
  return (BigInt(top) << BigInt(32)) | BigInt(0x02000000);
}
export function shortenAppId(longId: string) { return String(BigInt(longId) >> BigInt(32)); }
export function generateShortcutId(exe: string, appname: string) {
  return Number((generatePreliminaryId(exe, appname) >> BigInt(32)) - BigInt(0x100000000));
}
```

步骤：
1. `key = exe + appname` —— **直接拼接，无分隔符、无 NUL 字节**；
2. `crc32(key)`（标准 IEEE/zlib CRC-32，无符号）；
3. **置高位** `| 0x80000000`（这一点常见猜测是对的）；
4. 左移 32 位再 `| 0x02000000` → **64 位「长 AppID」**（大屏网格文件名、SRM 内部键）；
5. **短 AppID**（网格文件名）= `longId >> 32` = `crc32(key) | 0x80000000`；
6. **写进 `shortcuts.vdf` 的 `appid`** = 短 AppID − `0x100000000`，即同样的比特**按有符号 Int32 重新解释成负数**（二进制 VDF 的 int 类型就是这么存的）。

#### SRM 能从哪些来源导入

⚠️【事实】**SRM 没有 EmulationStation、LaunchBox 或 Playnite 解析器。** 权威清单是 `available-parsers.ts` 里的 15 个 parser type：

```
"Glob", "Glob-regex", "Manual",
"Amazon Games", "Epic", "Legendary", "GOG Galaxy", "itch.io",
"Steam", "UPlay", "UWP", "EA Desktop", "Battle.net",
"Non-SRM Shortcuts", "GitHub Launcher"
```

- **ROM 类**只有 `Glob`、`Glob-regex`、`Manual`；
- `Manual` parser 吃的是 JSON：对象或对象数组，键为 `"title"`、`"target"`、`"startIn"`、`"launchOptions"`、`"appendArgsToExecutable"`；
- `Steam` 与 `Non-SRM Shortcuts` 是 **artwork-only**，「do not add shortcuts」，只重刷已有条目的美术；
- ES 风格的库只能走通用 Glob parser + **社区预设**（就是填好的 Glob 配置）。

### B.8 Skraper

【事实】**闭源免费软件**，厂商自述：「a FREE, non-commercial, non-profit application made by retrogaming fans for retrogaming fans.」网站未发布任何源码仓库。来源：<https://www.skraper.net/>

⚠️ 本节结论来自**下载并解包实际发行版**（`Skraper-1.4.1.7z`）后检查 `SkraperUI.exe` 与 `SkraperLibrary2.dll` 得出，因为官方没有文档站。

#### 输出格式：只有两种，面向三个前端

【事实】`SkraperLibrary2.dll` 里的前端枚举只有这些成员：
```
None, Unknown, Recalbox, Retropie, LaunchBox
```
向导只有三个按钮：`ButtonRecalbox`、`ButtonRetropie`、`ButtonLaunchbox`。

gamelist 写出器位于命名空间 `SkraperLibrary2.FrontEnds.Gamelists`：
- `EmulationStationList`（含嵌套 `+GameT`、`+FolderT`、`+ProviderT`）→ **EmulationStation `gamelist.xml`**。UI 字符串 `EmulationStation Gamelist.XML`，内部 id `gamelistxml`。
- `LaunchBoxList`（含 `+GameT`）→ **LaunchBox 平台 XML**。内部 id `launchboxxml`，UI 常量 `PLATEFORM.XML`（法式拼写）。

#### 对照表

| 格式 | Skraper 1.4.1 支持？ |
|---|---|
| EmulationStation `gamelist.xml` | ✅ |
| Recalbox | ✅（作为**前端配置档**，写的仍是 ES gamelist.xml） |
| RetroPie | ✅（同上） |
| LaunchBox | ✅ 自己的平台 XML |
| **Batocera** | ❌ 二进制中无任何相关字符串 |
| **Retrobat** | ❌ 无输出写出器；只有一个 `KillRetrobat` 进程终止函数 |
| **Attract-Mode** | ❌ 无 `attract`/`romlist` 字符串 |
| **Pegasus** | ❌ **无 `pegasus` 字符串** |
| **「Universal XML」** | ❌ 无此命名类型 |
| HyperSpin / GameEx / ES-DE / OnionOS | ❌ |

网站自述与之吻合：「Skraper currently supports EmulationStation metadata through RecalBox & Retropie. It can fill LaunchBox game list & images more accurately and faster than LaunchBox itself!」

⚠️ **重要推论**：**Skraper 不能输出 Pegasus 格式**。Pegasus 之所以「支持 Skraper」，是靠自己的 `SkraperAssetsProvider` 去**识别 Skraper 的媒体目录布局**（见 A.8），而不是 Skraper 主动产出 Pegasus 文件。

【事实】数据源是 ScreenScraper.fr；平台是 .NET Framework 4.8.1（Linux/macOS 经 mono）；最新版本 1.4.1。
⚠️【事实】**不存在官方文档站**——枚举 skraper.net 全部外链后，唯一随包文档是一份讲图片合成模板的 PDF。

### B.9 其它前端（简）

#### Daijishō（Android）

⚠️【事实】**仓库已迁移**：`github.com/magneticchen/Daijishou` → **<https://github.com/TapiocaFox/Daijishou>**（作者改名）。
⚠️【事实】**应用本身闭源**，README 原文：
> "Daijishō is currently **closed-source**. This repo is for assets and served as a main page."

**平台定义格式：纯 JSON**，`platforms/` 下每平台一个 `.json`（约 130 个）。

顶层键：`databaseVersion`、`revisionNumber`、`platform`、`playerList`
`platform` 对象：`name`、`uniqueId`、`shortname`、`description`、`acceptedFilenameRegex`、`scraperSourceList`、`boxArtAspectRatioId`、`useCustomBoxArtAspectRatio`、`customBoxArtAspectRatio`、`screenAspectRatioId`、`useCustomScreenAspectRatio`、`customScreenAspectRatio`、`retroAchievementsConsoleIdList`、`extra`
`playerList` 每项：`name`、`uniqueId`、`description`、`acceptedFilenameRegex`、`amStartArguments`、`killPackageProcesses`、`killPackageProcessesWarning`、`extra`

启动配置是 `amStartArguments`——字面的 Android `am start` 参数，模板标签为 `{file.path}`、`{file.uri}`、`{file.mime}`、`{file.name}` 及自定义 `{tags.*}`。
⚠️ **注意这些标签与 Pegasus 的 `{file.path}` / `{file.uri}` 写法一致**——这不是巧合，见下。

⚠️【事实】**Daijishō 不读 EmulationStation `gamelist.xml`**（全仓库搜 `gamelist` 零命中）。设置页只有三个导入入口：
- "Import platform" —— from generated JSON file
- **"Import from Pegasus"** —— generated config from 'Pegasus config generator'
- "Import favorite items" —— from generated JSON file

README 明确：
> "Is Daijishō a Pegasus fork? Nope. But you can import some config for emulators from pegasus."

→ ⚠️ **对 Pegasus 的互操作仅限「模拟器/启动配置」，不是 `metadata.pegasus.txt` 的游戏库导入器。**

刮削由 `scraperSourceList` 驱动，混合两类源：`LIBRETRO:<系统名>` 与 `DSESS:`（Daijishō 自有的网页刮削 mini 语言，有完整规范文档）。

#### ArkOS

⚠️【事实】**ArkOS 已停止维护**：仓库 <https://github.com/christianhaitian/arkos> 于 **2025-12-30 被作者归档**。wiki 原文：「ATTENTION! dArkOS has replaced ArkOS. ArkOS will no longer be maintained effective immediately.」

**前端：EmulationStation-FCAMOD**（维护者的 fork：<https://github.com/christianhaitian/EmulationStation-fcamod>），因此**元数据格式就是标准 EmulationStation `gamelist.xml`**，查找顺序：
```
[SYSTEM_PATH]/gamelist.xml
~/.emulationstation/gamelists/[SYSTEM_NAME]/gamelist.xml
/etc/emulationstation/gamelists/[SYSTEM_NAME]/gamelist.xml
```

#### HyperSpin

⚠️【未证实】官网 <https://hyperspin-fe.com> 在线且活跃，并以专门的 "Databases" 分类分发各系统的 XML 数据库（如「Super Nintendo Entertainment System - Database (Official)」，描述自称「This is the official Hyperlist database…」）。
**但常见的字段清单（`<game name=>` 含 `<description>`、`<cloneof>`、`<crc>`、`<manufacturer>`、`<year>`、`<genre>`、`<rating>`、`<enabled>`）无法从任何一手来源证实**，原因：
(a) 官方文档站 docs.hyperspin-fe.com 现重定向到 docs.hyperai.io，只讲 HyperSpin 2 / HyperHQ，无数据库 XML 页；
(b) 旧官方 MediaWiki 的 `Databases_and_Lists` 页在 Wayback CDX 索引中存在，但调研期间 archive.org 页面服务不可用；
(c) 论坛下载需要账号。
⚠️ 二手来源显示的样例还带 `image` 与 `index` **属性**，是常见清单里没有的。**按未证实处理。**

#### GameEx

⚠️【未证实】`gameex.com` 对本环境的所有请求（含浏览器 UA 与多种路径）一律返回 **Cloudflare "you have been blocked"** 拦截页，官方 wiki `gameex.info` 同样；archive.org 当时也不可用。
可报告的（来自搜索引擎对**官方页面**的索引，措辞是厂商的，但无法打开页面核实上下文，属**弱一手/二手**）：GameEx 是 **免费、闭源、仅 Windows** 的前端，2003 年由 Spesoft 创建，自述为「the most powerful, stable and feature rich gaming front-end (emulator launcher) for MAME, GameBase, Daphne, PC Games and all command line based game emulators」。
其元数据路径**不是**像 ES gamelist.xml 那样的单一可移植文本格式，而是**驱动 MAME 自己的 `listxml`/DAT 输出加 `.ini` 支持文件**，并辅以 GameBase 数据库与自有在线服务。
⚠️ **具体配置文件名与数据库引擎（Access/SQLite/其它）未证实。**

#### Logiqx DAT（顺带）

【事实】Pegasus 内置 `logiqx` provider，读取 `*.dat`，要求 DTD 为 `http://www.logiqx.com/Dats/datafile.dtd`，解析 `<datafile><header><name>/<description>` 与 `<game name=...>` 下的 `<year>`、`<description>`、`<manufacturer>`、`<rom name=...>`。
来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/logiqx/LogiqxProvider.cpp>



---

## C. 互转可行性分析

### C.1 字段映射矩阵

以一个**中立规范化模型**为中心。图例：

- **✅ 无损**：语义与取值域一一对应，往返不丢信息
- **🟡 有损**：有对应字段，但取值域/精度/基数不匹配，转换会丢东西
- **❌ 无对应**：目标格式没有这个概念，只能进扩展通道或丢弃
- **📁 旁路**：有这个数据，但**不在主元数据文件里**，需要额外读写另一个文件

#### 身份与文件

| 中立模型 | Pegasus | RetroPie ES | ES-DE | Batocera | Recalbox | LaunchBox | RetroArch | Attract-Mode | Playnite |
|---|---|---|---|---|---|---|---|---|---|
| `title` | `game:` ✅ | `name` ✅ | `name` ✅ | `name` ✅ | `name` ✅ | `Title` ✅ | `label` ✅ | `Title` ✅ | `Name` ✅ |
| `sort_title` | `sort-by` ✅ | `sortname` ✅ | `sortname` ✅ | `sortname` ✅ | ❌ | `SortTitle` ✅ | ❌ | ❌ | `SortingName` ✅ |
| `files[]`（多碟） | `files:` ✅ | ❌ 单 `path` | ❌ 单 `path` | ❌（有 `multidisk` 标记） | ❌ | `AdditionalApplication` + `Disc`/`SideA`/`SideB` ✅ | `subsystem_roms` 🟡 | ❌ | `Roms[]` ✅ |
| `file.path` | `file:` ✅ | `path` ✅ | `path`（须带 `./`）🟡 | `path` ✅ | `path` ✅ | `ApplicationPath` ✅ | `path` ✅ | ❌ **只存 Romname** | `GameRom.Path` ✅ |
| `uuid` | ❌ | ❌ | ❌ | `id`(属性，ScreenScraper) 🟡 | ❌ | `ID` ✅ | ❌ | ❌ | `Id`(Guid) ✅ |
| `crc32` | ❌ | ❌ | ❌ | `crc32` ✅ | `hash`(大写hex) 🟡 | ❌ | `crc32`（带 `\|crc` 后缀）🟡 | ❌ | ❌ |
| `md5` | ❌ | ❌ | ❌ | `md5` ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |

#### 描述性元数据

| 中立模型 | Pegasus | RetroPie ES | ES-DE | Batocera | Recalbox | LaunchBox | RetroArch | Attract-Mode | Playnite |
|---|---|---|---|---|---|---|---|---|---|
| `summary` | `summary` ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `description` | `description` ✅ | `desc` ✅ | `desc` ✅ | `desc` ✅ | `desc` ✅ | `Notes` ✅ | ❌ | ❌ | `Description`（**HTML**）🟡 |
| `developers[]` | `developer` ✅ 列表 | `developer` 🟡 单串 | `developer` 🟡 | `developer` 🟡 | `developer` 🟡 | `Developer` 🟡 | ❌ | `Manufacturer` 🟡 | `DeveloperIds[]` ✅ |
| `publishers[]` | `publisher` ✅ 列表 | `publisher` 🟡 | `publisher` 🟡 | `publisher` 🟡 | `publisher` 🟡 | `Publisher` 🟡 | ❌ | ❌ | `PublisherIds[]` ✅ |
| `genres[]` | `genre` ✅ 列表 | `genre` 🟡 单串 | `genre` 🟡 | `genre` 🟡 | `genre`+`genreid` 🟡 | `Genre` 🟡 | ❌ | `Category` 🟡 | `GenreIds[]` ✅ |
| `tags[]` | `tag` ✅ | ❌ | ❌ | `tags` 🟡 单串 | ❌ | ❌ | ❌ | `Tags` 📁(`.tag` 文件) | `TagIds[]` ✅ |
| `series` | ❌ | ❌ | ❌ | `family` 🟡 | ❌ | `Series`(`;`分隔) ✅ | ❌ | `Series` ✅ | `SeriesIds[]` ✅ |
| `release{y,m,d,precision}` | `release` 🟡 **精度丢失** | `releasedate` 🟡 | `releasedate` 🟡 | `releasedate` 🟡 | `releasedate` 🟡 | `ReleaseDate` 🟡 | ❌ | `Year` 🟡 只有年 | `ReleaseDate` ✅ **唯一原生支持精度** |
| `rating_0_1` | `rating` ✅ | `rating` ✅ 不取整 | `rating` 🟡 **取整到 0.1** | `rating` ✅ | `rating` 🟡 2位小数 | `CommunityStarRating` 🟡 **0–5 标度** | ❌ | `Rating` 🟡 自由文本 | `UserScore`/`CriticScore`/`CommunityScore` 🟡 **三个，0–100** |
| `age_rating` | ❌ | ❌ | ❌ | ❌ | `adult`(bool) 🟡 | `Rating`(**字符串**,ESRB) 🟡 | ❌ | ❌ | `AgeRatingIds[]` ✅ |
| `players_min` | ❌ **只存最大值** | ❌ | 🟡 字符串可写区间 | 🟡 字符串 | ✅ `range` 原生 | ❌ | ❌ | `Players` 🟡 | ❌ |
| `players_max` | `players` 🟡 | `players` int 🟡 | `players` 🟡 | `players` 🟡 | ✅ | `MaxPlayers` ✅ | ❌ | `Players` 🟡 | ❌ |
| `regions[]` | ❌ | ❌ | ❌ | `region` 🟡 | `region` 🟡 | `Region` 🟡 | ❌ | `Region` 🟡 | `RegionIds[]` ✅ |
| `languages[]` | ❌ | ❌ | ❌ | `lang` 🟡 | ❌ | ❌ | ❌ | `Language` 🟡 | ❌ |

#### 用户状态

| 中立模型 | Pegasus | RetroPie ES | ES-DE | Batocera | Recalbox | LaunchBox | RetroArch | Attract-Mode | Playnite |
|---|---|---|---|---|---|---|---|---|---|
| `favorite` | 📁 `favorites.txt` | `favorite` ✅ | `favorite` ✅ | `favorite` ✅ | 📁 `.ini` | `Favorite` ✅ | 📁 `content_favorites.lpl` | 📁 `.tag` | `Favorite` ✅ |
| `hidden` | ❌ | `hidden` ✅ | `hidden` ✅ | `hidden` ✅ | 📁 `.ini` | `Hide` ✅ | ❌ | ❌ | `Hidden` ✅ |
| `kidgame` | ❌ | `kidgame` ✅ | `kidgame` ✅ | `kidgame` ✅ | ❌ | ❌ | ❌ | ❌ | ❌ |
| `completed` | ❌ | ❌ | `completed` ✅ | ❌ | ❌ | `Completed` ✅ | ❌ | ❌ | `CompletionStatusId` 🟡 |
| `broken` | ❌ | ❌ | `broken` ✅ | ❌ | ❌ | `Broken` ✅ | ❌ | `Status` 🟡 | ❌ |
| `play_count` | 📁 `stats.db` | `playcount` ✅ | `playcount` ✅ | `playcount` ✅ | 📁 `.ini` | `PlayCount` ✅ | 📁 `.lrtl` | 📁 `.stat` 第1行 | `PlayCount` ✅ |
| `play_time_sec` | 📁 `stats.db` | ❌ **无此概念** | `playtime` ✅ | `gametime` ✅ | 📁 `timeplayed` | `PlayTime` ✅ | 📁 `.lrtl` `runtime` | 📁 `.stat` 第2行 | `Playtime` ✅ |
| `last_played` | 📁 `stats.db` | `lastplayed` ✅ | `lastplayed` ✅ | `lastplayed` ✅ | 📁 `.ini` | `LastPlayedDate` ✅ | 📁 `.lrtl` | ❌ **无此概念** | `LastActivity` ✅ |
| `sessions[]`（逐次会话） | ✅ **唯一原生支持**（`plays` 表） | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |

#### 启动与分组

| 中立模型 | Pegasus | RetroPie ES | ES-DE | Batocera | Recalbox | LaunchBox | RetroArch | Attract-Mode | Playnite |
|---|---|---|---|---|---|---|---|---|---|
| 平台/系统 | `collection`(逻辑) 🟡 | 目录决定 | 目录决定 | 目录决定 | 目录决定 | `Platform` ✅ | `db_name` 🟡 | `Emulator` 🟡 | `PlatformIds[]` ✅ 多值 |
| 二级分组 | **多 collection** ✅ | ❌ | `custom-*.cfg` ✅ | ❌ | ❌ | `Playlist` ✅ | 多个 `.lpl` 🟡 | `.tag` ✅ | `CategoryIds[]` ✅ |
| 系统级启动命令 | `collection` 的 `launch` ✅ | `es_systems.cfg` `<command>` ✅ | `es_systems.xml` 多 `<command label>` ✅ | `<emulators>/<cores>` ✅ | `emulator`/`core` 📁 | `Emulators.xml` ✅ | `default_core_path` ✅ | `.cfg` `executable`+`args` ✅ | — |
| **游戏级**启动命令 | `launch:` ✅ **任意命令行** | ❌ | `altemulator` 🟡 **只能选预定义 label** | `emulator`/`core` 🟡 | 📁 `emulator`/`core` 🟡 | `CommandLine`+`Emulator` ✅ | `core_path` 🟡 | ❌ | `GameActions[]` ✅ |
| 工作目录 | `workdir` ✅ | ❌ | `%STARTDIR%=` 🟡 | ❌ | ❌ | ❌ | ❌ | `workdir`（模拟器级）🟡 | `GameAction.WorkingDir` ✅ |
| 扫描规则（扩展名） | `extension` ✅ | `<extension>` ✅ | `<extension>` ✅ | `<extension>` ✅ | ✅ | ❌ 显式条目 | `scan_file_exts` ✅ | `romext` ✅ | — |
| **扫描规则（正则）** | `regex` / `ignore-regex` ✅ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | — |

⚠️ **矩阵读出的四条硬结论**

1. **`sessions[]`（逐次游玩会话）只有 Pegasus 有**——任何 Pegasus → X 都会把它降采样成三个聚合值，且**不可逆**。
2. **`regex` / `ignore-regex` 扫描规则只有 Pegasus 有**——转出时必须「求值展开」成显式文件列表，转入时无法还原为规则。
3. **日期精度只有 Playnite 原生支持**——其余格式里「只知道年份」与「1 月 1 日」不可区分。
4. **用户状态的存放位置极度分裂**：Pegasus、RetroArch、Attract-Mode、Recalbox 都把它放在**主元数据文件之外**。只处理主文件的转换工具会 100% 丢失这些数据。



### C.2 表达能力差异（结构层面，比字段更难对齐）

#### ① 分组概念：collection vs system/folder vs Platform+Playlist

| 格式 | 分组机制 | 关键性质 |
|---|---|---|
| **Pegasus** | `collection` | **纯逻辑分组**，与目录无关；一个游戏可属于任意多个 collection；同名 collection 跨文件自动合并 |
| ES / ES-DE | `system`（由 `es_systems.xml` 定义）+ `<folder>` | **一个游戏只属于一个 system**（由它所在的 ROM 目录决定）；`folder` 是文件系统的镜像 |
| ES-DE 自定义收藏 | `collections/custom-*.cfg` | 与 system 正交的第二维；每行一个 `%ROMPATH%` 路径 |
| **LaunchBox** | `Platform` + `Playlist` | 两维分离：Platform 唯一，Playlist 可多属 |
| RetroArch | playlist（`.lpl`） | 一个游戏可出现在多个 `.lpl` 里，但每次都是**完整条目复制** |
| Attract-Mode | romlist + `display` | romlist 通常对应一个 emulator；`.tag` 提供第二维 |

⚠️ **核心不匹配**：
- **Pegasus → ES 方向是有损的**：Pegasus 允许一个游戏属于 N 个 collection，而 ES 的 system 是由目录唯一决定的。只能选一个 system 当"主"，其余 collection 降级为**自定义收藏文件**。
  ⚠️ 更正一个常见误解：**原版 / RetroPie ES 也有自定义收藏**（`~/.emulationstation/collections/custom-<名字>.cfg`），只是**每行写绝对路径、没有 `%ROMPATH%` 变量**（ES-DE 才有）。所以这条降级路径在两者上都可行，只是可移植性不同。
- **ES → Pegasus 方向是无损的**：system → collection，folder 信息可用 `x-` 保留。
- **Pegasus 的 collection 同时携带扫描规则（extension/regex/ignore-*）**，这在 ES 里对应 `es_systems.xml` 的 `<extension>`，但 ES **没有 regex 和 ignore 机制**。⚠️ Pegasus 的 `regex:` / `ignore-regex:` **在所有其它格式中都无对应物**，只能在转换时"求值展开"成显式文件列表。

#### ② 多文件游戏（多碟）

| 格式 | 表达方式 | 能力 |
|---|---|---|
| **Pegasus** | `files:` 多行 | ✅ **原生**。启动时弹出文件选择器 |
| ES / ES-DE | ✗ | 通常靠 `.m3u`/`.cue` 等**容器文件**绕过，或每碟一个条目 |
| LaunchBox | `<AdditionalApplication>` | ✅ 原生 |
| RetroArch | `subsystem_roms[]` | ✅ 但语义是"子系统多卡带"，不完全等同多碟 |
| Attract-Mode | ✗ | 一行一个 Romname |

⚠️ **Pegasus 的多碟 → ES 必然有损**：只能挑一个主文件，或要求外部先生成 `.m3u`。反向（ES 的 `.m3u` → Pegasus）则可以但不必展开。

#### ③ 自定义启动命令

| 格式 | 粒度 |
|---|---|
| **Pegasus** | collection 级 `launch` + **游戏级 `launch` 覆盖** + `workdir` |
| ES-DE | system 级多 `<command label>` + **游戏级 `altemulator`（只能选已定义的 label）** |
| 原版 ES | system 级 `<command>`，**无游戏级覆盖** |
| LaunchBox | `<Emulator>` 引用 + `<CommandLine>` + `UseDosBox` 等 |
| Attract-Mode | emulator 级 `executable` + `args`，**无游戏级覆盖** |
| RetroArch | 条目级 `core_path` / `core_name` |

⚠️ **Pegasus 的游戏级 `launch` 表达力最强（任意命令行）**，ES-DE 的 `altemulator` 只能从预定义 label 里选。
⚠️ **Pegasus → ES-DE**：任意命令行无法塞进 `altemulator`，除非先在 `es_systems.xml` 里为它造一个 `<command label>`。这是**结构性有损**。
⚠️ **Pegasus → Attract-Mode / 原版 ES**：游戏级自定义命令**完全无处安放**。

#### ④ 评分标度

| 格式 | 标度 | 备注 |
|---|---|---|
| Pegasus | 0.0–1.0 float | 写入接受 `NN%` 或 `0.x` |
| ES / ES-DE | 0.0–1.0 float | ES-DE **四舍五入到 0.1**（半星） |
| LaunchBox | `Rating`（ESRB 等文本）与 `CommunityStarRating`/`StarRating`（5 分制）分离 | 两个不同概念 |
| Playnite | `UserScore` / `CriticScore` / `CommunityScore`，均 0–100 整数 | **三个独立评分** |
| Attract-Mode | `Rating` 自由文本 | 无标度约定 |

⚠️ **多对一塌缩风险**：Playnite 有三个评分、LaunchBox 有两个，而 Pegasus/ES 只有一个。转换时**必须让用户选择用哪个作为源**，其余进扩展通道。
⚠️ ES-DE 的 0.1 取整意味着 **Pegasus → ES-DE → Pegasus 的 rating 往返必然有损**（0.87 → 0.9）。

#### ⑤ 日期格式

| 格式 | 格式 | 精度表达 |
|---|---|---|
| Pegasus（文件） | `YYYY` / `YYYY-MM` / `YYYY-MM-DD` | ✅ 可写 |
| Pegasus（内存） | `QDate` | ❌ **精度丢失**，缺失部分补 1 |
| ES / ES-DE / Batocera | `%Y%m%dT%H%M%S`（如 `19950311T000000`） | ❌ 只能靠 `0101` 之类约定暗示 |
| LaunchBox | .NET `XmlSerializer` 的 ISO 8601 | ❌ |
| Playnite | 专门的 `ReleaseDate` 结构 | ✅ 支持只到年/年月 |
| Attract-Mode | `Year` 单字段 | 只有年 |

⚠️ **只有 Playnite 原生支持日期精度**。其余格式里"只知道年份"和"1 月 1 日"不可区分。

#### ⑥ 地区 / 语言

| 格式 | 字段 |
|---|---|
| Pegasus | ❌ **无原生字段**（只能用 `tag` 或 `x-`） |
| ES-DE | ❌ 无 |
| Batocera | `<region>`、`<lang>` |
| LaunchBox | `<Region>` |
| Attract-Mode | `Region`、`Language` |
| Playnite | `Regions[]` |

⚠️ **Pegasus 缺地区/语言字段**，是 Batocera/LaunchBox/AM → Pegasus 的固定损失点，必须走 `tag:` 或 `x-region:`。

### C.3 媒体资源转换

#### 三种根本不同的定位范式

| 范式 | 代表 | 特点 |
|---|---|---|
| **① 元数据内显式路径** | 原版 ES、RetroPie ES、Batocera、LaunchBox（XML 里有 `VideoPath`/`ManualPath`） | 路径任意，转换时只需改写字符串 |
| **② 目录约定 + 文件名匹配** | Pegasus 的 `media/`、ES-DE 的 `downloaded_media/`、RetroArch 的 `thumbnails/`、LaunchBox 的 `Images/`、Attract-Mode 的 `artwork` 路径 | 路径由规则算出，**必须物理放对位置** |
| **③ 远程 URL** | Pegasus（`assets.*` 接受 `http(s)://`） | 无需本地文件 |

⚠️ **范式 ① → ② 是转换中真正的工作量所在**：不是改几个字符串，而是要**搬动/复制成百上千个文件**，且每种目标格式的目录名和文件名规则都不一样。

#### 同一张图在各格式中的落点对照（以「正面盒图」为例）

| 格式 | 位置 | 文件名依据 |
|---|---|---|
| Pegasus（约定） | `<gamedir>/media/<game>/boxFront.png` | 目录名 = ROM 去扩展名 **或** 游戏标题 |
| Pegasus（显式） | 任意 | `assets.box_front:` 指定 |
| ES-DE | `~/ES-DE/downloaded_media/<system>/covers/<相对路径>/<ROM去扩展名>.png` | **ROM 文件名**，且镜像子目录结构 |
| RetroArch | `thumbnails/<db_name>/Named_Boxarts/<净化后的 label>.png` | **播放列表 label**（净化后） |
| LaunchBox | `LaunchBox/Images/<Platform>/Box - Front/<净化后的 Title>-01.png` | **游戏标题**（净化后）+ `-NN` 序号 |
| Attract-Mode | emulator `.cfg` 里 `artwork flyer <路径>` 指定的目录下 `<Romname>.png` | **ROM 基名** |
| Skraper 布局 | `<gamedir>/media/box2dfront/<ROM名>.png` | ROM 名 |

⚠️ **匹配键在三者之间来回横跳：ROM 文件名 / 游戏标题 / 播放列表 label。** 这是媒体转换最容易出错的地方，也是中立模型必须同时保留「文件名」和「标题」两个键的原因。

#### 复制 / 硬链接 / 软链接的取舍

【未证实】**没有任何一个前端的官方文档对此作出约束**——它们只规定「文件要在那个位置、叫那个名字」，不关心 inode 层面怎么实现。因此这纯粹是工程取舍：

| 方式 | 优点 | 缺点 |
|---|---|---|
| **复制** | 最安全、跨文件系统/跨设备可用、目标独立 | 占用 N 倍磁盘；源文件更新后不同步 |
| **硬链接** | 零额外空间、读取性能同原文件 | **不能跨文件系统**；对同一 inode 的原地修改会波及所有引用；FAT/exFAT（大量便携掌机 SD 卡的格式）**不支持** |
| **软链接** | 零额外空间、可跨文件系统 | 源移动即失效；**FAT/exFAT 不支持**；Windows 需要权限；部分前端/扫描器可能不跟随 |

【事实】相关的一个已知约束：Pegasus 的媒体扫描器**显式启用了跟随符号链接**（`QDirIterator::FollowSymlinks`），Attract-Mode 的 ROM 扫描注释里也提到"非递归链接会被包含"（ES-DE `<path>` 说明："All subdirectories (and non-recursive links) will be included"）。所以在这些前端下软链接是可行的。
来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_media/MediaProvider.cpp>、<https://gitlab.com/es-de/emulationstation-de/-/blob/master/INSTALL.md#es_systemsxml>

【推断】实践建议：
- **默认硬链接，失败自动回退到复制**（跨文件系统、FAT 时）；
- 提供 `--media-mode={copy,hardlink,symlink,reference}` 开关；
- `reference` 模式只改写元数据里的路径、不动文件——**仅对范式 ① 的目标格式可用**。

#### 资源类型的语义损失

- Pegasus 有 **22 种**规范类型，ES-DE 有 **12 个**媒体目录，RetroArch 只有 **4 个**（Boxarts/Snaps/Titles/Logos）。
- ⚠️ **多对一塌缩**是常态：Pegasus 官方的 LaunchBox 导入器就把 `Cart - Front`、`Cart - Back`、`Disc`、`Fanart - Disc` 等**全部映射到同一个 `CARTRIDGE`**；把 `Advertisement Flyer - Front/Back` 都映射到 `POSTER`。
  来源：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/launchbox/LaunchBoxAssets.cpp>
- ⚠️ 转成 RetroArch 时，除盒图/截图/标题画面/Logo 外的**所有资源类型都无处可放**。
- 【事实】ES-DE 的 `miximages` 是**合成图**，由 ES-DE 自己生成，不应从其它格式导入。

### C.4 路径与转义

#### 相对 / 绝对路径

| 格式 | 路径基准 |
|---|---|
| Pegasus | `file:` 值相对**元数据文件所在目录**，也接受绝对路径；资源同理，另接受 `http(s)://` |
| ES / RetroPie ES | `<path>` 惯例为相对 gamelist 所在的系统目录，写作 `./Game.zip` |
| ES-DE | `<path>` 相对 `%ROMPATH%` 变量，或绝对路径 |
| ES-DE 自定义收藏 | 强制 `%ROMPATH%/<system>/<file>` 形式 |
| LaunchBox | `ApplicationPath` 可为相对 LaunchBox 根目录的相对路径，也可绝对 |
| RetroArch | `path` 通常为**绝对路径**；压缩包内用 `archive.zip#member` |
| Attract-Mode | romlist 里存的是 **Romname（基名）**，真实路径由 emulator `.cfg` 的 `rompath` + `romext` 拼出 |

⚠️ **Attract-Mode 是唯一一个「不存路径、只存基名」的格式**。这意味着：
- X → Attract-Mode：必须把目录信息挪到 emulator 配置里，且**同一 romlist 内的所有游戏必须共享 rompath 集合**；
- Attract-Mode → X：**必须读取 emulator `.cfg` 才能还原路径**，单看 romlist 无法转换。

#### Windows / Linux 差异

- 【事实】ES-DE 的 `<command>` 在 Windows 下「`.exe` 扩展名可选，正斜杠与反斜杠都允许作为目录分隔符」。
- 【事实】Pegasus 的占位符**原样替换、不加引号**，含空格路径必须自己加引号。
- 【事实】ES-DE 明确提醒 Linux 下媒体文件名**大小写敏感且扩展名须小写**。
- 【推断】跨平台转换时，盘符（`C:/`）与 `~`、`%ROMPATH%`、`%ESPATH%`、`$LAYOUT` 等变量互不相通，必须由转换工具显式地"落地"或"抽象化"。

#### 文件名/值中特殊字符的转义规则对照

| 格式 | 转义机制 | 危险字符 |
|---|---|---|
| **Pegasus 元数据** | 无通用转义。按**第一个** `:` 切分，故值中的 `:` 安全 | 值首行不能以空白开头；行首 `#` 不可表达；`\n` 是换行转义，字面 `\n` 需写 `\\n` |
| **XML（ES / ES-DE / Batocera / LaunchBox / Hyperspin）** | 标准 XML 实体 `&amp; &lt; &gt; &quot; &apos;` | 控制字符在 XML 1.0 中非法 |
| **RetroArch .lpl** | 标准 JSON 转义 | 无（JSON 完备） |
| **RetroArch 缩略图文件名** | **有损净化**：11 个字符 → `_` | ``& * / : ` " < > ? \ |`` |
| **Attract-Mode romlist** | **不对称**：含 `;` 才加引号并转义 `"`；否则原样 | 含 `"` 但不含 `;` 的值无法往返；首尾空格丢失 |
| **LaunchBox 图片文件名** | **有损净化**：`< > : " / \ | ? * '` → `_` | 注意**撇号 `'` 也被替换** |

⚠️ **最关键的一条**：RetroArch 与 LaunchBox 的图片文件名净化都是**多对一**的（多个不同标题可能映射到同一文件名），因此**从图片文件名反推游戏标题是不可逆的**。转换工具必须**以元数据条目为主键**去关联媒体，绝不能反过来靠文件名去猜标题。

【事实】Skyscraper（一个真实存在的多前端输出工具）在生成 Pegasus 输出时，会把描述中的半角冒号 `:`（U+003A）替换为 **`꞉` MODIFIER LETTER COLON（U+A789）**，并打印日志告知用户。
源码：<https://github.com/Gemba/skyscraper/blob/master/src/pegasus.cpp>（`Pegasus::replaceColon`）
【推断】源码未说明理由。按 Pegasus 现行解析器的规则，续行中的冒号其实是安全的，因此这更可能是**防御性处理**（兼容旧版本或第三方解析器）。转换工具**不必**照做，但需要知道存量 Pegasus 文件里可能出现这个字符。

### C.5 用户状态数据（favorite / play count / last played / play time）

这是**跨格式转换损失最集中的地方**，因为各家把它放在完全不同的位置。

| 格式 | 收藏 | 游玩次数 | 最后游玩 | 游玩时长 | 存放位置 |
|---|---|---|---|---|---|
| **Pegasus** | `favorites.txt`（每行一路径） | 由 `stats.db` 聚合 | 由 `stats.db` 聚合 | 由 `stats.db` 聚合 | **独立于元数据文件** |
| ES / RetroPie ES | `<favorite>` | `<playcount>` | `<lastplayed>` | ✗ **无此概念** | gamelist.xml 内 |
| ES-DE | `<favorite>` | `<playcount>` | `<lastplayed>` | `<playtime>`（秒） | gamelist.xml 内 |
| Batocera | `<favorite>` | `<playcount>` | `<lastplayed>` | `<gametime>`（秒） | gamelist.xml 内 |
| **Recalbox** | `favorite` | `playcount` | `lastplayed` | `timeplayed`（秒） | ⚠️ **全部在 `gamelist-userdata.ini`**，不在 XML 里 |
| RetroArch | `content_favorites.lpl`（独立播放列表） | `.lrtl` 的 `play_count` | `.lrtl` 的 `last_played` | `.lrtl` 的 `runtime` | **三处分离** |
| Attract-Mode | `romlists/<name>.tag` | `.stat` 第 1 行 | ✗ | `.stat` 第 2 行（秒） | **两处分离，且都不在 romlist 里** |
| LaunchBox | `<Favorite>` | `<PlayCount>` | `<LastPlayedDate>` | `<PlayTime>` | Platform XML 内 |
| Playnite | `Favorite` | `PlayCount` | `LastActivity` | `Playtime`（秒） | 库内 |

#### 关键结论

1. ⚠️ **Pegasus 的元数据文件里根本没有用户状态字段**。因此「把 ES gamelist.xml 转成 metadata.pegasus.txt」时，`favorite` / `playcount` / `lastplayed` **无处可放**——写成 `x-favorite:` 只是给自己留档，Pegasus 本身不会据此点亮收藏。要真正迁移收藏，必须**另外生成 `<config dir>/favorites.txt`**。
2. ⚠️ **Pegasus 的游玩记录粒度最细**（`plays` 表逐次会话记 `start_time` + `duration`），转成任何其它格式都是**降采样**：只能算出总次数/总时长/最后一次。反向则**永远无法还原**逐次会话。
3. ⚠️ **Pegasus 与 Attract-Mode 的状态都以「键」关联而非游戏 ID**：Pegasus 用**文件绝对路径**，Attract-Mode 用 **`Emulator` + `Romname`**。路径或 ROM 改名后统计即失联。
4. ⚠️ **原版 ES / RetroPie ES 没有 `playtime`**，ES-DE 才有。ES → ES-DE 方向该字段为空；反向则丢失。
5. ⚠️ **Attract-Mode 没有「最后游玩时间」**，只有次数与总时长。任何 → Attract-Mode 都会丢 `lastplayed`。
6. ⚠️ **Recalbox 把 8 个字段整体搬出了 XML**（`favorite`/`hidden`/`emulator`/`core`/`ratio`/`playcount`/`lastplayed`/`timeplayed` → `gamelist-userdata.ini`，且那个 `.ini` **不是标准 INI 格式**）。只读 `gamelist.xml` 的工具在 Recalbox 上会看到一个「没有任何用户状态」的库。
7. ⚠️ **RetroArch 的收藏是一个独立播放列表**（`content_favorites.lpl`），条目里再次完整重复 `path`/`label`/`core_path` 等；它不是游戏上的一个布尔标志。转换时需要**生成/解析一个额外文件**。

【推断】综上，任何声称"支持用户状态迁移"的转换工具，都**必须能读写每个格式的多个旁路文件**，而不能只处理主元数据文件。这应当是中立模型的一等公民，而不是事后补丁。

### C.6 结论：哪些方向可以往返无损

> 定义：**往返无损（round-trip lossless）** = A 格式 → B 格式 → A 格式后，A 的内容与原始逐字节/逐语义等价。
> 下面的判断均为【推断】，但每条都指向前文列出的具体事实依据。

#### ❌ 没有任何一对主流格式是天然往返无损的

根本原因有三个，且都是**结构性**的：

1. **用户状态存放位置不同**（C.5）——只要转换器不处理旁路文件，状态必丢。
2. **表达能力不对等**（C.2）——多碟、多集合归属、游戏级启动命令、日期精度、评分标度，至少有一项在任一格式对里不匹配。
3. **媒体文件名是有损单向函数**（C.4）——RetroArch/LaunchBox 的净化规则多对一。

#### 分方向评估

| 方向 | 可行性 | 主要损失 |
|---|---|---|
| **ES → Pegasus** | 🟢 **接近无损** | system→collection 是 1:1；ES 字段是 Pegasus 的子集。损失：`playcount`/`lastplayed`/`favorite` 需另写旁路文件；`hidden`/`kidgame` 无对应（走 `x-`） |
| **Pegasus → ES** | 🟡 **有损** | 多 collection 归属只能保一个；多碟 `files:` 只能保一个；游戏级 `launch` 无处放；`regex`/`ignore-*` 必须展开求值；`tag` 无对应 |
| **ES ↔ ES-DE** | 🟢 **ES→ES-DE 近无损**；🟡 **反向有损** | ES-DE 独有字段（`altemulator`、`playtime`、`controller`、`nogamecount`、`nomultiscrape`、`hidemetadata`、`collectionsortname`、`folderlink`）在原版 ES 无对应；且媒体范式从「XML 内路径」变成「downloaded_media 约定」，需物理搬文件 |
| **ES ↔ Batocera** | 🟡 | 同为 gamelist.xml，但 Batocera 有大量独有元素；⚠️ ES-DE 的清理工具官方警告会**清除它不认识的数据** |
| **任意 → RetroArch** | 🔴 **重度有损** | `.lpl` 只有 6 个核心字段（path/label/core_path/core_name/crc32/db_name）——**开发商、发行商、类型、描述、发行日期、评分、人数全部无处安放**。媒体只剩 4 类且文件名被净化 |
| **RetroArch → 任意** | 🟢 信息量本来就少，不会丢 | 但也几乎没什么可转的 |
| **任意 → Attract-Mode** | 🟡 | 描述（`desc`）无对应字段；`lastplayed` 无对应；路径信息必须外移到 emulator `.cfg`；⚠️ 含 `"` 不含 `;` 的值有转义 bug |
| **LaunchBox → Pegasus** | 🟡 | LaunchBox 字段远多于 Pegasus；`CustomField`、双评分体系、`Region`、`Series`、`Source`、`Version` 等需走 `x-` |
| **Playnite → 任意 ROM 前端** | 🔴 | Playnite 是 PC 游戏库模型（GUID、插件来源、安装状态、`GameActions`），与 ROM 前端的"文件+平台"模型**范式不同** |

#### 什么情况下**可以**做到往返无损

【推断】只有满足以下全部条件：

1. 转换是 **A → B → A**，且中途 **B 侧未被人工编辑**；
2. 转换器在 A→B 时把**完整的 A 侧中立模型快照**写入 sidecar（见 D.3 第二级），B→A 时优先从 sidecar 恢复；
3. 媒体采用 `reference` 或 `hardlink` 模式，**原始文件未被改名**。

换言之：**往返无损不是格式的性质，而是转换器的性质**——必须靠 sidecar 兜底，而非指望字段能一一对上。

#### 实践上的"够用无损"

【推断】若目标是"用户感知不到损失"，优先保证这几项（按重要性）：
1. 标题、文件路径、平台归属（**决定能不能启动**）；
2. 收藏与游玩统计（**用户最在意、最不可重建**）；
3. 盒图/截图/视频三类主力媒体；
4. 描述、开发商、发行日期、类型（**可由刮削器重建，优先级最低**）。

⚠️ 关键洞察：**元数据可以重新刮削，用户状态不能。** 转换工具应把 favorites/playtime 的保真放在比 description 更高的优先级上——而这恰恰是最容易被忽略的部分，因为它们不在主元数据文件里。

---

## D. 对通用元数据转换工具的可执行建议

> 本节整体属于【推断】——是从前面的事实推导出的设计建议，各前端官方并未规定这些。

### D.1 中立模型该长什么样

核心判断：**不要以"游戏"为唯一实体**。前面的事实显示至少需要五类实体，否则必然丢信息。

```
Library                     # 一次转换的作用域
├── Platform[]              # ES 的 system / LaunchBox 的 Platform / AM 的 emulator
│   ├── id, name, shortname
│   ├── rom_dirs[], extensions[], exclude_rules[]
│   └── launchers[]         # ★ 列表，不是单值（ES-DE altemulator / LaunchBox Emulator）
├── Collection[]            # ★ 与 Platform 正交：Pegasus collection / LaunchBox Playlist / ES 自定义收藏
│   └── member_refs[]
├── Game[]
│   ├── identity   { title, sort_title, platform_ref, uuid }
│   ├── files[]    # ★ 必须是列表（多碟）
│   │   └── { path, label, launcher_override, crc32, md5, region, disc_no }
│   ├── descriptive{ summary, description, developers[], publishers[],
│   │                genres[], tags[], series, players_min, players_max,
│   │                release{y,m,d,precision}, rating_0_1, age_rating,
│   │                regions[], languages[] }
│   ├── state      { favorite, hidden, kidgame, completed, broken,
│   │                play_count, play_time_sec, last_played,
│   │                sessions[]?  }   # ★ Pegasus 才有的逐次会话
│   ├── media[]    # ★ 列表，见下
│   └── extensions { <source_format>: { <key>: <value> } }   # ★ 保真通道
└── Folder[]                # ES/ES-DE 的 <folder> 元数据
```

**必须是列表而不是单值的字段**（每一条都有事实依据）：
- `files[]` —— Pegasus `files:`、LaunchBox `AdditionalApplication`、RetroArch `subsystem_roms`；
- `launchers[]` —— ES-DE 的多 `<command label=...>` + `altemulator`；
- `developers[]` / `publishers[]` / `genres[]` —— Pegasus 是列表，ES 系是单字符串（转换方向决定是 join 还是 split）；
- `media[]` —— Pegasus 每种类型都支持多值（`boxFrontList`），LaunchBox 有 `-01`/`-02` 序号。

### D.2 必须支持的扩展点

| 扩展点 | 为什么必须有（事实依据） |
|---|---|
| **1. 值域适配器（scale adapter）** | rating 在 Pegasus/ES/ES-DE 是 0–1 浮点，LaunchBox 是 5 分制，AM 是自由文本。**必须集中一处转换**，且要记住 ES-DE 会四舍五入到 0.1。 |
| **2. 日期精度模型** | Pegasus 接受 `YYYY`/`YYYY-MM`/`YYYY-MM-DD` 但**解析后丢精度**；ES 系用 `YYYYMMDDT HHMMSS`；Playnite 有专门的 `ReleaseDate` 结构。中立模型必须显式带 `precision` 字段。 |
| **3. 文件名净化器（可插拔）** | 至少要内置 RetroArch 的 11 字符规则和 LaunchBox 的 10 字符规则，且**必须是「正向单向函数」**——不要试图反解。 |
| **4. 媒体类型映射表（可覆盖）** | 官方已有两张现成的表可直接复用：Pegasus 的 Skraper 映射表和 LaunchBox 映射表（见 A.8）。用户应能覆盖。 |
| **5. 旁路文件读写器（sidecar I/O）** | 用户状态几乎全在旁路文件里：`favorites.txt`、`stats.db`、`.lrtl`、`.stat`、`.tag`、`content_favorites.lpl`。**没有这一层就等于不支持状态迁移。** |
| **6. 直通字段仓（passthrough store）** | 见下。 |
| **7. 媒体落地策略** | `copy` / `hardlink` / `symlink` / `reference`，带跨文件系统自动降级。 |
| **8. 路径变量解析器** | `%ROMPATH%`、`%ESPATH%`、`~`、`$LAYOUT`、`{file.path}` 等互不相通，需要统一"展开/收拢"。 |

### D.3 无法映射字段的保留策略（分级）

按**保真优先、且不污染目标格式**的原则，建议三级：

**第一级：目标格式的原生扩展机制（首选）**
- → Pegasus：用 **`x-*` 键**。这是官方指定用途（"could be used, for example, by other softwares (eg. scrapers) to store some program-specific data"），且能被主题通过 `game.extra.*` 读到。
  建议命名 `x-<源格式>-<原字段名>`，例如 `x-launchbox-communitystarrating: 4.5`。
- → LaunchBox：用 **`<CustomField>`**（Name/Value/GameID）。
- → ES / ES-DE：⚠️ **不要**塞自定义 XML 元素。ES-DE 官方明确警告清理工具"**data unknown to ES-DE may get purged**"。
- → RetroArch：⚠️ 未知 JSON 键**读得进但重写即丢**，不可依赖。

**第二级：转换工具自己的 sidecar 文件（推荐作为默认）**
在输出目录旁写一个 `.metaxfer.json`（或类似），保存**完整的中立模型快照 + 源格式原文摘要**。
理由：这是唯一能做到「A → B → A 往返无损」的通用手段，且不会破坏目标格式。

**第三级：转换报告（必须有）**
一份人类可读的 `conversion-report.md`，逐条列出：丢弃的字段、被塌缩的值、被净化改名的媒体、无法定位的文件。**有损转换必须让用户看见损在哪里**，这比悄悄丢弃重要得多。

### D.4 工程上的几条硬性建议

1. **以「读入→中立模型→写出」的两段式，禁止写点对点转换器。** N 个格式点对点是 N² 条路径，且每条都会各自漏掉边界情况。
2. **主键选择：不要用标题，用（platform, 规范化后的文件路径）。** 事实依据：Pegasus 的 favorites/stats 用路径，Attract-Mode 用 emulator+romname，只有 LaunchBox/Playnite 有稳定 GUID。标题在净化后会多对一碰撞。
3. **媒体一律以元数据条目为源头去推导文件名，永不反向。**（见 C.4 的结论）
4. **默认 dry-run。** 转换会搬动大量文件并可能覆盖用户手工维护的 gamelist——Skyscraper 的官方文档专门用一个 danger 框警告这一点。
5. **写出前先读入目标文件并合并。** 所有成熟工具都这么做：Skyscraper 对每个前端都有明确的「Metadata preservation」清单（ES 保留 `favorite/hidden/kidgame/lastplayed/playcount/sortname`，ES-DE 再加 `altemulator/broken/collectionsortname/completed/controller/hidemetadata/nogamecount/nomultiscrape`，AM 保留 `altromname/alttitle/buttons/cloneof/control/displaycount/displaytype/extra/rotation/status`）。
   来源：<https://github.com/Gemba/skyscraper/blob/master/docs/FRONTENDS.md>
6. **对 Pegasus 输出要专门处理三件事**：`rating` 必须带 `%` 或写成 `0.xx`；`players` 只能写单值或区间但会被折成最大值；**收藏要另写 `favorites.txt`**。

---

## 附录：来源清单

### Pegasus Frontend
- 元数据文件文档：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/meta-files.md>
- 资源查找文档：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/meta-assets.md>
- 第三方数据源文档：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/meta-sources.md>
- **开发者向语法规范**：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/dev/meta-syntax.md>
- 主题 API（数据模型）：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/themes/api.md>
- 破坏性变更：<https://github.com/mmatyas/pegasus-docs/blob/master/docs/user-guide/breaking-changes.md>
- 配置目录：<https://pegasus-frontend.org/docs/user-guide/config-dirs/>
- 低层解析器源码：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/parsers/MetaFile.cpp>
- 元数据语义源码：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_metadata/PegasusMetadata.cpp>
- 资源类型枚举：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/types/AssetType.h>
- 资源名映射：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/PegasusAssets.cpp>
- media 目录扫描：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_media/MediaProvider.cpp>
- 收藏存储：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_favorites/Favorites.cpp>
- 游玩统计存储：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/pegasus_playtime/PlaytimeStats.cpp>
- ES 导入器：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/es2/Es2Metadata.cpp>
- LaunchBox 资源映射：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/launchbox/LaunchBoxAssets.cpp>
- Skraper 资源映射：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/skraper/SkraperAssetsProvider.cpp>
- provider 目录总览：<https://github.com/mmatyas/pegasus-frontend/tree/master/src/backend/providers>
- 官方转换工具：<http://pegasus-frontend.org/tools/convert/>
- 官方图形化元数据编辑器：<https://github.com/mmatyas/pegasus-metadata-editor>

### EmulationStation 家族
- ES-DE gamelist.xml 官方参考：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/INSTALL.md#gamelistxml>
- ES-DE es_systems.xml 官方参考：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/INSTALL.md#es_systemsxml>
- ES-DE 用户指南（媒体目录、自定义收藏）：<https://gitlab.com/es-de/emulationstation-de/-/blob/master/USERGUIDE.md>
- RetroPie EmulationStation 源码：<https://github.com/RetroPie/EmulationStation>

### RetroArch
- 播放列表读写源码：<https://github.com/libretro/RetroArch/blob/master/playlist.c>
- 播放列表枚举：<https://github.com/libretro/RetroArch/blob/master/playlist.h>
- **缩略图路径与文件名净化**：<https://github.com/libretro/RetroArch/blob/master/gfx/gfx_thumbnail.c>
- 运行时日志 `.lrtl`：<https://github.com/libretro/RetroArch/blob/master/runtime_file.c>
- 压缩包路径分隔：<https://github.com/libretro/RetroArch/blob/master/libretro-common/file/file_path.c>
- 特殊文件名常量：<https://github.com/libretro/RetroArch/blob/master/file_path_special.h>
- 官方缩略图文档（⚠️ 净化字符集有遗漏）：<https://github.com/libretro/docs/blob/master/docs/guides/roms-playlists-thumbnails.md>

### Attract-Mode
- romlist 字段定义与转义：<https://github.com/mickelson/attract/blob/master/src/fe_info.cpp>
- 字段枚举：<https://github.com/mickelson/attract/blob/master/src/fe_info.hpp>
- 路径常量、资源查找、统计：<https://github.com/mickelson/attract/blob/master/src/fe_settings.cpp>
- romlist / tag 文件：<https://github.com/mickelson/attract/blob/master/src/fe_romlist.cpp>
- token 解析与转义：<https://github.com/mickelson/attract/blob/master/src/fe_util.cpp>
- 布局文档（仓库内，官网已 404）：<https://github.com/mickelson/attract/blob/master/Layouts.md>

### 跨格式工具（用作交叉印证）
- Skyscraper 前端支持与「元数据保留」清单：<https://github.com/Gemba/skyscraper/blob/master/docs/FRONTENDS.md>
- Skyscraper 的 Pegasus 写出器（含冒号替换）：<https://github.com/Gemba/skyscraper/blob/master/src/pegasus.cpp>
- Skyscraper 的 ES-DE 写出器：<https://github.com/Gemba/skyscraper/blob/master/src/esde.cpp>

### Batocera / Recalbox
- Batocera 元数据字段：<https://github.com/batocera-linux/batocera-emulationstation/blob/master/es-app/src/MetaData.cpp>
- Batocera 系统自定义（wiki）：<https://wiki.batocera.org/emulationstation:customize_systems>
- Recalbox 权威仓库（GitLab 单体仓库）：<https://gitlab.com/recalbox/recalbox>
- Recalbox 元数据描述符：<https://gitlab.com/recalbox/recalbox/-/blob/master/projects/frontend/es-app/src/games/MetadataDescriptor.cpp>

### LaunchBox
- 官方插件 API 文档：<https://pluginapi.launchbox-app.com/>
- 官方游戏数据库导出：<http://gamesdb.launchbox-app.com/Metadata.zip>
- Pegasus 的 LaunchBox 导入器（真实 XML 元素名）：<https://github.com/mmatyas/pegasus-frontend/blob/master/src/backend/providers/launchbox/LaunchBoxGamelistXml.cpp>
- ⚠️ `https://docs.launchbox-app.com/` 调研期间全程 HTTP 503，Wayback 零快照

### Playnite
- `Game` 模型：<https://github.com/JosefNemec/Playnite/blob/master/source/PlayniteSDK/Models/Game.cs>
- `ReleaseDate` 结构：<https://github.com/JosefNemec/Playnite/blob/master/source/PlayniteSDK/Models/ReleaseDate.cs>
- 数据库存储：<https://github.com/JosefNemec/Playnite/blob/master/source/Playnite/Database/GameDatabase.cs>
- LiteDB 存储确证：<https://github.com/JosefNemec/Playnite/blob/master/source/Playnite/Database/Collections/ItemCollection.cs>
- 格式版本迁移史：<https://github.com/JosefNemec/Playnite/blob/master/source/Playnite/Database/GameDatabaseMigration.cs>
- 官方文档：<https://api.playnite.link/docs/>

### Steam / Steam ROM Manager
- Steam ROM Manager：<https://github.com/SteamGridDB/steam-rom-manager>
- AppID 算法：<https://github.com/SteamGridDB/steam-rom-manager/blob/master/src/lib/helpers/steam/generate-app-id.ts>
- 美术类型后缀：<https://github.com/SteamGridDB/steam-rom-manager/blob/master/src/lib/artwork-types/available-artwork-types.ts>
- parser 清单：<https://github.com/SteamGridDB/steam-rom-manager/blob/master/src/lib/parsers/available-parsers.ts>
- 二进制 VDF 编码：<https://github.com/tirish/steam-shortcut-editor>

### 其它
- Skraper 官网（闭源免费软件，无文档站）：<https://www.skraper.net/>
- Daijishō（已迁移、闭源）：<https://github.com/TapiocaFox/Daijishou>
- Daijishō 启动参数标签：<https://github.com/TapiocaFox/Daijishou/wiki/Start-Arguments>
- ArkOS（**2025-12-30 已归档**）：<https://github.com/christianhaitian/arkos>
- ArkOS 的前端 fork：<https://github.com/christianhaitian/EmulationStation-fcamod>
