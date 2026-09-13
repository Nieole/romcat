# 06 — 删掉那份没人用的窗口实现

**What to build:** 两份**逐字相同**的窗口实现去掉一份——下一个人改那一带时不会改错那一份。

收挂单 `Q382`（并 `Q390`）：`impl eframe::App for App` 从此没人用，但删它会动 `App` 的公开接口。
—— **已收（2026-09-14）**：`crates/gui/src/app.rs` 末尾那份 `impl eframe::App for App` 删掉（6 行，含前面一个空行），别的一个字节没动。正文在 `.scratch/gui-self-sufficient/issues/02-lift-startup-into-a-testable-top-type.md` 的 `### Q382` 与 `03-opening-lists-and-opens-a-catalog.md` 的 `### Q390`，各在末尾补了一行「已落地」，原有字句没改（补不补这一行，同一批里两种做法都有，见挂单 `Q671`）。本票自己的岔路口：`Q671`。

**Blocked by:** 无 —— 可立即开工

**Status:** done

⚠️ **当初挡着它的两张票都已 `done`。** 那两张各自的硬约束写着「`App` 与五屏的公开接口一处
不动」，而去掉一个 trait 实现正是动公开接口——**挡的人没了**，所以这一件现在做得成。
`Q390` 记的正是「两张票的票面在这一处互相顶着」。

⚠️ 留着它不报任何告警（trait 实现不算死代码），所以**没有机器会替你发现它还在**——
这一件靠人做掉。

- [x] 那份没有调用方的实现删掉了
      —— 删的是 `crates/gui/src/app.rs` 末尾那份（`93835a0` 上第 657 行）。动手前核过它零调用方：全仓（排除 `target`）`grep -rn 'eframe' --include='*.rs'` 只剩 `main.rs` 的 `run_native`、`program.rs` 那一份与几句注释，没有 `dyn eframe::App`、泛型约束或 kittest 的 eframe harness；`bench.rs` 与 `crates/gui/tests/*` 调的都是固有的一参 `App::ui(&mut self, ui)`，从没走过这个 trait。删完没有文档断链：`[`App::ui`]` 那几处（`look.rs:83`、`bench.rs:9/206/477/761`）指的是固有方法，它还在；`lib.rs:16`、`program.rs:63` 本来就说 `eframe` 收的是 `Program`；`app.rs:498`「`eframe` 与量帧率的那条路走的是同一个函数」照旧成立（`Program::ui` → `App::ui`）。门禁 doc 步（`-D warnings`）的结果见第 3 条。
- [x] 窗口那条路照旧（另一份实现本来就是真正在跑的那份）
      —— `crates/gui/src/main.rs:303` `eframe::run_native` 交出去的是 `Box::new(program)`，`program` 来自 `Args::start`：默认那一支是 `Program::start`，`demo` feature 那一支是 `Program::opened(app, …)`，两支都是 `Program`。`crates/gui/src/program.rs:243` `impl eframe::App for Program` 一个字节没动，照旧调 `Program::ui`，已开库那一态再调 `App::ui`。本票 diff 只有 `app.rs` 那 6 行删除。
- [x] 全仓门禁绿，界面那几份测试一条没改也全绿
      —— 全量门禁（2026-09-14，在本票未提交的改动上跑，基点 `93835a0`）：`TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp cargo xtask gate -j 3 --test-threads 3 --keep-going`，日志 `/Users/nicoer/dev/game-wt/logs/slot-2-nm06-gate.log`（开头两行是 `/Users/nicoer/dev/game-wt/slot-2` 与本分支）。fmt、glossary、check、clippy、test、doc 六条全绿，`EXIT=0`；test 那一步 71 个测试目标全部 ok，1929 条通过、0 条失败、2 条 ignored（`magnitude.rs` 那两条量级测量）。doc 步是 `-D warnings`，删完没有断链。界面这一侧：`crates/gui/tests/` 一个文件没改（本票 diff 里没有它），12 份集成测试全绿——browse 48、close 1、font 13、font_check 2、layout 14、media 9、program 32、queue 45、roots 42、sublibrary 31、table 4、task 8，另有 `romcat_gui` lib 单元测试 43 条。日志里那几行 `error: unresolved link to NoSuchItem` 出自 `xtask/tests/gate.rs` 故意造的一份坏文档，那条测试本身是 ok。
