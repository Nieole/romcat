# 11 — 整仓库格式化一次，`cargo fmt --check` 进门禁

**What to build:** 拿票 10 定好的配置把整个仓库格式化一次，并把 `cargo fmt --check` 加进门禁，从此机器管格式。

**开工之前先确认没有未合并的分支。** 这一票一落地，**所有在飞的改动都会冲突**——同期另一条线（worktree agent 那批）的提交就是这么来的。这也是它排在这一批最后的理由。

格式化那一次**单独一条提交**，门禁改动**另一条**，两者不合并——那一大堆 diff 要有自己的提交可查，不混进任何一张票的历史。

**Blocked by:** 10

**Status:** done

- [x] 开工前确认没有未合并的分支在飞
      —— `git status --short` 空；`git branch -a` 只有 `main` 与 `remotes/origin/main`
      （没有第二条本地分支、没有第二个远端分支）；`git worktree list` 只有主工作树一行。
      本地 `main` 领先 `origin/main` 19 条，但那是**已合进 `main` 的历史**、不是在飞的改动，
      推不推是用户的事（本票不 push）。
- [x] 整仓库格式化，**单独一条提交**
      —— `cargo fmt --all` 一次，动 **77 个文件、+1,484 / −768，共 2,252 行**，
      与票 `10` 估的 **77 / 2,252 分毫不差**。那条提交里**只有 `.rs`**（`git show --stat` 可查）。
      幂等：跑完 `cargo fmt --all --check` 当场 exit 0。
- [x] `cargo fmt --check` 进门禁，**另一条提交**
      —— 门禁从三条变**四条**，`cargo fmt --all --check` 排在**第一行**（最便宜、最先红）。
      这个仓库的「门禁」是 `README.md`「开发」一节那份约定，没有 CI 也没有 git hook
      （实测 0 个 `.yml`/`.yaml`、`.git/hooks` 全是 `.sample`），所以「加进门禁」能做的
      就是改那份约定——这一点连同「它仍然靠人自觉」记在挂单 `Q187`，交给拿主意的人。
      同一条提交里还落了 `.gitattributes`（`* text=auto eol=lf` + `*.ttf binary`），
      堵 `rustfmt` 拦不住的那一手 CRLF（挂单 `Q184`）。
- [x] README 的开发那一节跟着改
      —— 「⚠️ 眼下别跑 `cargo fmt --all`」整节换成「排版归机器管」：提交前跑什么、
      配置在哪、别凭手感调、两件反直觉的（按显示列宽算 / 中文注释不会被重折）。
      `rustfmt.toml` 头一段那条同样的告示也按它自己写的「格式化落地后连这段一起删」删掉了。
      顺手把三处过期的测试条数按实测改准（挂单 `Q185`，那一节错的不只是数，见 `Q186`）。
- [x] 格式化之后全量测试仍然全绿（格式化不该改变任何行为）
      —— **58 个 `test result: ok`、1,581 条通过、0 失败**，与交接的基线**逐字相同**。
      另外两道独立佐证，专门用来证「这 2,252 行里没有一处是行为改动」：
      ① **逐字节复现**：`git archive HEAD | tar -x` 出一份丢弃副本，在副本上跑一遍
      `cargo fmt --all`，与工作区**逐个比 196 个 `.rs`，不同 0 个**——也就是说这条提交
      就是一趟纯 `cargo fmt --all` 的产出，没夹带任何人手改动；
      ② **`clippy` 零告警**（`--all-targets --all-features`）。
      顺带把挂单 `Q183` 那处真的排版错（`Ok(out)    }` 挤在同一行）自己修掉了：
      `grep -rnP '\S {2,}\}\s*$' crates --include='*.rs'` 从 1 处变 0 处。
