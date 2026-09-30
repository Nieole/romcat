# 06: 一条测试守住「没有写死的颜色」

**What to build:** 一条测试逐个读界面源码（剔除测试模块与令牌装配那一处），见颜色字面量或具名颜色常量就红，指出文件与行；白名单只放透明与占位色。
先把已经滑出来的三处写死白色（库体检两处平台标、一处开关圆点）换成令牌——强色底上的字那一格；换不了的新立令牌。暗色主题下它们从此跟着令牌走。

规格：`../spec.md`；已裁定的形状与核查见 `../grill.md`。

收挂单 `Q486`：见 `../grill.md` 里那一条。

**Blocked by:** None (can start immediately)

**Status:** done

- [x] 先写这条测试，看它在今天 `main` 上红、指出那三处
  - 证据：`crates/gui/tests/colors.rs` 的 `界面源码里没有写死的颜色`（另三条是扫描器自测：`具名颜色与颜色构造都抓_只放行透明与占位色`、
    `注释字符串与测试模块里的不算_测试模块之后的照抓`、`测试专用的字段与分支只抹它自己_整份测试文件不扫`）。
  - 在基点 `1550645` 的界面源码上红，报 **5 处**，不止票面的三处：`crates/gui/src/browse.rs:199`、`:208`（浏览屏卡面上的平台标，票面漏数）、
    `crates/gui/src/health.rs:1309`、`:1321`（库体检平台标）、`crates/gui/src/look.rs:1071`（开关圆点）。`look.rs:2431` / `:2433` 那两个
    `Color32::from_rgb` 在测试模块里（哨兵），被剔除，没报。日志 `/Users/nicoer/dev/game-wt/logs/q6-gt06-red-on-base.log`。
- [x] 三处换成令牌后测试绿
  - 证据：五处都换成令牌，`colors.rs` 4 条全绿。**没有一处取 `on-accent`**：稿上这三条规则两套主题都写死白、暗色不另写（`.hplat{color:#fff}` `prototype.html:520`、
    `.cv-plat{color:#fff}` `:485`、`.switch i::after{background:#fff}` `:765`），换不了，照票「换不了的新立令牌」新立两格，值照稿：
    `[color.platform-badge] ink = "#FFFFFF"`（平台标上的字）与 `[color.switch] knob = "#FFFFFF"`（开关圆点），都是两套主题共用。
    `on-accent` 暗色那一格是 `#0D1030`：压在平台色上对比度 MD 1.38、PSP 1.84（白字 13.37 / 10.06），压在暗色关着那一档的 `line-2` 上 1.62——
    平台标那一处与先例 `Q894` / `Q924` 相悖，记挂单 `Q1236` 交拿主意的人裁（见下一格）；开关圆点只有一条路站得住，写在提交说明里。
  - 作品页那枚平台标（`crates/gui/src/browse/work.rs` 的 `platform_chip`，同一条稿上规则 `.hplat`）从 `on-accent` 一并改取 `platform-badge.ink`（拿主意的人裁，见下一格）。
  - 新令牌进了设计稿核对脚本：三条核对（`.hplat` 与 `.cv-plat` 的 color、`.switch i::after` 的 background），不进豁免表。
    `python3 .scratch/gui-looks-like-the-design/check_tokens.py` → `EXIT=1`，只红基点就有的那两行（`Q1246` size-title、`Q1247` 视频播放标 shade），没多红一行、没报漏核；
    拿一份把两格改成 `#0D1030` / `#EEEEEE` 的令牌副本跑，多出三行「平台标上的字 platform-badge.ink：设计稿 .hplat 的 color 是 #fff，令牌是 #0D1030」等。
  - `tokens.rs` 的 `内置令牌读得出来` 钉住两格新令牌等于原来写死的 `Color32::WHITE`（搬家不许变色）。
- [x] 变异实测：临时写一个颜色字面量 → 测试红并指出文件与行（写进证据）
  - 证据：把 `crates/gui/src/browse/work.rs:2674` 的 `egui::Color32::TRANSPARENT` 临时改成 `egui::Color32::from_gray(40)`（子目录里的文件，顺带验递归）→
    `界面源码里没有写死的颜色` 红，印「界面源码里写死了 1 处颜色……\n  crates/gui/src/browse/work.rs:2674  Color32::from_gray    ← egui::Color32::from_gray(40)」，
    其余三条绿；改回之后 `git status` 里没有它。日志 `/Users/nicoer/dev/game-wt/logs/q6-gt06-mutation.log`（审查改过扫描器之后重做的那一趟）。
