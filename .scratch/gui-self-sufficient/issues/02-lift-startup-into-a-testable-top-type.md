# 02 — 把启动逻辑抬进一个可测的顶层类型

**What to build:** **行为一字不变的重构**，为 03 铺路。启动那条路——解析参数、定位要开哪份库、开出**现场**、进主窗口——现在摊在程序入口里，够不着测试。把它抬进一个持有两态（开场中 / 已开库）的顶层类型，入口只剩参数解析与开窗。

眼下那两态只用得上第二态，第一态是 03 要填的位子。这一张交付的是**接缝**：开场到主窗口那段过渡从此可测，而 03 就成了"给顶层类型加一个状态"这样一次容易的改动。

`App` 保持公开、拿的仍是一份已经开好的现场——**五屏一行都不改**。让 `App` 自己兜住"还没有库"，就得把现场变成可空的，那个可空会渗进五屏每一处判空（ADR-0023 否决过这条）。

**Blocked by:** None — can start immediately

**Status:** done

- [x] 启动那条路抬进顶层类型，程序入口只剩参数解析与开窗
      —— `crates/gui/src/program.rs` 新增 `Program`，持两态（开场中 / 已开库）。
      `Program::start(&Locate)` 一条龙走完「定位要开哪份库 → 开出现场 → 进主窗口」，
      工作目录也由它自己从三种给法里折（挂单 `Q378`）。`main()` 现在只剩五句：
      `Args::parse()`、`--font-check`、实测那几条、`args.start()`、`run_native`。
      **带保留**：`--demo` 那一支（造合成现场、换标题里那个库名）仍留在 `main.rs` 的
      `Args::start` 里。它在 `demo` feature 之后、默认不编进交付物，抬进 `program`
      会让假数据在库里长出一个公开入口——**记在挂单 `Q379`**，那一支往哪儿去由票 `03`
      或收尾裁。`/code-review` 的 Spec 轴把这一条判为「半过」，判词与这里一致。
- [x] 三种给法（主库根 / 库名 / 直接给中立库文件）行为与改动前完全一致
      —— `crates/gui/tests/program.rs::三种给法开出来的是同一份库`：三条路各开一次，
      `--library` 与 `--catalog` 指着同一个文件时**开出来的窗口标题逐字相同**
      （两条路折出两个名字的话，同一份库裁出来的路径锚会记在两个名字下），
      主库根那条开出的是它自己那一份。改动前后的等价性另有一道人工核对：
      `/code-review` 的 Spec 轴逐段对读了 `b1f1670:crates/gui/src/main.rs` 与现在这一份，
      结论是三种给法、`--demo`、`--bench*`、`--font-check` 全部等价。
      `Args::workspace_dir` 收窄成 `#[cfg(demo)]` 是安全的：`given()` 为真时它本来就是
      转调 `Locate::workspace_dir`，而非 demo 的 `default_dir` 那一支不可达。
- [x] 一个参数都不给时，照旧报"说清要开哪份库"那句话并退出——**这一张不改这个行为**
      —— `NO_LIBRARY` 那五行字**逐字搬进 `program.rs`，一个字符都没改**。
      `crates/gui/tests/program.rs::一个参数都不给就说清要开哪份库` 钉着它，
      断言用的是**字面量而不是引那个常量**：引常量的话两边同一个出处，谁把那句话改成
      别的，两边一起变而测试照样绿；这一条要钉的正是「人看见的还是那句话」。
      **另跑过一次二进制探针**（不采信测试）：`./target/debug/romcat-gui` 不带参数，
      屏上五行与改动前逐字相同、**退出码 1**。
- [x] 现有 GUI 测试一行不改、全部通过
      —— **一行都没改**：`git diff --stat b1f1670 -- crates/gui/tests/` 里只有新增的
      `program.rs` 一个文件，既有九个测试文件与 `shared/mod.rs` 全是空 diff。
      **实测条数**（`-- --list` 数出来的，不是估的）：`cargo test -p romcat-gui
      --all-features -- --list` = **199 条**（既有 **195** + 本票新增 **4**），
      `cargo test -p romcat-gui --all-features` **199 条全过、0 失败**。
      全仓门禁那一趟 `cargo test --workspace --all-features` = **1,726 条全过、0 失败**。
