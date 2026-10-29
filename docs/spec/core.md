---
id: core
type: module-design
title: romcat-core —— 领域逻辑的全部
status: active
parent: architecture
depends-on: [path-fs, catalog, platform-filename, scan, classify-container, shape, identify, sources, triage, verdict, scrape, title, adapter, sublibrary, sync, capability-convert, report, runtime]
---

# romcat-core

**职责**：这个工具全部的领域判断都在这里。扫描、成型、识别、待确认、刮削、标题、导出、子库、同步、体检——命令行与界面只调用、只画，不许各算一遍（`adr-0005`、`adr-0024`）。

**边界**：`core` 不依赖任何同级包，也不认识界面的存在。它对外只有模块自己的 `pub` 面，`lib.rs` 不做整包再导出——用哪个域就 `use` 哪个域。

## 十八个领域

| 领域 | 规格 | 一句话 |
|---|---|---|
| 路径与只读接缝 | `path-fs` | 跨平台路径、中立库的键、`LibraryFs` 的只读面 |
| 中立库 | `catalog` | 工具自己的事实来源，每个主库一份，可再生 |
| 平台与文件名规则 | `platform-filename` | 平台清单与剥离规则两份声明式配置 |
| 扫描 | `scan` | 只读走完主库，落中立库，留遍历与断点 |
| 内容分类与穿透 | `classify-container` | 三类主线与容器内部构成 |
| 成型 | `shape` | 按平台规则把文件聚成变体 |
| 识别 | `identify` | 分层匹配、置信度与依据 |
| 本地数据源 | `sources` | DAT 库、中文离线源、TitleID 库与 URL 闸门 |
| 待确认 | `triage` | 队列、分批、细分、批量与合并作品 |
| 沉淀库 | `verdict` | 裁决、收藏、合集与人工纠正 |
| 刮削 | `scrape` | 离线与在线两档、媒体池、输入指纹 |
| 标题 | `title` | 标题集合、显示标题、排序标题 |
| 适配器 | `adapter` | 前端格式的读写、能力档位与结构性损失 |
| 子库 | `sublibrary` | 选择集 = 规则 + 例外，目标与容量 |
| 同步 | `sync` | 三方对比、差量预览、清单边界、执行 |
| 能力档案与转格式 | `capability-convert` | 设备与存储吃得下什么，以及照它转 |
| 库体检 | `report` | 报告、重复拷贝明细、处置建议 |
| 运行支架 | `runtime` | 工作目录、现场、任务台、工序进度 |

另有两个不是领域、但要有归处的东西：`testing`（测试支持，普通模块而不是 `cfg(test)`，集成测试与后续票的测试都在用，因此随 release 一起编）与 `lib.rs` 那段叙述（几句边界话加模块清单，指向 `gui` 那份规格，不在代码里再讲一遍）。

## 依赖边

`catalog` 是持久化枢纽，**几乎每个领域与它互指**：领域定义类型与判断，中立库负责存（`catalog → shape::Variant → catalog`、`catalog → scrape::AnchorKind → catalog` 这样的两环是常态，不是设计失误）。因此不要问「谁拥有 schema」，问某张表归谁——归它的领域规格。

除枢纽外，真实成立的边是这些：

- **管线**：`scan → shape → identify → triage/verdict → scrape → title → sublibrary → sync/capability-convert → adapter`。这条顺序在数据里成立，不只是观感。
- **低层汇点**：`path-fs` 谁都能用、谁也不依赖它之外的判断；`classify-container` 被成型、识别、转格式、适配器共用；`runtime` 被长活那一侧共用。
- **识别依赖数据源**：`identify → sources`（DAT、中文源、TitleID），反过来 `sources` 不认识识别结论。
- **裁决回头看两侧**：`triage → identify`、`triage → verdict`；`verdict → scrape → sync` 是「人定的东西要能送到卡上」这条路。
- **同步是唯一往别人的盘上写的地方**：`sync → sublibrary → capability-convert → scan`；`convert` 只读主库、只产生新文件（`adr-0004`）。

## core 这一层的规矩

- **不写主库**：任何走到磁盘的写操作都要经过 `sync::execute` 或导出那一条路，且只写元数据与媒体（`core-path-fs`、`core-sync`）。
- **两套迁移制度并存**：`catalog::SCHEMA_VERSION`（对不上就删库重扫）与各不可再生库自己的 `MIGRATIONS`（顺序迁移、永不删库）。
- **每个判断一处**：见到同一件事在第二个地方被算了一遍，那是缺陷，不是冗余保险（`adr-0024`）。
- **每个公开条目的文档都要说清「为什么」**：`missing_docs` 是 warn，而 `clippy` 零告警在门禁里——模块文档是这个仓库事实上的设计记录，规格不重复它、只承接跨模块的那一层。