- [x] 暗色主题下那三处的截图基线重批，拿主意的人点头
  - 证据：拿主意的人 2026-09-30 裁甲，作品页一并改（挂单 `Q1236`）：库体检与浏览屏卡面的平台标字、开关圆点照稿两套主题都取白（新令牌 `platform-badge.ink`、`switch.knob`，
    值与原来写死的白逐位相同，这几处的基线逐像素不变——`UPDATE_SNAPSHOTS=1` 重批后一张 PNG 都没动）；作品页 `platform_chip` 从 `on-accent` 改取 `platform-badge.ink`，
    重批的是六张**暗色**基线：`work/overview-dark`、`work/titles-dark`、`work/metadata-dark`、`work/evidence-dark`、`work/variants-dark`、`merge/split-dark`
    （同一处平台标，各 116 个像素，矩形 (427,84)–(447,92)；`merge/split-dark` 是拆分那一层压在作品页上、遮罩底下透出同一枚；`work/media-*` 头上不画平台标，没变），亮色逐像素不变。
    2026-09-30 当面批六张暗色基线（清单见上）。变之前的另存与三倍并排图：`/Users/nicoer/dev/game-wt/logs/q6-gt06-before/`、`/Users/nicoer/dev/game-wt/logs/q6-gt06-work-page/compare/`。
  - 票面「暗色主题下那三处」预期的是换成 `on-accent` 之后暗色会变；照稿取白之后那三处逐像素不变，这一格实际批的是作品页那六张。
- [x] 收尾时照 `docs/agents/long-jobs.md` 跑一趟真门禁，回执写进票里当证据
  - 证据：`/Users/nicoer/dev/game-wt/logs/q6-gt06-gate1.log`，开头两行 `/Users/nicoer/dev/game-wt/slot-2` 与 `q6/gt-06-no-hard-coded-colours-in-the-gui`，最后一行 `EXIT=0`
    （`TMPDIR=/Users/nicoer/dev/game-wt/cs/tmp cargo xtask gate --keep-going -j 3 --test-threads 3`，跑在翻 `Status` 之前）。汇总表：
    ```
    绿  fmt        1s  cargo fmt --all --check
    绿  glossary    0s  cargo xtask glossary
    绿  check     24s  cargo check --workspace --all-targets -j 3
    绿  clippy    27s  cargo clippy --workspace --all-targets --all-features -j 3
    绿  test     987s  cargo test --workspace --all-features --no-fail-fast -j 3 -- --test-threads=3
    绿  numbers    4s  cargo xtask numbers --check
    绿  doc       14s  RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --lib --bins -j 3
    7 条全绿。
    ```
    `test` 那一步 87 个测试二进制各一行 `test result: ok`，没有 `FAILED`。之前先跑了 `cargo xtask numbers --write`（`/Users/nicoer/dev/game-wt/logs/q6-gt06-numbers.log`，`EXIT=0`）：
    README 的测试条数 2,628 → 2,632、测试目标数 86 → 87。`glossary` 这一趟扫的是未提交的改动（分支还没有自己的提交），6 份 `.rs` 新写的 527 行，没撞上。

## 收尾（2026-09-30）

- **收挂单 `Q486`**（`../grill.md`，第五轮收口已标 settled）：照裁定做了——`crates/gui/tests/colors.rs` 逐个读界面源码（剔除 `#[cfg(test)]` 管着的那一段与 `tokens.rs`），
  见颜色字面量或具名颜色常量就红、指出文件与行，白名单只放 `Color32::TRANSPARENT` 与 `Color32::PLACEHOLDER`；写死的白色全换成令牌（五处，外加作品页那一处 `on-accent`）。
  grill 那句「tokens.toml 已有 on-accent」的前提不成立：稿上这几处不是 `on-accent` 那一档，新立了两格（见上，`Q1236`）。
- **扫描器认不出的**（写在 `colors.rs` 文件头与 `抹掉测试代码` 的文档上）：给 `Color32` 起别名、结构体字面量 `Hsva { h, s, v, a }` 会漏报，那一层靠审查；
  `cfg(all(test, …))` 与 `#[cfg(test)] mod tests;` 指向的外置文件会多报（当场看得见）。
- **不是字面量、这条测试按裁定不管的**：`browse.rs` 卡面中文标与选择框拿 `window_fill` 乘一个写死的比例，也不照稿，记挂单 `Q1237`。
- **挂单**：`Q1236`（settled，拿主意的人 2026-09-30 裁甲）、`Q1237`（open，拿主意的人）。`Q924` 原文末尾追加了改裁一行。
- **顺手**：`browse.rs` 里复述的真库数（四处）换成量级词、指台账（「收尾一张票」的约定）；`tokens.rs` 里 `VideoMark` 那句「`check_tokens.py` 不核它们」改成实话（`Q1248` 说的那一句，状态由编排者合并时处理）。
