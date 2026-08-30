# egui/eframe 选型三风险核实

**查证日期**：2026-08-31
**核实时的版本**：`egui` / `eframe` / `egui_extras` / `egui-winit` **0.36.1**（发布于 2026-08-07，见 <https://github.com/emilk/egui/releases>）；上游 `winit` **0.30.13**（egui workspace `Cargo.toml` 第 162 行锁定 `winit = { version = "0.30.13" }`，<https://github.com/emilk/egui/blob/master/Cargo.toml>）；`egui_table` **0.10.0**（2026-08-05）。

**方法**：只用一手来源——emilk/egui 与 rust-windowing/winit 的源码、CHANGELOG、issue/PR tracker，docs.rs 官方文档，crates.io API，以及本机（macOS 15 / Darwin 24.6.0）上对字体文件的直接测量。所有 issue 状态均为 2026-08-31 通过 GitHub API 实时查询所得。

> 标注约定：**【事实】** = 来源里白纸黑字写着或可从源码/文件直接验证；**【推断】** = 我从事实推出来的、来源没有明说的结论；**【实测】** = 我在本机上直接跑出来的数据。

---

## 结论速览

| 风险 | 判定 | 一句话 |
|---|---|---|
| 一、中文输入法（IME） | 🟡 **今天可用，但有必须先验证的平台盲区** | 2026 年 2–4 月上游做了一轮系统性 IME 重写（0.35.0 起），维护者在 macOS/Windows 上用**搜狗、微信输入法**实测通过；已知遗留：Linux X11+Fcitx5 预编辑不在框内、macOS 字符检视器插不进 `★ ♪ Ⅲ`。 |
| 二、中文字体 | 🟢 **确认是坑，但完全可控** | 内置字体**一个汉字都没有**（我逐码位验证过）；打包一份子集字体 2.5–7.4 MB 即可解决，不构成选型障碍。 |
| 三、十万行表格 | 🟢 **不是问题** | 官方 demo 的行数滑杆上限就是 100 000；`TableBody::rows` 的开销与总行数无关。真正的成本是**排序、多选、列宽以外的功能全要自己写**。 |

**总判定**：三个风险里没有一个足以否决 egui。真正需要在写第一行 GUI 代码之前花半天做掉的，是**风险一的实机验证**（见文末「规避方案」第 0 步）。

---

## 风险一：中日韩输入法（IME）支持现状

### 1.1 结论先行