- [x] 顶层接缝上有新测试：给一份现成的库，走完启动拿到主窗口，窗口标题写着那一份
      —— `crates/gui/tests/program.rs::给一份现成的库就走完启动进主窗口`：临时目录里
      现建一份中立库 → `Program::start` → 断言窗口标题里写着**那一份的主库名**
      （`Site::library`，也就是中立库的主文件名），再跑一帧断言顶栏上五屏的名字都画出来了。
      同一个文件另有三条：三种给法、不给参数、库不在。四条**全从公开接口进去、断言屏上
      看得见的东西**，一处内部字段都没戳——照 `tests/close.rs` 与 `tests/roots.rs` 的形态。
      **主库一个字节都不碰**：中立库一律现建在临时目录里（ADR-0004）。
- [x] `App` 与五屏的公开接口一处不动
      —— `git diff b1f1670 -- crates/gui/src/app.rs` **为空**，五屏那五个模块一个都没进
      diff。`App` 照旧拿一份**已经开好的现场**：让它自己兜住「还没有库」就得把现场变成
      可空的，那个可空会渗进五屏每一处判空（ADR-0023 否决过这条）。
      顺带记一条：`App` 那份 `impl eframe::App` 从此没有调用方了（`run_native` 收的是
      `Program`），但删掉它就是动 `App` 的公开接口，本票明令不动——**挂单 `Q382`**。

## 收尾（本票自己写）

**门禁**：`cargo xtask gate` 五条全绿，墙钟 11m59s（这一趟与票 `01` 那棵树的门禁并行跑，
比 `main` 上单跑的 8m36s 长，属正常）。五条各自的结果：

| 条 | 结果 |
|---|---|
| `cargo fmt --all --check` | 绿 |
| `cargo check --workspace`（默认特性，交付出去的那份） | 绿 |
| `cargo clippy --workspace --all-targets --all-features` | 绿，零告警 |
| `cargo test --workspace --all-features` | 绿，**1,726 条全过、0 失败** |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins` | 绿 |

改到的文件五个：`crates/gui/src/program.rs`（新）、`crates/gui/tests/program.rs`（新）、
`crates/gui/src/main.rs`、`crates/gui/src/lib.rs`、`crates/gui/src/site.rs`（只改两处措辞，
见 `Q381`）。

## 挂单

号段 `Q376`–`Q385` 用掉 7 个（`Q383`–`Q385` 空号，不复用）。全文在 `.scratch/PARKING-LOT.md`：

| 号 | 一句话 | 谁来裁 |
|---|---|---|
| `Q376` | 顶层类型叫 `Program`，不叫 `Startup` | 票 `03` |
| `Q377` | 两态摆成私有枚举，开场那一支挂 `#[expect(dead_code)]` 留位 | 票 `03` |
| `Q378` | 接缝的入参只有 `Locate`，工作目录由它自己折 | 票 `03` |
| `Q379` | 合成数据那一支留在 `main.rs`，没跟着抬进顶层类型 | 拿主意的人 / 票 `03` |
| `Q380` | 全仓测试条数那两处（README 与 `gui/Cargo.toml`）没跟着改 | 收尾 |
| `Q381` | `Locate` 上那句「不给就是合成数据」是反话，顺手改了措辞 | 收尾 |
| `Q382` | `App` 那份 `eframe::App` 实现没人用了，但删它会动公开接口 | 票 `03` / 收尾 |

## 给票 `03` 的接缝说明

顶层类型是 `romcat_gui::program::Program`；两态是一个**私有**枚举
`Stage::Opening`（开场中，这一趟只留位、不构造）与 `Stage::Opened(Box<App>)`（已开库）。

**接缝就是 `Program::start` 里的这一句**：

```rust
if !locate.given() {
    return Err(NO_LIBRARY.to_string());   // ← 票 03 把这一句换成交出 `Stage::Opening`
}
```

- `Opening` 那一支上挂着 `#[expect(dead_code, reason = …)]`——**一构造它，那条期待就落空，
  编译器当场提醒把那三行属性删掉**。
- `Program::window_title()` 在开场那一态返回 `"romcat"`（还没有库、也还没有屏），
  `Program::ui()` 在那一态什么都不画：两处都是留给 `03` 的位子。
- 开场交出一份现场之后换主窗口，走的是 `Program::opened(App::new(site, 工作目录))`
  ——那个构造器已经在了（合成数据那一路正用着它）。
- `Opened` 必须装箱：不装箱 `clippy::large_enum_variant` 当场红（`all = deny`）。
