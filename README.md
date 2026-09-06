# romcat

把一批以 **Pegasus** 方式维护的模拟器资源，从手工维护变成**自动识别、自动刮削、可在主流前端格式间互转**的库。

> **主库只读。** 那是不可再生的资源。工具只写元数据文件、媒体目录与子库，**ROM 文件一个字节都不改**（ADR-0004）。
>
> **主库是一组根。** 几块盘、几个目录都能加进同一个主库，扫完收进**同一份中立库**统一管理。

---

## 它解决什么

一个 10 TB 量级的 ROM 库，46,444 个变体、215,964 个文件，绝大多数是**汉化版**——文件名五花八门，DAT 里根本没有，光看名字认不出是哪个游戏。手工维护元数据到某个规模就维护不动了。

romcat 做四件事：

1. **认出来** —— 零解压算 CRC-32 撞 DAT，读卡带内部头、光盘序列号、Switch 的 TitleID，撞不上再用文件名规则与中文离线数据源模糊匹配，最后才轮到模型推断兜底。**每一条结论都带置信度与依据**，拿不准的进**待确认队列**由人裁决。
2. **补齐** —— 在识别结论上刮削标题、简介、年份、开发商与封面。**离线档一个网络请求都不发。**
3. **导出** —— 转成 Pegasus 或 ES-DE 的元数据。每个适配器标着**能力档位**：导出前就知道这个格式会丢掉什么。
4. **分发** —— 按**规则**挑一批出来做**子库**（一台掌机一个），算差量、转格式、同步过去。

## 它不做什么

- **不改主库。** 不写、不删、不重命名、不 touch 时间戳。
- **不下载 ROM。** 不做任何资源分发。
- **不内置密钥。** Switch 的识别只走免密钥的路子。
- **不替你删重复文件。** 只报告，删不删是你的事。

---

## 快速上手

```bash
cargo build --release          # 产出 target/release/romcat 与 romcat-gui
```

**工作目录**按这条链定：`$ROMCAT_HOME` → `%APPDATA%\romcat` → `$XDG_DATA_HOME/romcat` → `~/.local/share/romcat`，也可以每条命令 `--workspace` 单指。中立库、沉淀库、DAT 库、媒体池、界面的版式偏好都住在这儿——**一样都不落在主库里**（落进去会被当场拦下）。

```bash
# 1. 扫一遍主库，给它起个名字（只读，只算哈希与体积）
romcat scan /path/to/Game --library 主库 --root-name 主库

# 1b. 第二块盘加进同一个主库：换个根名，扫进同一份中立库
romcat scan /path/to/Pegasus --library 主库 --root-name 元数据库

# 1c. 盘换了位置：拿原来那个根名再扫一趟，新位置就记进库里了
romcat scan /新位置 --library 主库 --root-name 主库

# 2. 取 DAT 与中文离线索引（各几百 MB，取一次）
romcat dat sync
romcat zh sync

# 3. 认
romcat identify --library 主库

# 4. 补元数据（默认离线档，不发网络请求）
romcat scrape --library 主库
romcat titles --library 主库

# 5. 看看认不出来的那些，逐条或批量裁决
romcat triage list --library 主库

# 6. 导出
romcat export --library 主库 --format pegasus --out /path/to/out
```

改了成型规则不必重扫主库，`romcat shape` 就地重来；`romcat report` 更是一个字节都不读主库——**外置盘不在位时照样出报告**。

### 界面

```bash
romcat-gui --library 主库                 # 按名字开
romcat-gui /path/to/Game                  # 给主库根，只用来找中立库
romcat-gui --catalog ~/.romcat/catalog/主库-xxxx.sqlite3
```

**不说开哪份库，它会直说「说清要开哪份库」然后退出**，不会擅自造一份合成数据糊弄你。窗口标题里写着开的是哪一份。

### 命令一览

| | |
|---|---|
| `scan` / `shape` / `report` | 扫描、成型、出报告 |
| `identify` / `triage` | 识别与**待确认队列** |
| `dat` / `zh` / `switch` | 三个数据源的本地镜像 |
| `scrape` / `titles` / `names` | 刮削、标题集合、文件名剥离规则 |
| `import` / `export` / `adapters` | 前端格式互转与**能力档位** |
| `sublibrary` / `capability` | 子库、选择集、目标设备的能力档案 |
| `platforms` | 平台清单与成型规则 |

每条都有 `--help`。

---

## 现在到哪一步了

**59/63 张票落地**（四条队列合计）。余下四张：三张是 Windows 真机验证（`ready-for-human`，需要人在 Windows 上跑），一张还没接（`ready-for-agent`）。**1,581 条测试全绿。**

真库实测（`docs/library-facts.md` 记着全部数字与日期）：

| | |
|---|---|
| 变体 | **46,444 个**，吃掉 215,964 个文件、5.33 TiB |
| 识别命中率 | **83.7%**（不含模糊匹配；含模糊匹配的候选另计） |
| 前端条目 | 46,444 个变体作品级收敛成 **28,529 条** |
| 无判据 | 2,631 个（拿不到可撞的东西，**不进命中率的分母**） |