**【事实】** 从 egui 0.34（2026-03-26）到 0.35（2026-06-25），贡献者 [@umajho](https://github.com/umajho) 对 IME 通路做了一轮系统性重写，一次性关掉了 7 个 CJK 输入相关的 issue。0.36.1 是包含全部这些修复的版本。

**【事实】** 这不是"跑通了 hello world"级别的验证。核心 PR [#7967 "Much improved IME"](https://github.com/emilk/egui/pull/7967)（合并于 2026-03-24）的正文里列出了作者的实测矩阵，逐字摘录：

| 平台 | 实测的输入法 |
|---|---|
| macOS 15.7.3 (AArch64) | 内置中文输入法（双拼-简体）、内置日文（罗马字）、内置韩文（2-Set） |
| Windows 11 25H2 (AArch64 VM) | 内置中文（双拼）、**搜狗输入法（中文双拼）**、**微信输入法 WeType（中文双拼）**、内置日文（平假名）、内置韩文（2 Beolsik） |
| Linux [Wayland + IBus] (Fedora KDE 43) | 中文智能拼音（双拼）、日文 Anthy、韩文 Hangul |
| Linux [X11 + Fcitx5] (Debian 13 + Cinnamon) | 同上 |

> **【事实】**PR 正文中的一段有趣旁证：在 Fedora KDE + Wayland + IBus 环境下，作者记录 "The Chinese Intelligent Pinyin IME is broken in native Apps like System Settings and KWrite, but works correctly in egui!"（中文智能拼音在 System Settings、KWrite 这些原生应用里是坏的，但在 egui 里工作正常）。

**【推断】** 搜狗和微信输入法被明确实测过，这对一个面向中国用户的桌面工具是最有价值的单条证据——它们合计覆盖了中国大陆桌面输入法的绝大部分市场。

### 1.2 IME 时间线（哪些坑已经填了）

**【事实】** 摘自 <https://github.com/emilk/egui/blob/master/CHANGELOG.md> 与 `crates/egui-winit/CHANGELOG.md`：

| 版本 | 日期 | 变更 |
|---|---|---|
| 0.34.0 | 2026-03-26 | Improve IME support with new `Event::Ime` [#4358](https://github.com/emilk/egui/pull/4358) |
| 0.35.0 | 2026-06-25 | **Much improved IME [#7967](https://github.com/emilk/egui/pull/7967)** |
| 0.35.0 | 2026-06-25 | Improve IME handling, add public method `owns_ime_events` on `Memory` [#7983](https://github.com/emilk/egui/pull/7983) |
| 0.35.0 | 2026-06-25 | **Implement proper visuals for IME composition [#8083](https://github.com/emilk/egui/pull/8083)** |
| 0.35.0 | 2026-06-25 | Fix backspacing leaving last character in IME prediction not removed on macOS native and Safari [#7810](https://github.com/emilk/egui/pull/7810) |
| 0.36.0 | 2026-08-05 | 移动端/web IME 改进 [#8045](https://github.com/emilk/egui/pull/8045)（对桌面无关） |

**【事实】** 被这一轮工作**关闭**的 CJK 输入 issue（全部已 closed，状态 2026-08-31 查询）：

| # | 标题 | 关闭日期 |
|---|---|---|
| [#7809](https://github.com/emilk/egui/issues/7809) | 日文 IME 输入完成后 `TextEdit` 丢焦点 | 2026-03-24 |
| [#7876](https://github.com/emilk/egui/issues/7876) | **CJK IME 用 Enter 上屏，会在 multiline 里多插一个换行** | 2026-03-24 |
| [#7908](https://github.com/emilk/egui/issues/7908) | **CJK IME 组字时按退格，会误删输入框里已有的字** | 2026-03-24 |
| [#7906](https://github.com/emilk/egui/issues/7906) | 光标位置不为 0 时无法使用 IME | 2026-02-14 |
| [#7485](https://github.com/emilk/egui/issues/7485) | IBus + Wayland 下 singleline/multiline 输入行为错误 | 2026-04-06 |
| [#8087](https://github.com/emilk/egui/issues/8087) | 字素簇（grapheme cluster）破坏 IME 组字与选中 | 2026-04-15 |
| [#8109](https://github.com/emilk/egui/issues/8109) | Debian 13 + Fcitx5 只能输入头几个中文字 | 2026-04-19 |

> 注：#7876（多插换行）和 #7908（退格误删）正是中文输入最高频的两个致命体验问题，在 0.35.0 之前是真实存在的。**用 0.34 及以下版本评估 egui 的中文输入，会得到严重偏悲观的结论。**

### 1.3 候选词窗口定位：源码级确认

**【事实】** `egui-winit` 确实调用了 winit 的三个 IME API（<https://github.com/emilk/egui/blob/master/crates/egui-winit/src/lib.rs>）：

- 第 1153 行 `window.set_ime_allowed(allow_ime)` — 有焦点的文本框存在时才开启
- 第 1167 行 `window.set_ime_purpose(...)` — 传递密码框等语义
- 第 1176–1183 行 `window.set_ime_cursor_area(...)` — **把光标矩形（`ime.rect` × `pixels_per_point`）报给系统，候选词窗就是靠它定位的**；且做了脏值判断（`if self.ime_rect_px != Some(ime_rect_px)`），不会每帧无谓调用

**【事实】** winit 的这个 API 是**为 CJK 候选词窗专门设计**的。`set_ime_cursor_area` 的 doc 原文（<https://github.com/rust-windowing/winit/blob/v0.30.13/src/window.rs#L1219-L1225>）：

> "The windowing system could place a candidate box close to that area, but try to not obscure the specified area, so the user input to it stays visible.
>
> The candidate box is the window / popup / overlay that allows you to select the desired characters. … **(Apple's official term is "candidate window", see their [chinese] and [japanese] guides).**"

—— doc 里直接链到了苹果的中文/日文输入法官方指南。

**【事实】** 平台注记逐字摘自同文件 L1240–1243：

```
/// ## Platform-specific
///
/// - **X11:** - area is not supported, only position.
/// - **iOS / Android / Web / Orbital:** Unsupported.
```

`set_ime_allowed` 的注记（同文件 L1272–1278）：

```
/// ## Platform-specific
///
/// - **macOS:** IME must be enabled to receive text-input where dead-key sequences are
///   combined.
/// - **iOS / Android:** This will show / hide the soft keyboard.
/// - **Web / Orbital:** Unsupported.
/// - **X11**: Enabling IME will disable dead keys reporting during compose.
```

**【推断】** 桌面三平台（macOS / Windows / X11 / Wayland）全部在 `set_ime_cursor_area` 的支持范围内，X11 只是忽略"区域大小"、只用左上角位置——对候选词定位来说这个降级基本无感。

### 1.4 预编辑（preedit）显示：0.35 起是真正的原生观感

**【事实】** 0.35.0 的 [#8083](https://github.com/emilk/egui/pull/8083) 引入了 `Visuals::ime_composition: ImeComposition` 结构。逐字摘自 <https://github.com/emilk/egui/blob/master/crates/egui/src/style.rs#L1206-L1229>：

```rust
pub struct ImeComposition {
    /// Stroke used to underline the actively composed segment.
    pub active_underline_stroke: Stroke,

    /// Stroke used to underline those non-active segments.
    pub inactive_underline_stroke: Stroke,

    /// If `true`, IME (Input Method Editor) composition (preedit) text is rendered
    /// the legacy way: visually indistinguishable from a text selection, with the
    /// cursor always shown at the end of the composition.
    ///
    /// If `false`, egui renders proper IME composition visuals: the cursor position
    /// inside the composition is shown, and the active conversion segment is
    /// highlighted (using the strokes configured above) distinctly from the rest of the
    /// composition. This makes composing Chinese, Japanese and Korean text much
    /// clearer.
    ///
    /// The legacy visuals have known shortcomings, but the new visuals are not yet
    /// fully reliable on every platform either (e.g. `winit` reports an incorrect
    /// cursor position for Korean IMEs on Windows), so this remains configurable.
    ///
    /// Defaults to `true` on Windows (because of the aforementioned `winit` bug) and
    /// to `false` everywhere else.
    pub legacy_visuals: bool,
}
```

**【事实】** 也就是说：预编辑文本**显示在输入框内**（不是浮在别处），带分段下划线，**但 Windows 上默认仍走 legacy 路径**，因为 winit 在 Windows 上给韩语 IME 报错误的光标位置。

**【推断】** 这个默认值是为韩语打的补丁。对纯中文输入场景，Windows 上手动 `visuals.ime_composition.legacy_visuals = false` 大概率能拿到更好的观感——**但这是我的推断，必须在真机+搜狗/微信输入法上验证后才能开**。

**【事实】** 0.35 还把 IME 事件的归属做成了公开 API，所以**自定义 widget 也能正确参与输入法**（docs.rs egui 0.36.1，`Memory`）：

- `owns_ime_events(id) -> bool` —— "Check if the widget owns IME events. A widget should only consume IME events if this returns `true`. **At most one widget can own IME events for each frame.**"
- `interrupt_ime()` —— "Interrupt the current IME composition, if any."

**【推断】** 这对本项目有实际意义：如果待确认队列的搜索框要做成带候选下拉的自定义控件（而不是裸 `TextEdit`），有这两个 API 就不会和系统输入法抢事件。

### 1.5 仍未关闭的 IME 缺陷（2026-08-31 实时查询）

**egui 仓库**（`repo:emilk/egui is:issue is:open IME`，共 18 条，下表是与桌面中文输入相关的）：

| # | 标题 | 平台 | 创建/更新 | 对本项目的影响 |
|---|---|---|---|---|
| [#7975](https://github.com/emilk/egui/issues/7975) | **On Linux X11 + Fcitx5, IME pre-edit text is not in `TextEdit`** | Linux X11 + Fcitx5 | 2026-03-13 / 2026-03-20 | **中等**。预编辑文本不显示在输入框里、而是浮在旁边的候选条上。issue 作者本人写 "This is not a big deal for Chinese"（对中文影响不大，因为中文预编辑是拼音串，在哪显示都能用；对韩语才致命）。同环境的 Wayland + IBus 正常。 |
| [#7974](https://github.com/emilk/egui/issues/7974) | macOS 韩文 IME 的汉字候选窗 `Option+Return` 打不开 | macOS | 2026-03-13 / 2026-03-17 | **无**。纯韩语功能，已上报 winit（[winit#4524](https://github.com/rust-windowing/winit/issues/4524)）。 |
| [#7941](https://github.com/emilk/egui/issues/7941) | `IMEOutput` 缺少 hint / text / input type 信息 | 移动端为主 | 2026-02-28 | **无**。桌面用不到。 |
| [#5817](https://github.com/emilk/egui/issues/5817) | UTF-8 字符打不进 `TextEdit`（Arch + fcitx5 / elementary OS） | Linux | 2025-03-17 / 2025-07-02 | **未知**。最后一条评论停在 egui 0.31.1 时代，**早于 0.35 的 IME 重写**，很可能已被 #7967/#7983 顺带修掉但无人复测。 |
| [#3532](https://github.com/emilk/egui/issues/3532) | 中文标点输不进去（Win10, egui 0.23） | Windows | 2023-11-06 / 2024-05-10 | **无**。评论里 @TicClick 2024-05-10 已指出"most likely fixed by #4436"，只是没人关掉。属于陈旧未清理的 issue。 |
| [#2317](https://github.com/emilk/egui/issues/2317) | Win11 下 Ctrl+Space 切不了输入法（egui 0.19） | Windows | 2022-11-17 / 2024-02-14 | **无**。评论里已确认 0.20.1 修好，2024-02-14 有人请求关闭但维护者没处理。 |

**winit 仓库**（`repo:rust-windowing/winit is:issue is:open IME`，共 37 条，下表是与桌面中文输入相关的）：

| # | 标题 | 平台 | 创建/更新 | 对本项目的影响 |
|---|---|---|---|---|
| [#2780](https://github.com/rust-windowing/winit/issues/2780) | **Sogou Input Method window cannot be located near the text windows** | Windows | 2023-04-28 / 2026-01-06 | **需实测**。搜狗候选词窗不跟随光标、固定在屏幕底部。2026-01-06 @csmoe 的诊断："搜狗忽略 `ImmSetCandidateWindow` 调用，转而依赖 `GetCaretPos` 定位；相比之下其他中文输入法与 winit 配合无碍。" 注意：PR #7967 的作者在 Windows 11 上实测过搜狗且未报告此问题——**两条证据冲突，必须自己测**。 |
| [#3814](https://github.com/rust-windowing/winit/issues/3814) | **Chinese IME punctuation behaviour is reversed on macOS** | macOS | 2024-07-23 / 2024-12-28 | **需实测**。IME 关闭时收到中文标点、开启时收到英文标点，正好反了。issue 作者自称没有 macOS 机器、证据来自 Neovide/Alacritty 的转述，且注明"未在最新 winit 上复测"。 |
| [#4626](https://github.com/rust-windowing/winit/issues/4626) | **macOS: `insertText` panics during window teardown with active marked text** | macOS | 2026-07-13 | **中等**。组字过程中关窗口 → `WinitView` 已无 `WinitWindow` → panic 无法 unwind → **进程 abort**。issue 里给了应用层解决办法："Calling `Window::set_ime_allowed(false)` while the window is still alive, immediately before requesting event-loop exit, prevents the abort in 20/20 runs."（20/20 次成功规避） |
| [#4508](https://github.com/rust-windowing/winit/issues/4508) | Windows: IME 允许期间仍在发 `KeyboardInput` 事件 | Windows | 2026-03-11 / 2026-03-17 | **低**。egui 侧的 #7967 已经在处理这类"泄漏"事件。 |
| [#4525](https://github.com/rust-windowing/winit/issues/4525) | X11: Fcitx5 组字期间泄漏 `Released` 键盘事件 | Linux X11 | 2026-03-17 / 2026-03-31 | **低–中**。与 egui#7975 同源。 |
| [#3092](https://github.com/rust-windowing/winit/issues/3092) | X11 下无法禁用 IME 或设置 IME 位置 | Linux X11 | 2023-09-10 / 2024-08-27 | **低**。 |
| [#3342](https://github.com/rust-windowing/winit/issues/3342) | **IME 与 macOS 字符检视器（Character Viewer）不兼容** | macOS | 2023-12-30 / 2025-02-24 | **中等，且与本项目直接相关**。macOS 上按 fn/🌐 打开的字符检视器是插入 ★ ♪ Ⅲ 这类符号的标准方式，双击字符**不会给 winit 应用发任何事件**（对其他应用正常）。报告者的诊断：只有在 IME 已经处于组字状态时才收得到。**ROM 标题里恰好常有这类符号**。规避：从别处复制粘贴，或在应用里做一个符号选择器。egui 侧的对应 issue 是 [#2359](https://github.com/emilk/egui/issues/2359)（开着）。 |
| [#4412](https://github.com/rust-windowing/winit/issues/4412) | New IME API is too restrictive | 全平台 | 2025-11-17 / 2026-04-29 | **无**（架构讨论）。 |

### 1.6 风险一小结

**【事实】** 一个需要大量中文输入的 egui 应用，**在 0.35.0 及以上版本上今天是可用的**。核心证据：上游维护者在 macOS 内置中文、Windows 内置中文 + 搜狗 + 微信输入法上逐一实测过；中文输入最致命的两个体验 bug（Enter 上屏多插换行、组字退格误删已有文字）已于 2026-03-24 修复。

**【事实】** 三个必须自己实测的盲区：

1. **Windows + 搜狗的候选窗定位**（winit#2780，2023 年至今未关）。证据冲突：issue 说坏，egui PR 作者说测过。
2. **macOS 中文标点方向**（winit#3814，2024 年，证据是二手转述且未在新版复测）。
3. **Linux X11 + Fcitx5 的预编辑位置**（egui#7975，已确认坏，但对中文影响有限；换 Wayland + IBus 即正常）。

**【事实】** 上游还有一批未发布的修复在路上。winit `master` 的 `winit/src/changelog/unreleased.md` 里有两条与本项目相关：

> - On macOS, fix **IME being locked on (regardless of requests to disable) after being enabled once**.
> - On macOS, fix **a panic and incorrect cursor position in `Ime::Preedit` when the preedit string contains special characters (ie. emojis)** caused by incorrect UTF-16 to UTF-8 offset conversion.

**【事实】** winit 已有 **0.31.0-beta.2**（2025-11-16），带 IME API 的破坏性改动（`ImeRequest`、`ImeSurroundingTextError` 等新类型出现在 unreleased changelog 的重命名列表里）。egui 0.36.1 仍锁 winit 0.30.13（该 tag 发布于 2026-03-02）。

**【推断】** 这意味着两件事：(a) macOS 上"IME 一旦开启就关不掉"这个问题在现行 0.30.13 上是存在的（第 0 步应顺带留意）；(b) 未来 egui 跟进 winit 0.31 会是一次有风险的升级，**升级时必须重跑第 0 步的验证清单**。

**【事实】** 两个已确认存在、需要在应用层处理的问题：

- macOS 上退出前调用 `set_ime_allowed(false)`，否则组字中关窗会进程 abort（winit#4626）。
- **macOS 字符检视器（fn/🌐）插不进符号**（winit#3342 / egui#2359，均开着三年）。对本项目直接相关，因为 ROM 标题里常有 `★ ♪ Ⅲ`。规避：自己做个符号选择器，或走粘贴。

---

## 风险二：中文字体渲染

### 2.1 内置字体确实不含 CJK —— 逐码位验证

**【事实】** egui 的内置字体一共四个，全部列在 <https://github.com/emilk/egui/blob/master/crates/epaint_default_fonts/src/lib.rs>：

| 常量 | 文件 | 大小 | 用途（官方 doc comment） |
|---|---|---|---|
| `HACK_REGULAR` | `Hack-Regular.ttf` | 302 KiB | 等宽代码字体 |
| `NOTO_EMOJI_REGULAR` | `NotoEmoji-Regular.ttf` | 409 KiB | 黑白 emoji |
| `UBUNTU_LIGHT` | `Ubuntu-Light.ttf` | 353 KiB | 默认比例字体 |
| `EMOJI_ICON` | `emoji-icon-font.ttf` | 317 KiB | 图标字体 |

**【实测】** 我下载了这四个文件，写了一个 `cmap` 表解析器逐码位检查（macOS，2026-08-31）：

```
Hack-Regular.ttf:       可映射码位 1548，CJK 统一表意文字(U+4E00–9FFF) 0 个，假名(U+3040–30FF) 0 个
NotoEmoji-Regular.ttf:  可映射码位  887，CJK 0 个，假名 0 个
Ubuntu-Light.ttf:       可映射码位 1194，CJK 0 个，假名 0 个
emoji-icon-font.ttf:    可映射码位  652，CJK 2 个（非汉字，是两个图标复用了码位），假名 0 个
```

**你的理解完全正确：默认字体里一个汉字都没有。**

**【实测】** 更要紧的是，缺的**远不止汉字**。下面是这四个字体合起来**仍然完全无法显示**的字符（会渲染成 `?` 或空白）：

| 类别 | 缺失字符 |
|---|---|
| 汉字 | 简 / 繁 / …… 全部 |
| 假名 | あ (U+3042) / カ (U+30AB) / …… 全部 |
| **中文标点** | **，** (U+FF0C 全角逗号) / **。** (U+3002 句号) / **、** (U+3001 顿号) / **《** (U+300A 书名号) / **～** (U+FF5E 全角波浪) / **〜** (U+301C 波浪号) |
| 日文中点 | ・ (U+30FB) / ･ (U+FF65) |
| **游戏标题常见符号** | **Ⅲ** (U+2162 罗马数字) / **ⅲ** (U+2173) / **①** (U+2460 带圈数字) / **Ⓡ** (U+24C7) |

**【实测】** 而这些**默认字体已经能显示**（不必额外操心）：`“ ” — … § ° ¥ ™ α →`（在 Hack/Ubuntu 里），以及 `★ ☆ ♪ ♥ ￥`（在 `emoji-icon-font` 里）。

**【推断】** 这里有个隐蔽的坑：`★ ♪` 能显示，但它们来自 emoji 图标字体，**字形风格与正文字体完全不搭**（是 emoji 图形不是文字符号）。ROM 标题里的 `★` 会长得像 emoji 而不是标点。加载中文字体后，如果把中文字体放在 `Proportional` 家族的**首位**，这些符号会改由中文字体渲染，风格反而统一——这是把中文字体插在 `index 0` 而不是 `push` 到末尾的一个额外理由。

**【事实】** 唯一一处官方对此的说明，在 README 的 FAQ（<https://github.com/emilk/egui#can-i-use-egui-with-non-latin-characters>）：

> ### Can I use `egui` with non-latin characters?
> Yes! But you need to install your own font (`.ttf` or `.otf`) using [`Context::set_fonts`].

### 2.2 官方推荐做法

**【事实】** `FontDefinitions` 的 doc comment 给出了官方范式（<https://github.com/emilk/egui/blob/master/crates/epaint/src/text/fonts.rs#L400-L430>）：

```rust
let mut fonts = FontDefinitions::default();
fonts.font_data.insert("my_font".to_owned(),
   std::sync::Arc::new(FontData::from_static(include_bytes!("....ttf"))));
// 放在首位（最高优先级）：
fonts.families.get_mut(&FontFamily::Proportional).unwrap().insert(0, "my_font".to_owned());
// 或作为等宽家族的最后回退：
fonts.families.get_mut(&FontFamily::Monospace).unwrap().push("my_font".to_owned());
egui_ctx.set_fonts(fonts);
```

**【事实】** 0.28 起还有更简洁的 `Context::add_font(FontInsert)`（[#5228](https://github.com/emilk/egui/pull/5228)），带 `FontPriority::{Highest, Lowest}` 语义。

**【事实】** 回退链是**按字形逐字符查找**的：`FontDefinitions::families` 的 doc 写 "When looking for a character glyph `epaint` will start with the first font and then move to the second, and so on."，实现在 `find_face_for_char`（fonts.rs L677）"Walk the fallback chain and return the first face whose charmap supports `c`"，并且有 `char → face` 的缓存。**所以中英文混排、以及"中文字体 + emoji 字体"的组合是原生支持的，不用自己拆字符串。**

**【事实】** `FontData` 支持 `.ttc` 字体集合：结构体里有 `pub index: u32` —— "Which font face in the file to use."（fonts.rs L116–118）。macOS/Windows 的系统中文字体几乎都是 `.ttc`，这一点是通的。

### 2.3 体积：实测数据

**【事实】** `FontData.font` 的类型是 `Cow<'static, [u8]>`（fonts.rs L114）——**整个字体文件必须完整读进内存**，没有按需/mmap 通路。这是下面所有取舍的根因。

**【实测】** 我下载了 `NotoSansSC[wght].ttf`（来自 <https://github.com/google/fonts/blob/main/ofl/notosanssc/NotoSansSC%5Bwght%5D.ttf>，17,772,300 字节），用 `fonttools 4.63.0` 做了裁剪，2026-08-31 在本机测得：

| 方案 | 字符数 | TTF 大小 | gzip 后 |
|---|---:|---:|---:|
| 完整 Noto Sans SC（可变字重） | 30,890 | **16.9 MB** | 10.8 MB |
| 完整，固定 `wght=400` | 30,890 | **10.1 MB** | — |
| **GB2312 + 中文标点 + 假名 + 常用符号**，固定字重 | 9,006 | **2.5 MB** | 1.5 MB |
| **GBK（含繁体）+ Big5 + 标点假名符号**，固定字重 | 23,246 | **7.4 MB** | 4.4 MB |

**【实测】** Noto Sans SC **覆盖了本项目需要的全部字符**——我逐码位验证过下面这 25 个（用空格分隔，避免与被测的顿号混淆）：

```
简 繁 龍 鬱 囧    あ カ    ， 。 、 《 ～ 〜 ・
★ ☆ ♪ ♥ Ⅲ ① ￥ Ⓡ → ∀
```

**全部命中，无一缺失。**

**【推断】** 对一个 ROM 元数据管理器：日文标题需要假名 + 日本汉字，繁体标题需要 Big5/GBK 扩展区。**7.4 MB 的 GBK+Big5 子集是合适的落点**；如果确定只做简体，2.5 MB 就够。相比之下一个 Rust GUI 的 release 二进制本身就有十几到几十 MB，**加 7 MB 字体不构成分发问题**。

**【事实】** 其他常见中文字体的官方发布体积（GitHub Releases API 查得）：

- LXGW WenKai（霞鹜文楷）v1.522（2026-03-17）：`LXGWWenKai-Regular.ttf` **24.3 MB**
- Noto Sans CJK SC（notofonts/noto-cjk `Sans2.004`）：`18_NotoSansSC.zip` **47.7 MB**、`13_NotoSansMonoCJKsc.zip` 26.4 MB

### 2.4 「按需从系统字体加载」的现状

**【事实】** egui 官方**没有**这个能力，而且是个已知的、开着的 feature request：

- [emilk/egui#5233 — "Automatically load system fonts when needed"](https://github.com/emilk/egui/issues/5233)，**开着**，标签 `help wanted` / `eframe` / `text` / `egui`，2024-10-08 由 emilk 本人开，最后更新 2025-12-04。原文：
  > "Currently egui only supports glyphs available in fonts that has been explicitly installed with `ctx.set_fonts`. It would be great if there was a way for `egui` to tell it's integration 'The user wanted to show a Chinese glyph, but no current font has support for that'…"

  **值得注意的一条评论**（@haixuanTao, 2024-10-09）：
  > "FYI, some country like China has a GFW that can limit access to western website like google.font that can make dynamically loaded asset fail to load. 🥹 I think that having a fully packaged chinese-compatible release could be easier to manage."

  以及 @GamingLiamStudios（2025-12-04）自己实现的教训：用 fontconfig/DirectWrite 枚举系统字体全量加载，**Linux 上占 200 MB、Windows 上占 800 MB 内存**，因为 egui 要求所有字体完整驻留内存。他提议加一个 `Context::set_font_loader` 回调，但目前只是提议。

- [emilk/egui#7325 — "Provide a way to reduce memory usage by fonts"](https://github.com/emilk/egui/issues/7325)，**开着**，2025-07-09。由 [Ruffle](https://github.com/ruffle-rs/ruffle) 团队提出：他们同时加载 Noto Sans CJK KR/JP/SC/TC + Hebrew + Arabic，内存吃不消，希望 egui 支持 "fonts without loading them into memory in their entirety"。原文还提到 "We were thinking about mmapping font files into memory, but it's inherently unsafe in Rust."

**【事实】** 社区的实际做法是**硬编码系统字体路径**。最有分量的样本是 **Servo 浏览器自己的 servoshell**（它的工具栏 UI 就是 egui），<https://github.com/servo/servo/blob/main/ports/servoshell/desktop/gui.rs>：

```rust
#[cfg(target_os = "windows")]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        (r"C:\Windows\Fonts\malgun.ttf", "Malgun Gothic"), // Korean
        (r"C:\Windows\Fonts\msyh.ttc",   "Microsoft YaHei"), // Chinese + Japanese
    ])
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn configure_fonts() -> FontDefinitions {
    load_cjk_fonts(&[
        ("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc", "Noto Sans CJK"), // Ubuntu/Debian
        ("/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",      "Noto Sans CJK"), // Fedora/Arch
        // …FreeBSD 各区域子集…
        ("/usr/share/fonts/truetype/wqy/wqy-microhei.ttc", "WenQuanYi Micro Hei"), // common fallback
    ])
}

#[cfg(target_os = "macos")]
fn configure_fonts() -> FontDefinitions {
    // TODO: Default proportional fonts: ["Ubuntu-Light", "NotoEmoji-Regular", "emoji-icon-font"]
    // does not support CJK. Add them for Mac.
    FontDefinitions::default()
}
```

**注意 macOS 分支是个 TODO —— Servo 自己都没做。**

**【实测】** 我在本机（macOS）查了原因，这是硬路径方案在 macOS 上失效的具体机制：

```
$ fc-match -f "%{file}\n" ":lang=zh-cn"
/System/Library/AssetsV2/com_apple_MobileAsset_Font7/3419f2a427639ad8c8e139149a287865a90fa17e.asset/AssetData/PingFang.ttc
```

**苹方（PingFang）不在 `/System/Library/Fonts/` 下，而在一个哈希命名的 AssetsV2 路径里，且文件大小 78,201,856 字节（74.6 MB）。** 这个哈希会随系统版本变化，**不可硬编码**。

**【实测】** 本机上路径稳定的中文字体及其体积：

| 路径 | 大小 |
|---|---:|
| `/System/Library/Fonts/Supplemental/Songti.ttc` | 63.8 MB |
| `/System/Library/Fonts/STHeiti Light.ttc` | 53.2 MB |
| `/System/Library/Fonts/Hiragino Sans GB.ttc` | 24.7 MB |
| `/System/Library/Fonts/Supplemental/Arial Unicode.ttf` | 22.2 MB |

**【实测】** 还有一个走系统字体才会暴露的问题：**系统字体的符号覆盖是不一致的**。我用 `fonttools` 直接读了本机字体的 `cmap`（2026-08-31）：

| 字体 | 可映射码位 | 简 繁 あ カ ，。《 ★ | **♪** | Ⅲ ① ～ ・ → 龍 | **Ⓡ** |
|---|---:|:---:|:---:|:---:|:---:|
| 苹方 PingFang.ttc（macOS 默认中文） | 33,257 | ✓ | **✗** | ✓ | ✓ |
| 冬青黑 Hiragino Sans GB.ttc | 29,026 | ✓ | **✗** | ✓ | **✗** |
| Arial Unicode.ttf | 38,917 | ✓ | ✓ | ✓ | ✓ |
| **Noto Sans SC（我推荐打包的那份）** | 30,890 | ✓ | ✓ | ✓ | ✓ |

**macOS 的默认中文字体苹方，显示不了 `♪`。** 走系统字体路线意味着你的界面在不同机器上会有不同的缺字表，且无法在自己机器上复现——**打包一份固定的子集字体，等于把"哪些字能显示"变成一个编译期常量。**

**【推断】** 综合起来：即便走系统字体，因为 `FontData` 要求整份进内存，代价是 **20–75 MB 常驻 RSS**——比打包一份 7.4 MB 的子集**更贵**；再加上路径不稳定（macOS 哈希路径）、发行版差异、覆盖不一致三类问题。**对本项目，打包子集字体是更优解，不是妥协。**

**【事实】** 可选的社区 crate（均非官方，成熟度需自行判断，crates.io API 查询于 2026-08-31）：

| crate | 版本 | 更新 | 累计下载 | 备注 |
|---|---|---|---:|---|
| [`egui-system-fonts`](https://github.com/yijehyung/egui-system-fonts) | 0.36.0 | 2026-08-15 | 3,274 | 紧跟 egui 版本号；`set_auto(ctx, FontStyle::Sans)` 按系统 locale 自动挑字体。**我核对过源码**：它依赖 `ehttp` 从 `cdn.jsdelivr.net` 下载 Noto CJK，但该路径整个包在 `#[cfg(target_arch = "wasm32")] mod wasm_defaults` 里——**原生桌面走 `system_fonts::find_for_system_locale()` 读本地系统字体，不联网**。GFW 风险只在 web 版存在。 |
| `egui-chinese-font` | 0.2.0 | 2026-06-19 | 2,367 | |
| `egui-cjk-font` | 0.2.0 | 2026-03-27 | 424 | |
| `egui_zhcn_fonts` | 0.1.1 | 2025-02-24 | 2,003 | |
| `font-kit`（servo） | 0.14.3 | 2025-05-26 | 16.8M | 通用字体枚举/加载；macOS 走 CoreText，能拿到字节 |
| `fontdb`（RazrFalcon） | 0.24.0 | 2026-07-29 | 30.6M | 纯 Rust，`load_system_fonts()` |

**【推断】** 这些 egui 专用 crate 下载量都在千级，属于个人项目量级，**不建议放进关键路径**。若确实要做系统字体探测，用 `fontdb`（3000 万下载、活跃）自己写 20 行更稳。

### 2.5 中文文本排版：egui 是懂 CJK 的

**【事实】** 这不是"能显示就行"级别的支持，源码里有 CJK 专门处理：

- `epaint/src/text/font.rs` L863 `is_cjk_ideograph()`，覆盖 U+4E00–9FFF、U+3400–4DBF、U+2B740–2B81F
- `text_layout.rs` 引用 `is_cjk_break_allowed` —— **CJK 换行规则**（中文不靠空格断行）
- `font.rs` L622–627：**CJK 字形跳过亚像素定位**，注释 "CJK scripts contain a lot of characters and could hog the glyph atlas if we stored 4 subpixel offsets per glyph." —— 说明字形图集是为 CJK 的字符量做过针对性设计的
- `TextOptions::subpixel_binning` 的 doc（`text/mod.rs` L48）："This is always disabled for CJK characters (which have too many unique glyphs)."

**【事实】** 0.34 起文本渲染换成了 `skrifa` + `vello_cpu`（[#7694](https://github.com/emilk/egui/pull/7694)），0.36 起用 `harfrust` 做字距与连字（[#8031](https://github.com/emilk/egui/pull/8031)）。

**【事实】** 历史 issue [#962 "Rendering of Chinese characters is very slow"](https://github.com/emilk/egui/issues/962)（2021-12-17 开，**2024-08-27 已关**）：用户抱怨加了中文 TTF 后 demo 启动要 10 秒。维护者回复 "I tried it locally and had no problems with performance. Make sure you compile with `--release`!"，无人复现，最终关闭。**【推断】** 这是 debug 构建的问题，不是 egui 的中文渲染瓶颈。

---

## 风险三：十万行级表格性能

### 3.1 虚拟化机制：官方提供且够用

**【事实】** egui 提供两级虚拟化 API：

1. `ScrollArea::show_rows(ui, row_height, total_rows, |ui, row_range| …)` —— 官方 doc："Efficiently show only the visible part of a large number of rows."（<https://docs.rs/egui/0.36.1/egui/containers/scroll_area/struct.ScrollArea.html#method.show_rows>）
2. `egui_extras::TableBody::rows(row_height, total_rows, |row| …)` —— **是的，`TableBuilder` 支持虚拟化**。官方 doc："Add many rows with same height. Is a lot more performant than adding each individual row as non visible rows must not be rendered."；`TableBody::row`（单行版）则带警告："⚠️ It is much more performant to use `Self::rows` or `Self::heterogeneous_rows`, as those functions will only render the visible rows."
3. `TableBody::heterogeneous_rows(heights_iter, |row| …)` —— 变高行；doc 明说代价："a very slight performance hit … due to the need to iterate over all row heights"，**即 O(总行数) 的高度累加**。

**【事实】** `rows()` 的实现（<https://github.com/emilk/egui/blob/master/crates/egui_extras/src/table.rs#L1027-L1082>）确认了开销模型：

```rust
let scroll_offset_y = self.scroll_offset_y().min(total_rows as f32 * row_height_with_spacing);
let max_height = self.y_range.span();
let mut min_row = 0;
if scroll_offset_y > 0.0 {
    min_row = (scroll_offset_y / row_height_with_spacing).floor() as usize;
    self.add_buffer(min_row as f32 * row_height_with_spacing);
}
let max_row = ((scroll_offset_y + max_height) / row_height_with_spacing).ceil() as usize + 1;
let max_row = max_row.min(total_rows);
for row_index in min_row..max_row { /* 只对可见行调用回调 */ }
if total_rows - max_row > 0 { self.add_buffer(/* 下方占位 */); }
```

**逐帧只对 `min_row..max_row` 调回调，上下各放一个占位块。CPU 开销与 `total_rows` 无关，只与视口高度有关。**

**【推断】** 因此"每帧重建 UI"这个立即模式的天然弱点，在**等高虚拟化表格**上被完全消除：十万行和一百行的每帧成本几乎相同。**代价是 `heterogeneous_rows` 不享受这个性质**（要遍历全部高度）——所以待确认队列**必须用等高行**。

### 3.2 十万行的实际表现

**【事实】** 官方 demo 的表格演示里，行数滑杆的上限**恰好就是 100 000**（<https://github.com/emilk/egui/blob/master/crates/egui_demo_lib/src/demo/table_demo.rs#L91>）：

```rust
egui::Slider::new(&mut self.num_rows, 0..=100_000).logarithmic(true).text("Num rows")
```

默认值 `10_000`。这个 demo 在 <https://www.egui.rs/#demo> 上可直接跑（web 版，README 第 19 行的官方入口），**是最省事的存在性验证：拖到 100 000 自己滚一遍。**

**【事实】** 我没有找到官方发布的十万行基准数字（egui 仓库的 `benches/` 只有 `egui_demo_lib/benches/benchmark.rs` 和 `epaint/benches/benchmark.rs`，都不是表格基准）。**这一项无一手数据。**

**【事实】** 有一条上界证据：[emilk/egui#1391 "ScrollArea::show_rows() scrolling jitters with very large total_rows"](https://github.com/emilk/egui/issues/1391)（**开着**，2022-03-21，仅 emilk 回复过一次）。报告者原文：

> "Up to about **2 million rows**, scrolling with the trackpad or by dragging the background with a mouse is relatively smooth. Above 2 million total rows, scrolling has some obvious jitter. Above 100 million total rows, the scrolling gets very broken."

emilk 的确认："Yup, `f32` will cause trouble here. Seems like we need to switch to `f64` in `ScrollArea::show_rows`…"

**【推断】** 抖动阈值是 f32 精度问题，本质取决于**总像素高度**而非行数。f32 尾数 24 位，所以：

| 场景 | 内容总高 | f32 ULP | 报告者的描述 |
|---|---:|---:|---|
| **本项目：10 万行 × ≈21 px**（egui 默认行高 = `TextStyle::Body` 与 `interact_size.y≈18` 取大，再加 `item_spacing.y≈3`） | 2,100,000 px | **0.25 px** | — 亚像素，不可见 |
| 报告者的抖动起点：200 万行 × ≈15 px | 30,000,000 px | **2 px** | "obvious jitter" |
| 报告者的崩坏点：1 亿行 × ≈15 px | 1,500,000,000 px | **128 px** | "almost completely unusable" |

**这个模型同时解释了报告者的两个观测点**（2 px 抖动 ↔ "明显抖动"，128 px ↔ "几乎不可用"），所以可以放心外推：**你的十万行比抖动阈值低一个数量级以上，安全。**

**【事实】** 还有一个更强的方案：**[`egui_table`](https://github.com/rerun-io/egui_table) 0.10.0**（Rerun 官方出品，2026-08-05 发布，与 egui 0.36 同步，累计 128 万次下载）。它的 README 明写特性 "**Support for millions of rows**"，`Table` 结构体 doc 写 "A table viewer. Designed to be fast when there are millions of rows, but only hundreds of columns."。源码里行号类型是 `u64` 而非 `usize`（`num_rows: u64`, `scroll_to_row(row: u64)`）。

**【事实 / 修正】** 但我核对了它的源码：`row_top_offset` / `get_row_top_offset` / `get_row_nr_at_y_offset` **仍然全部用 f32**，没有 f64、没有窗口化相对偏移、没有前缀和。**所以 egui_table 并没有解决 f32 滚动精度天花板**，它相对 `egui_extras` 的真实优势是：u64 行号（不溢出）、通过 delegate 提供 O(1) 的变高行偏移查询（而不是 `heterogeneous_rows` 的 O(n) 累加）、冻结列、多级表头。**"millions of rows" 指的是"每帧开销与行数无关"，不是"滚动条在百万行下依然平滑"。**

**【推断】** 对本项目（十万行、等高）**两者都够用**。选 `egui_extras::TableBuilder` 起步更简单；只有当你需要**冻结左侧的"作品/发行版"列**横向滚动看后面的字段时，才值得换 `egui_table`——这对一个字段很多的元数据表其实是个很现实的需求，可以早点决定。

### 3.3 功能矩阵：给了什么，要自己写什么

**【事实】** `egui_extras::Table` 的完整公开 API 我逐条看过（table.rs，1373 行）：

| 能力 | `egui_extras::TableBuilder` | `egui_table::Table` | 自己要写多少 |
|---|---|---|---|
| **虚拟化** | ✅ `body.rows()` | ✅ 原生 u64 | 0 |
| **列宽拖拽** | ✅ `TableBuilder::resizable(true)` + `Column::resizable(true)`，还有 `Column::{auto, initial, exact, remainder, at_least, at_most, range}` | ✅ `Column::resizable(true)` | 0 |
| **表头固定** | ✅ `TableBuilder::header(height, …)` | ✅ 且支持**多级表头**与**冻结列** `num_sticky_cols` | 0 |
| **斑马纹** | ✅ `striped(true)` | — | 0 |
| **滚动到指定行** | ✅ `scroll_to_row(row, align)` | ✅ `scroll_to_row` / `scroll_to_rows` | 0 |
| **行悬停/选中的视觉** | ✅ `TableRow::set_selected(bool)` / `set_hovered` / `set_overline` —— **仅是视觉，不管状态** | ✅ 类似 | 状态自管 |
| **行点击响应** | ✅ `TableBuilder::sense(Sense::click())` + `TableRow::response()` | ✅ | — |
| **多列排序** | ❌ **完全没有**（在 table.rs 全文里 `sort` 出现 **0** 次） | ❌ | **全部自己写**：表头点击态、排序方向图标、菜单、以及对数据源的重排 |
| **多选 / Shift 范围选 / Ctrl 加选** | ❌ | ❌ | **全部自己写**。官方 demo 只给了最朴素的 20 行版本（`fn toggle_row_selection`：点一下就 `HashSet` 里 insert/remove，**没有 Shift 范围、没有 Ctrl 累加**） |
| **行内编辑** | 🟡 单元格里可放任意 widget（`row.col(\|ui\| { ui.text_edit_singleline(…) })`）。**但官方 demo 连按行编辑都没演示**：它的单元格 checkbox 绑的是 `TableDemo` 上一个共享的 `checked: bool`（tdemo 第 23/231/267/301 行），**所有行点的是同一个值** | 🟡 同 | 编辑态管理、提交/取消、校验全自己写 |
| **列筛选** | ❌ | ❌ | 全部自己写 |

**【事实】** "仅是视觉"不是我的解读，是 doc 原文：`/// Set the selection **highlight** state for cells added after a call to this function.`，实现体只有一行 `self.selected = selected;`（table.rs）。**表格不持有任何选中集合，全部由你维护。**

**【事实】** 「自己写多少」的量级参考：**Rerun 在 `egui_table` 之上另写了一整个 crate `re_dataframe_ui`** 来提供排序 + 筛选 + 多选的生产级表格。其中 `datafusion_table_widget.rs` 单文件 **51 KB / 1419 行**，另有独立的 `column_sorting`、`table_selection`、`filters/*`（8 个文件）模块，整个 `crates/viewer/re_dataframe_ui/src/` 约 300 KB 源码。见 <https://github.com/rerun-io/rerun/tree/main/crates/viewer/re_dataframe_ui>。

**【推断】** Rerun 那 300 KB 里很大一部分是 DataFusion/Arrow 特有的查询下推与筛选 UI，不是你需要的。**对本项目，「排序 + Shift/Ctrl 多选 + 单元格内编辑」这一层，估计 800–1500 行 Rust。这是可控的，但必须计入排期，不能当成「装个库就有」。**

**【推断】** 一条对本项目特别重要的设计约束：排序必须在**数据层**做（对 `Vec<候选>` 排序或让 SQL/索引出有序结果），**不能在渲染层做**——因为 `rows()` 只把 `row_index` 传给你，你必须自己拿它去索引一个**已经排好序的**数组。这反过来是好事：十万行的排序在数据层是毫秒级的，只在点表头时做一次。

### 3.4 立即模式的耗电问题

**【事实】** 这是 egui README 的原文（<https://github.com/emilk/egui/blob/master/README.md#cpu-usage>，第 232–234 行）：

> "Since an immediate mode GUI does a full layout each frame, the layout code needs to be quick. If you have a very complex GUI this can tax the CPU. In particular, **having a very large UI in a scroll area (with very long scrollback) can be slow, as the content needs to be laid out each frame.**
>
> If you design the GUI with this in mind and refrain from huge scroll areas (**or only lay out the part that is in view**) then the performance hit is generally pretty small. For most cases you can expect `egui` to take up **1-2 ms per frame**… **`egui` only repaints when there is interaction (e.g. mouse movement) or an animation, so if your app is idle, no CPU is wasted.**"

**【事实】** 这个"只在需要时重绘"的机制是 eframe 的默认行为，官方称为 **Reactive 模式**。逐字摘自 <https://github.com/emilk/egui/blob/master/crates/egui_demo_app/src/backend_panel.rs>：

```rust
/// If this is selected, egui is only updated if are input events
/// (like mouse movements) or there are some animations in the GUI.
///
/// Reactive mode saves CPU.
///
/// The downside is that the UI can become out-of-date if something it is supposed to monitor changes.
/// … you need to call `egui::Context::request_repaint()` each time such an event happens.
Reactive,

/// This will call `egui::Context::request_repaint()` at the end of each frame …
/// For games or other interactive apps, this is probably what you want to do.
Continuous,
```

且 `impl Default for RunMode { fn default() -> Self { Self::Reactive } }`，注释："Default for demo is Reactive since 1) We want to use minimal CPU 2) There are no external events that could invalidate the UI"。

**【事实】** 主动唤醒的三个 API（docs.rs egui 0.36.1，`Context`）：
- `request_repaint()` —— "Call this if there is need to repaint the UI"；可从**非 UI 线程**调用（会唤醒 UI 线程）
- `request_repaint_after(duration)` —— "Request repaint after at most the specified duration elapses."；官方 doc 明确其用途是**省电**："This function is useful to ensure that the UI is updated in a timely manner without wasting resources"（多次调用取最小值）
- `request_repaint_after_secs(f32)`

**【推断】** 对本项目这是**理想匹配**：待确认队列是"人看着、偶尔点一下"的界面，绝大多数时间完全空闲 → 零重绘、零 CPU。后台扫描/哈希线程算完一批后调一次 `ctx.request_repaint()` 推进度即可。**在笔记本上不会有持续耗电问题——前提是不要无脑写 `ctx.request_repaint()` 在 `update()` 末尾（那就退化成 Continuous 模式了）。**

### 3.5 已知性能坑

**【事实】** [emilk/egui#8093 "Text layout performance regression"](https://github.com/emilk/egui/issues/8093)，**开着**，2026-04-11 创建 / 2026-05-30 更新。0.33 → 0.34 的字形栅格化回归。根因由 @RyanJamesStewart 于 2026-05-15 定位（引原文）：

> "The never-cached-during-zoom mechanism is in `GlyphCacheKey::new` (`crates/epaint/src/text/font.rs`): the key hashes `pixels_per_point.to_bits()` and `px_scale_factor.to_bits()` exactly, so every frame of a continuous zoom is a fresh key and a full skrifa + vello_cpu rasterization"

其 criterion 基准（55 个字形的正文，i9-13900HX 单核）：**连续缩放时每帧 440 µs，而字号稳定时（warm control）只要 80 ns —— 相差 5500 倍。** emilk 2026-05-25 回复：缓存方案难做，"Any solution should ensure pixel-perfect rendering in the normal/happy case. This means scale-binning won't work in the general case." **仍未修。**

**【推断】** 对本项目影响：**几乎为零，但有一个明确禁区**。只要字号固定，字形全部命中缓存（80 ns 级）。禁区是：**不要给表格做平滑缩放动画，也不要让行高/字号随滚动或悬停连续变化**。用户按 Cmd+`+` 改 UI 缩放是离散的一步，只会有一帧的重新栅格化，无感。

**【推断】** 另一条与本项目直接相关的隐患（**来源未明说，是我的推断**）：虚拟化 + 行内编辑的组合有风险。`rows()` 只构造 `min_row..max_row` 的 widget——如果用户正在某个单元格里用输入法组字，此时表格因为任何原因滚动，那个 `TextEdit` 当帧就不存在了。而 egui#7975 的评论里 @rustbasic 记录了一个相关的真实行为：

> "When composing Korean characters (IME), if the focus moves to another element, the OS sends the final `Commit` signal **after** the focus has already shifted. … The issue mentioned in PR #7967, where characters are incorrectly copied to other fields, is directly related to this behavior."

**规避办法：待确认队列的中文输入不要做成表格单元格内联编辑，而是选中行 → 在下方/右侧的固定详情面板里编辑。** 详情面板不受虚拟化影响，输入框位置稳定，IME 通路最短。这同时也解决了"十万行表格里点中一个 3 px 高的单元格开始打字"的可用性问题。

---

## 附加一：存在性证明

**【事实】** 表格密集 + 生产级：

| 项目 | 证据 |
|---|---|
| **Rerun** (<https://github.com/rerun-io/rerun>) | ⭐11,374，2026-08-29 仍在推送。机器人多模态数据可视化平台，整个 viewer 是 egui。用 `egui_table` 实现 DataFusion 支撑的数据表（`crates/viewer/re_dataframe_ui/src/datafusion_table_widget.rs`），含排序、筛选、多选。**这是"egui 能扛生产级大表格"的最强证据**。<br>关系是明写在 egui README 里的（第 118 行）："egui can be used to create professional looking applications, like **the Rerun Viewer**"；第 374 行："**egui development is sponsored by Rerun**"。`egui_table` 是 Rerun 为自己的表格需求造的，所以"支持百万行"不是营销话术而是它自己的生产需求。 |
| **egui 官方 demo** (<https://www.egui.rs/#demo>) | Table demo 的行数滑杆上限 100 000，含列宽拖拽、行选中、单元格内 checkbox。可在浏览器里立刻验证。 |

**【事实】** 含 CJK 的 egui 应用：

| 项目 | 证据 |
|---|---|
| **Servo / servoshell** (<https://github.com/servo/servo>) | Servo 浏览器的桌面外壳 UI 用 egui，并且专门实现了 `load_cjk_fonts()` 加载 Microsoft YaHei / Noto Sans CJK / 文泉驿微米黑。**说明"egui + 中文"在一线项目里是被认真对待的**，也说明 macOS 分支至今是 TODO。 |
| **komorebi-bar** (<https://github.com/LGUG2Z/komorebi>) | Windows 平铺窗口管理器的状态栏，代码里加载 `msyh`。 |

**【事实 / 局限】** **我没有找到一个"知名的、中文界面 + 表格密集"的 egui 应用可以作为完整的存在性证明。** GitHub 代码搜索命中的中文 egui 项目（`egui-chinese-font`、`sgnay/simple-translation`、`cheng01315/batch-image-watermark` 等）全部是 0–4 星的小工具。

**【推断】** 这个空缺**不构成否决理由**——「中文输入」和「十万行表格」两个能力各自都有强证据（前者是上游维护者的实测矩阵，后者是 Rerun + 官方 demo），只是没人把它们合在一个知名项目里。但它确实意味着：**你会是这条路径上比较靠前的人，遇到问题时社区里可能没有现成答案。**

---

## 附加二：备选 GUI 方案

前提不变：**核心能力是独立的 lib + CLI，GUI 只是一层壳**，所以换壳的代价被限制在 GUI 层（ADR 0005）。这一节是**只在第 0 步验证失败时才需要执行**的应急预案。

### 什么情况下才该换

**【推断】** 只有一种情况值得换：**第 0 步里 Windows 搜狗候选窗定位（winit#2780）挂了，且搜狗必须支持。** 理由是：

1. 这个缺陷在 **winit 层**，不是 egui 层——**换 `iced` 也无用**，因为 iced 的桌面运行时同样跑在 winit 上（`iced_winit` crate 的官方描述逐字是 "A runtime for iced on top of winit"，v0.14.0 / 2025-12-07，<https://crates.io/crates/iced_winit>）。
2. 它已经开了三年、有明确诊断（搜狗忽略 `ImmSetCandidateWindow`、只认 `GetCaretPos`）却没人修，说明短期不会自愈。
3. 唯一能绕开它的路线是**不走 winit**：要么用系统 WebView（Tauri），要么用平台原生窗口层。具体候选见下表。

其余两个盲区（macOS 中文标点方向、X11+Fcitx5 预编辑）都**不足以触发换框架**：前者证据薄弱且大概率已修，后者可以用"推荐 Wayland"绕过。

### 三个风险上的对照

> 以下逐条核实自各项目官方仓库源码 / CHANGELOG / issue tracker / 官方文档，查证日期同为 2026-08-31。

| | **iced** 0.14.0<br>(2025-12-07, ⭐31.4k) | **Slint** 1.17.1<br>(2026-07-07, ⭐23.6k) | **GPUI** (Zed)<br>crates.io 0.2.2 停在 2025-10-22, ⭐89k | **Xilem** 0.4.0<br>(2025-10-29, ⭐5.5k) | **Tauri** 2.11.5<br>(2026-07-01, ⭐110k) |
|---|---|---|---|---|---|
| **底层窗口** | **winit**（`iced_winit`） | **winit**（默认）/ **Qt**（可选） | 自研：macOS AppKit、Win IMM32、X11 XIM、Wayland `zwp_text_input_v3` | **winit**（`masonry_winit`） | 系统 WebView（WKWebView / WebView2 / WebKitGTK） |
| **能否绕开 winit#2780（搜狗）** | ❌ 同一条 winit 通路 | ✅ **可以**：换 Qt 后端；而且 Slint 自己的同名 issue [#2658](https://github.com/slint-ui/slint/issues/2658) 已于 2023-06-05 **关闭** | ✅ 走 IMM32，不经 winit | ❌ 同一条 winit 通路 | ✅ 走 WebView |
| **内联预编辑** | ⚠️ **伪内联**：shell 层画一个带不透明背景的覆盖层（`Preedit::draw`），会盖住光标右侧文字 | ✅ **真内联**：`text_with_preedit()` 把 preedit 拼进显示文本并按 range 加下划线 | ✅（但要你自己实现 `EntityInputHandler`） | ✅ 真内联（Parley `set_compose`） | ❌ **Linux 上被 wry 主动关闭**：`set_enable_preedit(false)`（`src/webkitgtk/mod.rs` L421-423，注释说是为了让 fcitx 能锚定光标） |
| **最要命的未关闭 IME 缺陷** | 🔴 [#3232](https://github.com/iced-rs/iced/issues/3232) **Linux 拼音连打三个字直接 panic**（`Preedit::update` 按字节切片 UTF-8），修复 PR [#3253](https://github.com/iced-rs/iced/pull/3253)/[#3290](https://github.com/iced-rs/iced/pull/3290) **停滞 3 个月未合并** | 🟡 [#10861](https://github.com/slint-ui/slint/issues/10861) 组字中窗口失焦丢最后一个字；[#10912](https://github.com/slint-ui/slint/issues/10912) Wayland 下 40 字以上文本选中/缩放**卡死 10–15 秒** | 🔴 **12 条 `area:controls/ime` open**，另有大量 i18n open：X11 fcitx5 频繁失效（[#54959](https://github.com/zed-industries/zed/issues/54959)/[#58192](https://github.com/zed-industries/zed/issues/58192)）、Windows 中文输入间歇失灵（[#59882](https://github.com/zed-industries/zed/issues/59882)）、[#21042](https://github.com/zed-industries/zed/issues/21042) 泄漏按键给输入法**开了 21 个月** | 🟡 [#1692](https://github.com/linebender/xilem/issues/1692) 失焦时 `clear_compose()` 丢弃组字内容 | 🔴 [#11412](https://github.com/tauri-apps/tauri/issues/11412) Linux 候选窗**滚动后跑到屏幕外**（document 坐标 vs viewport 坐标），**开了 22 个月、2026-08-26 仍在更新**；🔴 [#15436](https://github.com/tauri-apps/tauri/issues/15436) Windows：**含已有文本的输入框首次聚焦会冻结 TSF**，中文输入法完全打不出字；🟡 [#15924](https://github.com/tauri-apps/tauri/issues/15924) macOS 微信输入法**吞掉第一次按键**（2026-08-27 报，Safari 同环境正常） |
| **中文字体** | ✅ cosmic-text 自动加载系统字体，`Shaping::Auto` 默认开回退。代价：[#2455](https://github.com/iced-rs/iced/issues/2455) Linux 启动扫全盘字体慢 | ✅ fontique（`system_fonts: true`）。⚠️ **软件渲染器官方文档写明 "Text rendering currently limited to western scripts"** —— 嵌入式/LinuxKMS 场景中文不可用 | ✅ macOS font-kit / Win DirectWrite / Linux cosmic-text。⚠️ macOS 不开 `font-kit` feature 则**一个字形都不渲染** | ✅ Parley + fontique（**未查到任何 CJK 实测证据**） | ✅ 浏览器引擎兜底，质量最稳 |
| **虚拟化长列表** | ❌ **没有**。维护者 hecrj 2026-08-17 亲口："Sir, I'm afraid we don't have a `list` widget."；0.14 的 `table` 控件**全量物化**（`Vec::with_capacity(cols × rows)`，每行每列都构造 Element） | ✅ 官方文档明文承诺：ListView "elements are **only instantiated if they are visible**, which **guarantees stable performance with a practically unlimited number of items**" | ✅ `uniform_list`（等高，最强）+ `list`（变高，SumTree 前缀和）。官方示例 `data_table.rs` 只到 10,000 行 | ⚠️ `VirtualScroll`，文档**自称 "minimum viable solution"** | ✅ 靠 Web 生态（TanStack Virtual 等） |
| **表格功能** | ❌ 只有 width/padding/separator/style，排序、列宽、选择、编辑全无 | ⚠️ `StandardTableView`：✅列宽拖拽 ✅斑马纹 / ⚠️**单列**排序且**只发回调不排数据** / ❌单选 / ❌无行内编辑（[#2042](https://github.com/slint-ui/slint/issues/2042) open） | ❌ **完全没有表格控件**，全部自己写 | ❌ 无表格（`grid.rs` 是布局网格不是数据表） | ✅ **唯一开箱即得完整表格**（AG Grid / TanStack Table） |
| **重绘/耗电** | ✅ 保留模式（0.14 [#2662](https://github.com/iced-rs/iced/pull/2662) Reactive Rendering） | ✅ 保留模式 + 脏属性追踪 + 软渲染局部重绘 | ✅ 保留模式（`WindowInvalidator`） | 🔴 **无脏区域，全窗口重绘**（[#789](https://github.com/linebender/xilem/issues/789) "Damage regions aren't currently implemented"）。实测 [#1562](https://github.com/linebender/xilem/issues/1562)：**光标闪烁就让四个核各涨 45%** | ✅ 浏览器合成器 |
| **官方成熟度自评** | "Iced is currently **experimental software**" | 1.x 稳定线 | "still **pre-1.0**. There will often be breaking changes" | "**alpha state**" | — |

### 排除与保留

**【推断】直接排除两个：**

- **iced** ——「不走 winit」这个唯一的换框架理由它不满足（`iced_winit` 就在 winit 上），却额外背上一个 **Linux 中文输入连打三字就 panic** 的开着的 bug，且修复 PR 停滞三个月。它在三个风险上**全面弱于 egui**：没有虚拟化列表（维护者亲口确认），`table` 控件全量物化。
- **Xilem** —— alpha；`VirtualScroll` 自称 MVP，且文档**自己列出的第一条 Caveat 就是**："if the user is typing in a text box in a virtual scroll, and scrolls down, continuing to type will stop working" —— 这正是本项目「十万行表格里编辑中文」的场景；再加上无脏区域全窗口重绘导致的高 CPU。

**【推断】真正的两个备选，各自的适用条件：**

| 备选 | 什么时候选它 | 换过去要付的代价 |
|---|---|---|
| **Slint** | **第 0 步的搜狗候选窗验证失败时的首选。** 它是唯一在两个高优先级风险上都有官方保证的方案：真内联预编辑 + 官方承诺"实际无上限行数"的虚拟化；而且它自己的搜狗定位 issue（#2658）**已经关闭**，说明这条路上有人踩过。 | 表格要从 `ListView` 自己搭（`StandardTableView` 只有单选、单列排序、无行内编辑，比 `egui_extras` 强的只有"官方承诺"）；要学 `.slint` DSL；避开软件渲染器（不支持 CJK）与 LinuxKMS（无 IME） |
| **Tauri** | **只有当目标平台不含 Linux 时才考虑。** 它是唯一「表格功能开箱即得」的路线（AG Grid 等），也是 ADR 0005 里原本就被推荐过、因包体取舍被否决的方案。 | Linux 上 IME 是硬伤：wry 主动关掉内联预编辑 + 候选窗滚动后错位（22 个月未解）；Windows 上还有"含已有文本的输入框首次聚焦冻结 TSF"这条致命 bug（#15436）。以及 ADR 0005 原本否决它的理由（包体、运行时依赖）依然成立 |

**【推断】GPUI 不推荐**：虚拟化能力确实最强（`uniform_list` + SumTree `list`），但代价是 12 条开着的 IME 缺陷 + 一堆 i18n 缺陷（其中"泄漏按键给中文输入法"开了 21 个月）+ 表格 100% 自己写 + crates.io 版本停在 0.2.2（2025-10-22，落后 main 近十个月，实际要用 git 依赖）。**它在中文输入上比 egui 更差、在表格工作量上更大。**

### 一条容易被忽略的结论

**【推断】egui 在这五个方案里，是唯一一个「上游最近半年专门为 CJK 输入法做过系统性重写、并且修复者用搜狗和微信输入法逐一实测过」的。** 对照来看：iced 的 IME 只有 9 个月历史且带着一个中文 panic；GPUI 的中文输入缺陷在持续增加而不是减少；Tauri 的三条缺陷分布在三个平台且最老的一条开了 22 个月。**这反过来强化了"留在 egui"的判断——不是因为 egui 完美，而是因为它是唯一一个能指出「谁、在什么时候、用什么输入法、在哪些平台上测过」的。**

---

## 结论与规避方案

### 判定

**egui 在三个风险上都没有硬伤，可以按 ADR 0005 推进。** 但风险一的判定是**有条件的**——条件是先做一次实机验证。

之所以敢下这个判断，最关键的一条是：中文输入的两个致命 bug（Enter 上屏多插换行 #7876、组字退格误删已有文字 #7908）在 **2026-03-24 才修好**，随 **0.35.0（2026-06-25）** 发布。**任何基于 0.34 或更早版本的中文输入体验评价都已经过时。** 而修复者在 Windows 上用**搜狗和微信输入法**逐一实测过——这是针对中国用户最有说服力的单条证据。

**【推断】一个降低风险量级的观察**：按 `CONTEXT.md` 的领域模型，**裁决**是"人工在候选之间做出选择，或者判定'都不对'"——**这主要是选择动作，不是打字动作**。真正需要中文输入的是三处：按中文标题搜索/筛选、手工补一个中文译名、录入汉化组名字。这是"搜索框 + 偶尔编辑一个短标题"的形态，不是"长时间连续组字"。**ADR 0005 把中文输入法列为"头号风险"，量级可能高估了**——它是必须验证的风险，但不是持续暴露的风险。这也直接支持了 3.5 节的建议：把中文输入集中到详情面板的少数几个输入框里，风险面就收窄成了几个固定位置的 `TextEdit`。

### 规避方案

**第 0 步（写第一行 GUI 代码之前，半天）——实机验证五件事**

跑官方 demo（`cargo run -p egui_demo_app --release`，锁 egui ≥ 0.35，建议 0.36.1），在 TextEdit demo 里逐条验：

| 要验的 | 对应未关缺陷 | 通过标准 |
|---|---|---|
| macOS 内置拼音 + 搜狗/微信输入法 | winit#3814（中文标点方向反转） | 输 `，。、《》` 出中文标点 |
| Windows 搜狗输入法的**候选词窗位置** | winit#2780 | 候选窗跟随光标，不掉到屏幕底部 |
| 三平台：Enter 上屏不多插换行、组字退格不误删 | egui#7876 / #7908（应已修） | 行为与记事本/TextEdit.app 一致 |
| macOS：组字过程中按 Cmd+Q 关窗 | winit#4626 | 不 abort |
| macOS：用 fn/🌐 字符检视器插 `★ ♪ Ⅲ` | winit#3342 / egui#2359 | **预期会失败**（三年未修）。确认后在应用里自己做个符号选择器，或明确告知用户走粘贴 |

**如果第 2 项（搜狗候选窗）挂了**——这是唯一可能真正卡住的一项，因为它在 **winit 层**且开了三年。届时的降级顺序是：

1. 确认换微信输入法 / 微软拼音是否正常。若正常，作为「已知限制」记入文档即可（winit#2780 的评论里 @csmoe 的判断是"其他中文输入法与 winit 配合无碍"）。
2. 若搜狗必须支持，则换 GUI 层。**注意换 `iced` 无效**（同样跑在 winit 上）；有效的只有**不走 winit** 的方案——按附加二的结论，首选 **Slint**（它自己的同名 issue #2658 已于 2023-06-05 关闭，且有 Qt 后端作为第二条通路）。核心库不动。

**第 1 步——版本与配置基线**

```toml
egui   = "0.36"   # 必须 ≥ 0.35，0.34 及以下中文输入体验不可接受
eframe = "0.36"
egui_extras = { version = "0.36", features = ["..."] }
```

- macOS 退出前、窗口还活着时调 `set_ime_allowed(false)`（winit#4626 报告者给出的规避，其原文称 20/20 次运行有效；**这是 issue 报告者的验证，不是 winit 官方声明**）
- Windows 上视第 0 步结果决定是否 `visuals.ime_composition.legacy_visuals = false`（默认 `true`，是为韩语打的补丁，中文场景可能可以关掉换取更好的预编辑观感）
- Linux 首选 Wayland + IBus；文档里注明 X11 + Fcitx5 的预编辑显示位置异常（egui#7975，不影响输入正确性）
- **做一个内置符号选择器**（★ ☆ ♪ ♥ Ⅰ–Ⅹ ① ② ③ → ・ 等一二十个字符的小面板），绕开 macOS 字符检视器的失效（winit#3342）。这是几十行代码，且对三个平台的用户都是效率提升——ROM 标题里这些符号出现频率不低，让用户去系统面板里翻本来就慢

**第 2 步——字体：打包子集，不要走系统字体**

- 用 `NotoSansSC[wght].ttf` 做子集，字符集取 **GBK + Big5 + CJK 标点 + 假名 + U+2100–27BF 符号区**（覆盖 Ⅲ、①、★、♪、→），固定 `wght=400` → **实测 7.4 MB**
- `include_bytes!` 静态嵌入，`FontData::from_static`，插到 `FontFamily::Proportional` 的 **index 0**（顺便把 ★ ♪ 的渲染从 emoji 图标字体夺回来，风格统一）
- **不要走系统字体**：macOS 上苹方在哈希路径下且 74.6 MB，系统字体方案在内存（20–75 MB RSS）和可靠性上都劣于 7.4 MB 的静态子集。Servo 自己的 macOS 分支就是个 TODO。
- 上线前跑一遍覆盖检查：拿真实的 ROM 标题库跑一遍 `cmap` 命中率，缺字直接暴露

**第 3 步——表格：接受要自己写 800–1500 行**

- **等高行 + `TableBody::rows()`**，绝不用 `heterogeneous_rows`（后者 O(总行数)）。十万行 × 21 px ≈ 2.1×10⁶ px，f32 ULP = 0.25 px，距离抖动阈值有一个数量级以上余量（egui#1391，推算见 3.2）
- **需要冻结列 / 多级表头时**才换 `egui_table` 0.10（Rerun 出品）。注意它**没有**解决 f32 滚动精度问题（源码仍全 f32），换它是为了冻结列和 O(1) 变高行，不是为了更高的行数上限。元数据表字段多、需要左侧固定「作品/发行版」列横向滚动——**这个需求值得早点判断，因为换表格库比换 GUI 框架便宜得多，但比一开始就选对贵**
- **排序在数据层做**：点表头 → 对底层 `Vec` 重排 → `rows()` 用 `row_index` 索引已排序数组。渲染层零成本
- **多选自己写**：官方 demo 只给了朴素 toggle，Shift 范围选和 Ctrl 加选要自己实现（记 `anchor_index` + `HashSet`）
- **中文输入放详情面板，不放单元格**：避免"组字中行被滚出视口 → widget 消失 → Commit misrouted"（见 3.5 的推断）。选中行 → 下方固定详情面板编辑。这条同时改善了可用性
- **不做表格缩放动画**、不让字号随滚动变化（egui#8093，连续变字号会让字形缓存 100% miss，440 µs/帧 vs 80 ns）
- **保持 Reactive 模式**：不要在 `update()` 末尾无脑 `request_repaint()`。后台扫描线程算完一批再 `ctx.request_repaint()`；需要定时刷新用 `request_repaint_after()`

**第 4 步——把第 0 步的清单留下来**

egui 现在锁的是 winit 0.30.13，而 winit 已有 **0.31.0-beta.2**，且带 IME API 的破坏性改动。**egui 将来跟进 winit 0.31 时必须重跑第 0 步的五项验证**——把那张表存成仓库里的一份 checklist（比如 `docs/ime-acceptance.md`），别只留在这篇调研里。同理，每次升 egui 大版本也跑一遍。

**第 5 步——退出路线保持敞开**

ADR 0005 已经把核心做成独立 lib + CLI，这是这次选型里最正确的一个决定。GUI 层的所有代码应当只依赖核心库的公开 API，不要让 egui 类型渗进领域模型。这样即便第 0 步验证失败或将来撞上不可绕过的 IME 缺陷，换壳的代价是有界的——而按附加二的结论，换的目标是 **Slint**（或在不含 Linux 的前提下回到 Tauri），**不是 iced**。