- [x] 门禁全绿（`--all-features`）
      —— 五条命令的收尾（`-j 1`，`--test-threads=2`，一趟全量；审查回执之后重跑了一遍，数不变）：
      1. `cargo fmt --all --check` → exit 0，**零输出**（格式化前是 exit 1 / 401 条 `Diff in`）
      2. `cargo clippy --workspace --all-targets --all-features -j 1` →
         `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1.46s`，0 warning / 0 error
      3. `cargo test --workspace --all-features -j 1 --no-run` → exit 0，56 个测试二进制全编出来
      4. `cargo test --workspace --all-features -j 1 -- --test-threads=2` → exit 0，
         **58 个 `test result: ok`、1,581 条通过、0 失败**
      5. `cargo doc --workspace --no-deps -j 1` → exit 0（README 那个命令块里的第四条）。
         **带保留**：它退出码是 0 但有 **68 条既有的 rustdoc 告警**（`clippy` 那条明写「零警告」，
         `cargo doc` 从来没写过）。与格式化无关，判据是格式化提交里**文档注释被改动 0 行**——
         记在挂单 `Q189`，本票不修。


## 审查回执

`/code-review`（`--effort high`）把本票的两件关键事**独立复现了**：

- **那条 2,252 行的提交是一趟纯 `cargo fmt --all` 的产出**——它自己 `git archive HEAD~1`
  出一份丢弃副本跑了一遍 `cargo fmt --all`，与 `HEAD` 逐字节比 **196 个 `.rs`，不同 0 个**；
  提交里**只有 `.rs`**；`#[rustfmt::skip]` 的条数**前后都是 1**（那处既有的
  `crates/gui/src/layout.rs` 的 `ALL`），也就是说 `Q182` 定的「接受摊开」真的兑现了，
  没有偷偷开豁免；`cargo fmt --all --check` 在 `HEAD` 上 exit 0（幂等）。
  它另外核过：diff 里剩下的「token 层面差异」全是 `rustfmt` 的折行与 `use` 重排，
  而所有 glob 导入都是测试模块里的 `use super::*;` 惯用法，重排不可能改变遮蔽关系。
- **门禁那条真的把 `cargo fmt --all --check` 加上了**（`README.md` 开发块第一行）；
  `.gitattributes` 也独立核过：`git ls-files --eol` 数出 326 个文本文件全是 `i/lf w/lf`、
  那个字体正确地是 `-text`，**它确实不重整任何既有文件**。

它报了九条，全在门禁/文档那条提交里（格式化那条零条）。**落地七条，转挂单两条：**

| # | 它报的 | 处置 |
|---|---|---|
| 1 | `.scratch/queue-followups/spec.md` 的门禁块还是三条，没加 `cargo fmt --all --check` | **改了。** 加上第一条，并注明「权威的是 README 那一节，那里还多一条 `cargo doc`」。这条它抓得对：`Q187` 自己点名 spec 是门禁写在哪的两处之一，却没顺手改它。 |
| 2 | 没有 `rust-toolchain.toml`，`cargo fmt --check` 上了门禁却没钉住是哪一版 `rustfmt` | **转挂单 `Q188`。** 风险是实的（跨版本产出会变，`style_edition` 钉不住实现改动），但钉工具链是仓库级决定、有不止一个方向，还会改掉 `rustup` 的行为，不该由一张格式化票顺手定。 |
| 3 | 同一份 spec 里还留着「静默跳过 22 条」——正是本票开 `Q186` 说它错的那句 | **改了**，按实测重写（101 条 / 7 个文件整份不编译 / 退出码照样 0）。 |
| 4 | `crates/gui/Cargo.toml` 自己打架：一处写「那六个测试」，两行后写「这七个测试」，实际 7 个 `required-features` | **改了。** 这处最该改——它是 `demo` feature 的**源头**，本票改 README 那三个数正是为了这件事，却漏了源头。顺手把「静默跳过」也按 `Q186` 改成「整份不编译」。 |
| 5 | 验收里「门禁四条」列的不是 README 那四条，`cargo doc` 没跑过 | **跑了**，结果补进上面第 6 条验收，并如实标了带保留（68 条既有 rustdoc 告警 → `Q189`）。 |
| 6 | `.gitattributes` 的 `text=auto` 靠 NUL 启发式认二进制，将来签进来的 ROM 夹具可能被当文本改写 | **加了一段显式叮嘱**（往仓库加二进制夹具时补一行 `binary`），并把「眼下 0 个签进来的二进制夹具」这个前提写在旁边。没预先列一堆猜的扩展名——那是替将来的人猜。 |
| 7 | `eol=lf` 是无条件的，将来加 Windows 批处理会栽（`cmd.exe` 读不好只有 LF 的 `.bat`） | **占了位子**：补 `*.bat` / `*.cmd` → `eol=crlf`。这是标准做法，不是猜。 |
| 8 | `README.md` 第 97 行「27/29 张票落地，余下两张」已经不是全仓库的状态 | **改了。** 复核过：四条队列 63 张，59 张 `done`，余 4 张（3 张 `ready-for-human` + 1 张 `ready-for-agent`）。原句只对 `rom-metadata-automation` 那一条队列成立，而标题是「现在到哪一步了」、没点队列名。 |
| 9 | spec 第 26 行的问题陈述还写着「门禁里也没有 `cargo fmt`」 | **补了一句括注**说明票 10 + 票 11 已经落完，原句留着——那是当时的问题陈述，改掉就没有对照了。 |