第二批工作已出规格与工单：**中文离线源取全字段**（`.scratch/offline-chinese-fields/`）——本机那份中文离线数据里 94.2% 的条目带中文简介、99.7% 带类型、83.8% 带开发商，眼下**一条都没被取出来过**。

---

## 项目怎么组织的

```
crates/core/   romcat-core   领域逻辑全在这儿：扫描、成型、识别、刮削、适配器、子库
crates/cli/    romcat        命令行
crates/gui/    romcat-gui    窗口壳。只画和转发，一条领域逻辑都不许长在这里
docs/adr/      22 份架构决策记录
docs/          library-facts.md（真库实测台账）、platforms.md、research/
CONTEXT.md     词表。动手前先读它，输出用它的词
.scratch/      规格与工单
```

**核心库不依赖界面，界面不含领域逻辑。** 命令行与界面共用同一套结论——命令行裁的界面看得见，反过来也一样。

三份数据分得很清：

- **中立库**（每个主库一份，一个主库可以有好几个**根**）—— **整份可再生**。结构版本一变就让你删库重扫，那是省下一整套迁移代码的便宜买卖。
- **沉淀库**（全局一份）—— **不可再生**。你一条条看出来的裁决住在这儿，删掉就没了，所以它走顺序迁移，永不要求删库。键是**内容锚**，两块盘接同一台机器裁决一次两边都受益。
- **DAT 库 / 中文索引 / 媒体池** —— 本地镜像，取一次用很久。

### 该读的文档

| 你要做什么 | 先读 |
|---|---|
| 动手改代码 | `CONTEXT.md`（词表）+ 相关的 ADR |
| 搞清某个数字哪来的 | `docs/library-facts.md` |
| 加一个平台 | `docs/platforms.md` |
| 出票、接票 | `docs/agents/issue-tracker.md` |

---

## 开发

```bash
cargo fmt --all --check                                   # 排版归机器管
cargo clippy --workspace --all-targets --all-features     # 零警告
cargo test --workspace --all-features                     # 1,581 条
cargo doc --workspace --no-deps
```

### 排版归机器管

**提交前跑 `cargo fmt --all`。** 门禁第一条就是 `cargo fmt --all --check`，红了就是没跑。

配置在仓库根下的 `rustfmt.toml`，只有三项（`edition` / `style_edition` / `max_width`），
每一项为什么这么定、以及**量过但故意不设**的那十来个旋钮各自的实测代价，都写在文件里。
**别凭手感调它**——那份矩阵是整仓库跑出来的：试过的每一处偏离都让 diff 变大。

两件反直觉的，先说在这儿免得你重量一遍：

- `rustfmt` 按**显示列宽**算，不按字符数算，所以 `max_width = 100` 是一百列，一行 50 个汉字就到顶。
- **中文注释不会被重新折行**（`wrap_comments` 在稳定版设不了、默认 `false`），注释里的对齐表格是安全的。
  会被动的只有代码行。

换行一律 LF，由 `.gitattributes` 钉住——`rustfmt` 自己拦不住 CRLF 混进来。

### ⚠️ 门禁必须带 `--all-features`

`romcat-gui` 有一个默认关掉的 **`demo` feature**：合成数据、`--demo`、以及 `--bench*` 那几条实测开关全在它后面。**关掉之后它们一个字节都不进交付出去的二进制**——假数据与真库在界面上长得一模一样，看见一屏假名字的第一反应会是「我的库怎么了」，那比起不来更坏。

代价是：**不带 `--all-features` 跑测试，会少跑 101 条**（1,480 而不是 1,581）——其中 7 个测试文件（`browse` / `close` / `layout` / `media` / `queue` / `table` / `task`）整份都不编译，剩下的散在别的文件里被 `cfg` 掐掉。**少跑不报错**，退出码照样是 0，所以命令得记牢。

```bash
# 跑实测（那几条都不开窗，没显示器也跑得起来）
cargo run -p romcat-gui --features demo -- --bench 240
cargo run -p romcat-gui --features demo -- --bench-queue
cargo run -p romcat-gui --features demo -- --demo          # 拿合成数据开个窗看看
```

### 几条纪律

- **主库只读。** 测试与实测**一律用本地 fixture 目录**模拟目标设备，绝不去动任何真实设备或 SD 卡。
- **路径键一律 NFC 归一**，从磁盘读时用原始形态（ADR-0020）。这条被违反过四次。键的第一段是**根名**，拆键走 `path::split_root`。
- **绝不直连 `datomatic.no-intro.org`**——一次畸形请求已在调研中导致永久 IP 封禁。Redump 走 `redump.info`。
- **在线源赌的是你的账号与 IP**（ADR-0007）。默认限流，未识别的变体一个请求都不发。
- `#![forbid(unsafe_code)]`，Rust 1.95，edition 2024。

## 许可

界面内嵌的中文字体是 Noto Sans SC 子集，OFL 许可，随附 `crates/gui/assets/OFL.txt`。
