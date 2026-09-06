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
      —— 四条各自的收尾（`-j 1`，`--test-threads=2`，一趟全量）：
      1. `cargo fmt --all --check` → exit 0，**零输出**（格式化前是 exit 1 / 401 条 `Diff in`）
      2. `cargo clippy --workspace --all-targets --all-features -j 1` →
         `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 25.21s`，0 warning / 0 error
      3. `cargo test --workspace --all-features -j 1 --no-run` → exit 0，56 个测试二进制全编出来
      4. `cargo test --workspace --all-features -j 1 -- --test-threads=2` → exit 0，
         **58 个 `test result: ok`、1,581 条通过、0 失败**