审查后重跑了全套：`fmt --check` exit 0 零输出、`clippy` 零告警、`test --no-run` exit 0、
一趟全量仍是 **58 个 `test result: ok`、1,581 条通过、0 失败**、`cargo doc` exit 0。
**这一轮改的全是 `.md` / `.toml` / `.gitattributes`，一个 `.rs` 都没动**，
所以那条格式化提交仍然是逐字节可复现的纯 fmt 产出。

## 挂单裁决（第三轮）

本票记下的两条，由票 `parking-3/01`（「门禁真的会拦人」）落地，均 **settled**。

| 挂单 | 它说的 | 裁决 |
|---|---|---|
| `Q187` | 「门禁」没有任何机器执行的地方——四条命令只写在 `README.md` 的开发一节里，漏跑了没有人拦 | **settled。** 四条落成仓库里一个真实可跑的子命令 `cargo xtask gate`（`xtask/src/gate.rs` 的 `steps()` 是**唯一**定义，`.cargo/config.toml` 给别名）；`.github/workflows/gate.yml` 只调这一句、一条门禁命令都不抄；`README.md` 的门禁块改成指向它。「它跑哪四条」由 `cargo xtask gate --list` 当场答，不再有第二份会漂的清单。**红得动这件事有测试钉着**：`xtask/tests/gate.rs` 的 `排版不干净就红改回来就绿` 拿一份丢弃 crate 真的起子进程验红绿。 |
| `Q188` | 没有任何东西钉住是哪一版 `rustfmt`——换台机器提交就会多出一大片无关重排 | **settled。** 仓库根落 `rust-toolchain.toml`，`channel = "1.98.1"`（落它时实测 `rustfmt 1.9.0-stable`）、`components = ["rustfmt", "clippy"]`、`profile = "minimal"`。本票当年转挂单时说的顾虑（「钉工具链是仓库级决定、有不止一个方向，还会改掉 `rustup` 的行为」）在那份文件里逐条写明并认下了：rustup 从此在这个目录下只认这一版，机器上没有它时第一次进目录会触发一次下载。同时写清它与 `Cargo.toml` 的 `rust-version = "1.95"` **并存、互不替代**——前者是给 cargo 解析器的 MSRV 下界，钉不住任何一版。 |

**顺带更正本票的一处记录：** 本票收尾写的「**58 个 `test result: ok`、1,581 条通过**」，
到 `parking-3/01` 落门禁时实测已是 **66 个 `test result: ok`、1,635 条通过**
（`cargo xtask gate --throttle`，四条全绿）。原句不改——那是当时的实测，改掉就没有对照了。
