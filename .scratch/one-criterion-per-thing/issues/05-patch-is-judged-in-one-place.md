# 05 — 「这是不是补丁」提成核心库一处，两种依据都喂给它

**What to build:** Switch 的**补丁**与**附属内容**不再被导出成前端条目——包括那些文件名里
一个「补丁」字都没有的（`xxx[v1.0.2].nsp`）。词表与 ADR-0013 逐字说它们不该成为前端条目，
现在真的拦得住了。

收挂账 `D142`：Switch 的补丁与附属内容照样做成前端条目。**已收**，证据见下面六条验收。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **实情比挂账写的重，动手前先读清楚**：眼下**有两个独立的补丁判断**——一个按**扩展名与
名字里的词**（`ips`/`ups`/`bps`… 与「补丁」/`patch`），一个按 **TitleID 低 12 位**（`0x800`）。
**只有前者到得了导出那道闸**（它 parse 的是识别结论里那句理由的中文前缀），
后者的唯一消费者是报告里一个分组计数。**于是一份 Switch 补丁永远走不到那道闸。**

⚠️ **按 ADR-0024 推论 2：输入由领域定。** 「是不是补丁」要的是**这份内容有哪些依据**——
文件名与 TitleID **都是依据，不是两个判断**。**依据有优先级：TitleID 比文件名准，有它就用它。**

⚠️ 导出那道闸改成读这个判断的结果，**不再 parse 理由字符串**。

- [x] 一份文件名不含「补丁」字样的 Switch 补丁（TitleID 低 12 位是 `0x800`）**不成为前端条目**
  - 证据：`crates/core/tests/pegasus.rs` 的 `switch_的补丁与附属内容名字里一个字都没说也不导出为前端条目`、`crates/core/tests/gamelist.rs` 的 `补丁与附属内容不导出为前端条目_名字说的与_titleid_说的都算`（`伊蘇X[v1.0.2].nsp`，基点上红）；识别侧 `crates/core/tests/switch.rs` 的 `更新包落下的结论是补丁_追加内容是附属内容_本体与卡带都不是`。
  - 审查抓出的两种情形也钉住了：人裁过之后重跑识别（`人裁过的更新包重跑一趟识别照旧是补丁`），删库重扫后钉在路径上的裁决先短路（`裁决先于第一趟识别就钉在路径上_更新包照样判得出是补丁`），改前都红，挂单 `Q704`。
  - 保留：票据说不清的更新包（没有票据、一个容器装着不止一档）照旧导出，挂单 `Q703`。
- [x] 附属内容同样不成为前端条目
  - 证据：上面两条适配器测试里的 `伊蘇X 追加曲包.nsp`（TitleID 尾 `9001`，基点上红）。
  - 保留：PSV 的附属内容照旧认成员身份，两条依据住在成型与识别两层，挂单 `Q705`。
- [x] 靠文件名判出来的那些**照旧拦得住**（行为不变）
  - 证据：`crates/core/tests/pegasus.rs` 的 `非游戏资产与补丁不导出为前端条目` 与 `crates/core/src/identify/scope.rs` 的单元测试一字未改，照旧绿；gamelist 那条另摆了一份按名字判的汉化补丁。
  - 旧库：`没有那一列的旧库_按名字判出来的补丁照旧不导出为前端条目`（补列那一刻从理由开头搬一次旧结论，改前红），挂单 `Q701`。
- [x] 导出那道闸不再 parse 那句理由字符串
  - 证据：`crates/core/src/adapter/converge.rs` 的 `excluded` 改读 `Catalog::not_standalone()`（`identification.standalone` 那一列）；`scope::Skip::kind_in` 删掉，全仓 grep `kind_in` 零处。
  - 保留：打开旧库补列那一刻的一次性搬迁读的是理由开头（`crates/core/src/catalog/identify.rs` 的 `OLD_PATCH_REASON`），不在导出那道闸上；这个搬法请拿主意的人点头，挂单 `Q701`。
- [x] 全仓再没有第二处判「是不是补丁」的代码
  - 证据：判断只在 `crates/core/src/identify/scope.rs` 的 `standalone`（TitleID 优先、名字其次）；票据怎么读成本体 / 补丁 / 附属内容只在同文件的 `title_kind`。`scope::decide`、识别的 `standalone_of`、报告落库的 `content_switch.kind`（`switch::from_rows`）都从这两处取。grep：`PATCH_WORDS` / `PATCH_EXTENSIONS` / `patch_by_name` 只在 `scope.rs`；`title_kind` 一处定义、两处调用。
  - 保留：`crates/core/src/identify/switch.rs` 的 `named` / `describe_shape` / `cross_check` 与命令行 `crates/cli/src/main.rs` 仍拿 `Kind::of` 把 TitleID 尾巴印成「补丁 / 附属内容」字样。那是依据文本与候选名里的描述，不决定导出，本票没收。
- [x] 两份适配器的导出各留一条测试
  - 证据：Pegasus 与 gamelist 各一条（见第一条）；Pegasus 另有 `本体加更新的卡带只带着更新的票据_照样是前端条目`，钉「票据不一定替整份容器说话」那一头。

门禁：`/Users/nicoer/dev/game-wt/logs/slot-4-oc05-gate.log`，6 条全绿（test 71 个目标 1943 条通过、0 失败），`EXIT=0`。结构版本不升，ROM 与主库一个字节不动。
