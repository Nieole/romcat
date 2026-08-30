# 街机沿用统一模型，MAME XML 作为权威元数据源

MAME 撞破了「变体 = 磁盘上一份实际可玩的东西」这条定义：clone set 单独存在不可运行（`sf2ce.zip` 必须有 `sf2.zip`），而 BIOS set 与 device set（`neogeo.zip`、`qsound`）根本不是游戏，却必须躺在磁盘上。但 MAME 自己把关系表达得很清楚——`mame.xml` 里 `cloneof`、`isbios`、`isdevice`、`runnable` 都是显式属性。

映射：一个可运行的 game set = 一个**发行版**（不是变体）；`cloneof` 关系映射为同一**作品**下的多个发行版，这正是 MAME parent/clone 的原本语义；CHD 子目录是附属文件；BIOS 与 device 标记为**非游戏资产**，入库但永不导出。

元数据不走通用刮削链路：`mame.xml` 自带 `description`、`year`、`manufacturer`，且是调研认定的唯一法律完全干净的源（明确献给公有领域）。

## Consequences

- 库里需要「非游戏资产」这个状态——磁盘上必须存在、库要知道它存在、但它永远不进导出。
- 街机与用户的核心痛点（汉化版）完全无关，街机基本不存在汉化版。它虽然模型特殊，优先级却应该很低，交由库体检的实际数据决定。
