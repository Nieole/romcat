# 规格图（`docs/spec/`）

这张图是这个项目的**事实来源**：判断、边界、不变量写在这里，代码只确认实现。根在仓库顶上的两份，其余在 `docs/spec/`。

## 从哪开始

| 你要弄清什么 | 读哪份 |
|---|---|
| 这个产品是什么、给谁用、不做什么 | `goal-and-requirements.md`（`goal`） |
| 包与包怎么分、数据住在哪、跨域不变量 | `architecture.md`（`architecture`） |
| 核心库某一域 | `docs/spec/core.md` 那张十八个领域的表，再进那一份 |
| 命令行 / 界面 / 门禁 | `cli.md` / `gui.md` / `xtask.md` |
| 某个领域概念该用哪个词 | `CONTEXT.md`（`glossary` 节点；门禁的词表也从这里读） |
| 为什么当初这么定 | `docs/adr/`（24 份，各是一个 `adr-NNNN` 节点） |

## 文件在哪

| 层 | 文件 |
|---|---|
| 根 | `goal-and-requirements.md`、`architecture.md` |
| 包 | `docs/spec/core.md`、`cli.md`、`gui.md`、`xtask.md` |
| core 的十八个领域 | `docs/spec/core-<域>.md`（路径与只读接缝、中立库、平台与文件名规则、扫描、内容分类与穿透、成型、识别、本地数据源、待确认、沉淀库、刮削、标题、适配器、子库、同步、能力档案与转格式、库体检、运行支架） |
| 采纳的既有文档 | `docs/adr/*`、`CONTEXT.md`、`docs/library-facts.md`、`docs/platforms.md`、`docs/scale-reference.md`、`docs/research/*` |

## 图是怎么连的

- `parent` 照着代码结构走：`goal` → `architecture` → `core` → 各领域；`cli` / `gui` / `xtask` 挂在 `architecture` 下。
- `depends-on` 只写代码里真实成立的边。**领域之间的边写在 `core.md` 里**，不在架构里重复，也不在每份领域规格里各抄一遍。
- 采纳的既有文档就地加 frontmatter、正文不动；一份文档在图上是一个节点，不是一个副本。

## 改动规矩

- **判断、边界、不变量一变，同一笔改动里把 owning 规格改掉。** 规格漂了就不再是事实来源。
- 新开一域：在 `core.md` 的表里加一行、建 `docs/spec/core-<域>.md`、`parent: core`。
- 状态只有四个词有含义：`draft`（还没人复核）、`active`（生效中）、`stale`（与代码漂了）、`deprecated`（不再成立）。**复核过的规格翻 `active`，不翻 `done`**——它一直在用。
- 这份 README 不是节点（没有 frontmatter），它只负责带路；判断一个都不许长在这儿。
