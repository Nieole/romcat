//! 门禁那条子命令的钉子。
//!
//! 这份文件只钉下面这几件事，别的都不该在这儿验：
//!
//! - **fmt 那一条真的会拦人。** 给它一份故意不 fmt-clean 的输入，它红；
//!   跑一趟 `cargo fmt --all` 改回来，它绿。这条走的是**真的子进程**——
//!   门禁的价值全在「它真的会退非零」上，断言参数串对不对是验不到这件事的。
//! - **doc 那一条真的会拦人。** 同样的两面验法：给它一份文档里断了一条链接的输入，
//!   它红；把那条链接改成不带链接的写法，它绿。这一条是票 `parking-3/02` 的题目本身
//!   ——在那张票之前 `cargo doc` 那一条退出码**永远**是 0，「跑绿」只在字面上成立。
//! - **定义只有一份。** 门禁每一条的名字与形态从 `gate::steps` 一处来，
//!   README 与 CI 都只指向它。
//! - **词表那一条扫的是哪几行。** 相对 `main` 的 merge base、含未提交的、存量不扫，
//!   拿不到历史时如实跳过；撞了真的退非零。「哪些词元算撞、注释里不算」那一半是纯函数，
//!   单元测试在 `xtask/src/glossary.rs` 里，不在这儿。
//! - **那几个算得出来的数过期时真的会红。** 造一份数写错了的丢弃仓库，`--check` 退非零
//!   **并把对的那个数印出来**；`--write` 改完，同一份输入就绿。「标记怎么扫、四样各怎么数」
//!   那一半是纯函数，单元测试在 `xtask/src/numbers.rs` 里，不在这儿。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use xtask::gate::{Limits, Step, steps};
use xtask::glossary::{self, Scope};

// ── 一份用完即删的丢弃 crate ──
//
// 落在系统临时目录里，因此**不在本仓库的 workspace 里**，也读不到仓库根的
// `rustfmt.toml`。这条测试要验的是「门禁这一步会不会红」，不是那份配置。

static 序号: AtomicU64 = AtomicU64::new(0);

/// 一个用完即删的临时目录。
struct 丢弃目录 {
    path: PathBuf,
}

impl Drop for 丢弃目录 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 在系统临时目录里建一个空目录，名字带上 `tag`、进程号与序号，几条测试并行跑也撞不上。
///
/// 交出的是路径，由调用方包进 [`丢弃目录`]——包好之前出了事，目录顶多留在临时目录里。
fn 丢弃路径(tag: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let unique = 序号.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!(
        "romcat-xtask-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("能建临时目录");
    path
}

/// 造一份最小的独立 crate，只有 `src/lib.rs`。
fn 丢弃crate(tag: &str, lib: &str) -> 丢弃目录 {
    丢弃crate带同名bin(tag, lib, None)
}

/// 同上，但可以再给一份 `src/main.rs`——**那个 bin 与 lib 同名**（都随包名）。
///
/// ⭐ 同名是**故意的**，它正是这里要复现的那件事：`cargo doc` 默认跳过与 lib 同名的
/// bin（两者都往 `target/doc/<crate>/index.html` 里写），于是那份 `main.rs` 里的
/// 文档链接一个字都没被读过。本仓库 `romcat-gui` 与 `xtask` 都长这个样。
fn 丢弃crate带同名bin(tag: &str, lib: &str, main: Option<&str>) -> 丢弃目录 {
    let path = 丢弃路径(tag);
    fs::create_dir_all(path.join("src")).expect("能建临时目录");
    fs::write(
        path.join("Cargo.toml"),
        // `[workspace]` 那一行是**封口用的**，不是摆设：cargo 会从这个临时目录一路往上找
        // workspace 根，而 `TMPDIR` 被指进某个 cargo workspace 里头并不罕见（容器 / CI）。
        // 少了它，`cargo fmt` 会报「current package believes it's in a workspace when it's not」
        // 而红——红的原因跟门禁毫无关系，正是这条测试最该躲开的那种假红。
        // 包名用 **ASCII**：一个 CJK 的 crate 名单独当 lib 编得过（`rustc` 校验用的是
        // `char::is_alphanumeric`），可一旦这个包里还有个 bin 要链它，cargo 递下去的
        // `--extern 丢弃` 就会当场炸「not a valid ASCII identifier」——而下面
        // `与_lib_同名的那个_bin_里断一条链接照样红` 造的正是那种 lib + 同名 bin 的包。
        // 红的原因与门禁无关，是这份夹具最该躲开的那种假红（同 `[workspace]` 那一行）。
        "[package]\nname = \"throwaway\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .expect("能写 Cargo.toml");
    fs::write(path.join("src/lib.rs"), lib).expect("能写 lib.rs");
    if let Some(main) = main {
        fs::write(path.join("src/main.rs"), main).expect("能写 main.rs");
    }
    丢弃目录 { path }
}

/// 门禁里名叫 `name` 的那一条，在 `dir` 上跑出来的退出码是不是 0。
///
/// 走的是**真的子进程**：门禁的全部价值在「它真的会退非零」上，
/// 断言参数串长什么样是验不到这件事的。
fn 这一条绿吗(name: &str, dir: &Path) -> bool {
    let 门禁 = steps(Limits::default());
    let 它 = 门禁
        .iter()
        .find(|it| it.name == name)
        .expect("门禁里有这一条");
    它.command_in(dir)
        .status()
        .expect("能起 cargo 子进程")
        .success()
}

#[test]
fn 排版不干净就红改回来就绿() {
    // ⭐ **门禁的全部价值在这一条上。** 四条命令写在 README 里的时候，「它会不会红」
    // 靠的是人记得跑；写成子命令之后这件事才第一次有东西钉着。
    let dir = 丢弃crate("fmt-red", "pub fn 加(a:i32,b:i32)->i32{a+b}\n");

    assert!(
        !这一条绿吗("fmt", &dir.path),
        "参数挤在一起、大括号贴着函数体的源码，`cargo fmt --all --check` 必须退非零"
    );

    let 改回来 = Command::new(xtask::gate::cargo())
        .arg("fmt")
        .arg("--all")
        .current_dir(&dir.path)
        .status()
        .expect("能起 cargo fmt");
    assert!(改回来.success(), "`cargo fmt --all` 本身得跑得通");

    assert!(
        这一条绿吗("fmt", &dir.path),
        "同一份输入 fmt 过之后，门禁第一条必须绿——否则它红的不是排版，是别的什么"
    );
}

#[test]
fn 门禁就是这几条而且顺序是从便宜到贵() {
    // ⭐ **顺序不是随手排的**：fmt 两秒就出结果，`test` 要几分钟。默认在第一处红上就停，
    // 于是「忘了跑 fmt」这种最常见的红，代价是两秒而不是一整趟编译。`glossary` 紧跟在
    // `fmt` 后面：它跑几条 git、读几份改过的文件，也是秒级。
    let 门禁 = steps(Limits::default());
    let 名字: Vec<&str> = 门禁.iter().map(|it| it.name).collect();
    // `numbers` 是唯一不按这条排的：它要的那份编译缓存由 `test` 热着，所以它跟在 `test` 后面。
    assert_eq!(
        名字,
        [
            "fmt", "glossary", "check", "clippy", "test", "numbers", "doc"
        ]
    );
}

#[test]
fn 交付出去的那份配置还有一条在看着() {
    // ⭐ **这一条是票 `parking-3/02` 给 `doc` 补 `--all-features` 时接住的那一格。**
    // `clippy` / `test` / `doc` 三条都带 `--all-features`，于是它们编的都是**开着 `demo`**
    // 的那份代码；而 `cargo build --release` 交付出去的是 `demo` 关掉的那份。
    // 少了下面这一条，「交付的那份还编不编得过」在门禁里一个把门的都没有：
    // 谁在 `crates/gui/src/` 里写下一处只有开着 `demo` 才编得过的引用，门禁全绿，
    // 而 `cargo build --release` 当场编不过。
    //
    // 按 `--workspace` 过滤不是多余的：`fmt` 走 `--all`，`glossary` 压根不编译，
    // 两条都不带 `--all-features`，但它们也都不是在编「交付的那份」。
    let 门禁 = steps(Limits::default());
    let 盖默认特性的: Vec<&str> = 门禁
        .iter()
        .filter(|it| !it.args.iter().any(|a| a == "--all-features"))
        .filter(|it| it.args.iter().any(|a| a == "--workspace"))
        .map(|it| it.name)
        .collect();
    assert_eq!(
        盖默认特性的,
        ["check"],
        "整个工作区在**默认特性**下编不编得过，得有且只有一条门禁看着"
    );
}

#[test]
fn 每一条打印出来对得上_readme_那一段() {
    // ⭐ 这条钉的是**定义只有一份**：README 的门禁块从此只写 `cargo xtask gate`，
    // 「它到底跑什么」由 `cargo xtask gate --list` 当场打印，不再有第二份手抄的
    // 清单在旁边慢慢漂。下面这几行字面量是手写的**独立比对基准**，不是从 `steps`
    // 反推出来的：其中三行（`fmt` / `clippy` / `test`）原样抄自门禁落地之前 README
    // 的开发块，两行是票 `parking-3/02` 添的——`check` 那一条，以及敲成硬红、
    // 补上 `--all-features` 之后的 `doc`；`cargo xtask glossary` 那一行是票
    // `machine-checks-premises/02` 添的，`cargo xtask numbers --check` 那一行是票
    // `machine-checks-premises/03` 添的——两条都是递归的 xtask 子命令，仍然是 cargo 子进程。
    let 打印: Vec<String> = steps(Limits::default()).iter().map(Step::display).collect();
    assert_eq!(
        打印,
        [
            "cargo fmt --all --check",
            "cargo xtask glossary",
            "cargo check --workspace",
            "cargo clippy --workspace --all-targets --all-features",
            "cargo test --workspace --all-features",
            "cargo xtask numbers --check",
            "RUSTDOCFLAGS=\"-D warnings\" cargo doc --workspace --no-deps --all-features --lib --bins",
        ]
    );
}

#[test]
fn 限流那一档把两个开关都递下去() {
    // ⭐ 15 GB 的开发机上跑全量测试会被 OOM 杀掉，所以要能退回限流那一档。
    // 两个开关**各管一段**：`-j` 管编译期的并行 rustc，`--test-threads` 管跑测试时
    // 同时活着的测试线程数——只限其中一个，另一段照样能把内存吃光。
    let 限流 = steps(Limits::throttled());
    let test = 限流
        .iter()
        .find(|it| it.name == "test")
        .expect("有 test 这一条");
    assert!(test.args.contains(&"-j".to_string()));
    assert!(test.args.contains(&"1".to_string()));
    assert!(test.args.contains(&"--test-threads=2".to_string()));

    // 默认那一档一个都不递——按机器给的资源跑。
    let 默认 = steps(Limits::default());
    let test = 默认
        .iter()
        .find(|it| it.name == "test")
        .expect("有 test 这一条");
    assert!(!test.args.iter().any(|a| a == "-j"));
    assert!(!test.args.iter().any(|a| a.starts_with("--test-threads")));
}

#[test]
fn 文档里断一条链接就红改成不带链接的写法就绿() {
    // ⭐ **这一条是票 `parking-3/02` 的题目本身。** 在那张票之前，`cargo doc` 那一条
    // 不递 `RUSTDOCFLAGS`：仓库里躺着 67 条 rustdoc 告警，而它的退出码**永远**是 0
    // ——那时门禁里只有它从来没写过标准（挂单 `Q189`）。清零之后把 `-D warnings`
    // 敲进去，这条测试钉的就是「敲进去了没有」。
    //
    // **两面都验**：只验红的那一半，一条恒红的命令也能过；只验绿的那一半，
    // 本票之前那条软红（`RUSTDOCFLAGS` 是空的、退出码永远 0）也能过。
    // 断链红 + 同一份输入改成不带链接的写法就绿，才说得上
    // 「它红的是那条断链，不是别的什么」。
    let 断了 = 丢弃crate(
        "doc-red",
        "//! 一份丢弃 crate。\n\n/// 见 [`NoSuchItem`]。\npub fn 加(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    );
    assert!(
        !这一条绿吗("doc", &断了.path),
        "文档里链到一个不存在的条目，`cargo doc` 那一条必须退非零——\
         它退 0 的时候，「门禁跑绿了」只在字面上成立"
    );

    // 同一份代码、同一条注释，只把链接换成不带链接的代码体——这正是本票清那 67 条
    // 用的写法（`[`X`]` → `` `X` ``），它不该红。
    let 改回来 = 丢弃crate(
        "doc-green",
        "//! 一份丢弃 crate。\n\n/// 见 `NoSuchItem`。\npub fn 加(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
    );
    assert!(
        这一条绿吗("doc", &改回来.path),
        "同一份输入把断链改成不带链接的写法之后，`doc` 那一条必须绿——\
         否则它红的不是那条链接，是别的什么"
    );
}

#[test]
fn 与_lib_同名的那个_bin_里断一条链接照样红() {
    // ⭐ **这一条钉的是 `--lib --bins`。** `cargo doc` 默认**跳过与 lib 同名的 bin**
    // ——两者都往 `target/doc/<crate>/index.html` 里写，撞文件名。本仓库
    // `romcat-gui` 与 `xtask` 都是 lib 加一个同名 bin，于是少了这两个开关，
    // `crates/gui/src/main.rs`（四百多行、里头七处 intra-doc 链接）与
    // `xtask/src/main.rs` **一个字都没被 rustdoc 读过**：谁往它们里头写一条断链，
    // 门禁照样全绿。这是票 `parking-3/02` 的 `/code-review` 抓出来的，
    // 而它**抓的正是本票自己在别处写下的那句「从此断一条文档链接门禁当场红」**
    // ——那句话当时只对 lib 成立。
    //
    // 下面这份丢弃 crate 的 `lib.rs` 是干净的，断链只在 `main.rs` 里。
    // 少了 `--bins` 它就是绿的——而那正是要防的假绿。
    let 干净的lib = "//! 一份丢弃 crate。\n";
    let 断了 = 丢弃crate带同名bin(
        "doc-bin-red",
        干净的lib,
        Some("//! 这个 bin 与 lib 同名。\n\n/// 见 [`NoSuchItem`]。\nfn main() {}\n"),
    );
    assert!(
        !这一条绿吗("doc", &断了.path),
        "与 lib 同名的那个 bin 里断了一条链接，`doc` 那一条必须退非零——\
         它退 0 就说明 rustdoc 压根没读过那份 `main.rs`"
    );

    let 改回来 = 丢弃crate带同名bin(
        "doc-bin-green",
        干净的lib,
        Some("//! 这个 bin 与 lib 同名。\n\n/// 见 `NoSuchItem`。\nfn main() {}\n"),
    );
    assert!(
        这一条绿吗("doc", &改回来.path),
        "同一份输入把断链改成不带链接的写法之后，`doc` 那一条必须绿"
    );
}

// ── 词表那一条扫的是哪几行 ──
//
// 纯函数那一半（哪些词元算撞、注释里不算）在 `xtask/src/glossary.rs` 里单元测试；这里验的是
// **采集那一层**：拿 diff、读词表。用的是真的 `git`，落在丢弃目录里的真仓库上。
//
// ⚠️ 词表里的词是编出来的（「近义一」「正名甲」），理由同那几条单元测试：测试钉的是规则，
// 不该因为真词表增删一个词就变红变绿。

const 丢弃词表: &str = "\
# 词表

**正名甲**:
_Avoid_: 近义一、近义二
_Gate_: 近义一、近义二
";

/// 在 `dir` 里跑一条 git，要求它跑通，交回 stdout。
///
/// 身份、签名、钩子、默认分支名一律当场给死：跑测试那台机器的全局配置（要求签名、
/// 装了提交钩子、默认分支叫 `master`）不该让这几条红——红的原因跟门禁无关，
/// 正是这份夹具最该躲开的那种假红（同 `丢弃crate带同名bin` 里 `[workspace]` 那一行）。
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=门禁测试"])
        .args(["-c", "user.email=gate@example.invalid"])
        .args(["-c", "commit.gpgsign=false"])
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("能起 git");
    assert!(
        out.status.success(),
        "git {args:?} 没跑通：{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// 往丢弃仓库里写一份文件，目录不在就建。
fn 写(dir: &Path, path: &str, text: &str) {
    let full = dir.join(path);
    fs::create_dir_all(full.parent().expect("有上级目录")).expect("能建目录");
    fs::write(full, text).expect("能写文件");
}

/// 一个 `main` 上已经有一个提交的丢弃仓库：词表、一份干净的 `lib.rs`，
/// 外加一份**带着存量命中**的 `old.rs`。
fn 丢弃仓库(tag: &str) -> 丢弃目录 {
    let dir = 丢弃目录 {
        path: 丢弃路径(tag),
    };
    git(&dir.path, &["init", "--quiet"]);
    写(&dir.path, "CONTEXT.md", 丢弃词表);
    写(&dir.path, "crates/a/src/lib.rs", "pub fn 正常() {}\n");
    写(&dir.path, "crates/a/src/old.rs", "pub fn 存量近义一() {}\n");
    git(&dir.path, &["add", "."]);
    git(&dir.path, &["commit", "--quiet", "-m", "起点"]);
    dir
}

/// 扫一趟，交出范围与每一处命中的（路径, 行, 词）。
fn 扫(dir: &Path) -> (Scope, Vec<(String, usize, String)>) {
    let report = glossary::scan(dir).expect("这一条跑得下去");
    let found = report
        .findings
        .into_iter()
        .map(|it| (it.path, it.hit.line, it.hit.gate.word))
        .collect();
    (report.scope, found)
}

/// 一处命中的（路径, 行, 词）。
fn 处(path: &str, line: usize, word: &str) -> (String, usize, String) {
    (path.to_string(), line, word.to_string())
}

#[test]
fn 词表那一条扫分支上已提交未提交与新建的改动而存量一处不报() {
    let 仓库 = 丢弃仓库("glossary-branch");
    let d = &仓库.path;
    git(d, &["checkout", "--quiet", "-b", "topic"]);

    // ⭐ `main` 在分出去之后又往前走了一步，把 `old.rs` 里那处存量改掉了。拿 `main` 的
    // **尖端**去比，`topic` 上原样没动的 `old.rs` 就会被读成「新写的」——所以比的必须是
    // merge base，不是 `main` 本身。
    git(d, &["checkout", "--quiet", "main"]);
    写(d, "crates/a/src/old.rs", "pub fn 存量() {}\n");
    git(d, &["commit", "--quiet", "-am", "main 往前走"]);
    git(d, &["checkout", "--quiet", "topic"]);

    // 接票的人跑门禁时改动一半已提交、一半没有，还有一份没 `git add` 的新文件。
    写(
        d,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 已提交的近义一() {}\n",
    );
    git(d, &["commit", "--quiet", "-am", "已提交的那一半"]);
    写(
        d,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 已提交的近义一() {}\npub const 未提交: &str = \"近义二\";\n",
    );
    写(d, "crates/a/src/new.rs", "fn 还没加进索引的近义一() {}\n");

    let (范围, 撞) = 扫(d);
    assert!(
        matches!(范围, Scope::SinceMergeBase { on_base: false, .. }),
        "分支上比的是相对 `main` 的 merge base：{范围:?}"
    );
    assert_eq!(
        撞,
        [
            处("crates/a/src/lib.rs", 2, "近义一"),
            处("crates/a/src/lib.rs", 3, "近义二"),
            处("crates/a/src/new.rs", 1, "近义一"),
        ],
        "已提交的、未提交的、没加进索引的都要报；`old.rs` 那处存量一处都不许报"
    );
}

#[test]
fn 词表那一条在_main_上只看未提交的改动() {
    // 站在 `main` 上，merge base 就是 `HEAD`，于是只剩未提交的改动可看——这是对的，
    // 不是门禁在 `main` 上失效了。`old.rs` 那处存量是 `main` 上已经提交的，不报。
    let 仓库 = 丢弃仓库("glossary-main");
    let d = &仓库.path;
    写(
        d,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 未提交的近义二() {}\n",
    );

    let (范围, 撞) = 扫(d);
    assert!(
        matches!(范围, Scope::SinceMergeBase { on_base: true, .. }),
        "在 `main` 上 merge base 就是 `HEAD`：{范围:?}"
    );
    assert_eq!(撞, [处("crates/a/src/lib.rs", 2, "近义二")]);
}

#[test]
fn 词表那一条在浅克隆上如实跳过而不是假装扫过() {
    // CI 上 `actions/checkout` 不给 `fetch-depth` 时拿到的就是这个形状：只有一层历史。
    let 源 = 丢弃仓库("glossary-shallow-src");
    写(
        &源.path,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 第二个() {}\n",
    );
    git(&源.path, &["commit", "--quiet", "-am", "第二个提交"]);
    let 克隆 = 丢弃目录 {
        path: 丢弃路径("glossary-shallow"),
    };
    let url = format!("file://{}", 源.path.display());
    let into = 克隆.path.to_string_lossy().into_owned();
    git(&源.path, &["clone", "--quiet", "--depth", "1", &url, &into]);
    // 克隆里有一处明明白白的撞词：跳过时它不许被报，也不许被说成「扫过了、干净」。
    写(
        &克隆.path,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 未提交的近义一() {}\n",
    );

    let report = glossary::scan(&克隆.path).expect("拿不到历史是跳过，不是出错");
    let Scope::Skipped { reason } = &report.scope else {
        panic!("浅克隆上求不准 merge base，必须跳过：{:?}", report.scope);
    };
    assert!(reason.contains("浅克隆"), "跳过得说清为什么：{reason}");
    assert!(
        report.findings.is_empty() && report.files == 0 && report.lines == 0,
        "跳过就是一行都没扫：{report:?}"
    );
}

#[test]
fn 词表那一条找不到_main_时如实跳过() {
    let 仓库 = 丢弃仓库("glossary-no-main");
    git(&仓库.path, &["branch", "--quiet", "-m", "main", "trunk"]);

    let report = glossary::scan(&仓库.path).expect("拿不到历史是跳过，不是出错");
    let Scope::Skipped { reason } = &report.scope else {
        panic!(
            "没有 `main` 就没有 merge base，必须跳过：{:?}",
            report.scope
        );
    };
    assert!(reason.contains("main"), "跳过得说清为什么：{reason}");
}

/// 在 `dir` 里跑 `xtask glossary`，交回退出码绿没绿与它印的全部输出。
///
/// 直接跑编出来的那个二进制，不绕 `cargo xtask`：丢弃仓库里没有那条别名。
fn 跑词表那一条(dir: &Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("glossary")
        .current_dir(dir)
        .output()
        .expect("能起 xtask");
    let 输出 = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), 输出)
}

#[test]
fn 词表那一条撞词就红并说清哪一行哪个词该用哪个正名改掉就绿() {
    // ⭐ 两面都验，理由同 `doc` 那一条：只验红的一半，一条恒红的命令也能过。
    let 仓库 = 丢弃仓库("glossary-cli");
    let d = &仓库.path;
    写(
        d,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 新的近义一() {}\n",
    );
    let (绿, 输出) = 跑词表那一条(d);
    assert!(!绿, "新写的标识符撞了 `_Gate_`，这一条必须退非零：\n{输出}");
    for 该有 in ["crates/a/src/lib.rs:2", "「近义一」", "「正名甲」"] {
        assert!(输出.contains(该有), "报红得说清「{该有}」：\n{输出}");
    }

    写(
        d,
        "crates/a/src/lib.rs",
        "pub fn 正常() {}\npub fn 新的() {}\n",
    );
    let (绿, 输出) = 跑词表那一条(d);
    assert!(绿, "改掉之后这一条必须绿——否则它红的不是那个词：\n{输出}");
}

#[test]
fn 词表里门禁词越出它那条近义词时这一条当场红() {
    let 仓库 = 丢弃仓库("glossary-subset");
    写(
        &仓库.path,
        "CONTEXT.md",
        "**正名甲**:\n_Avoid_: 近义一\n_Gate_: 近义一、近义四\n",
    );
    let (绿, 输出) = 跑词表那一条(&仓库.path);
    assert!(!绿, "`_Gate_` 越出了 `_Avoid_`，这一条必须当场红：\n{输出}");
    assert!(
        输出.contains("CONTEXT.md:3") && 输出.contains("近义四"),
        "报错得点名是哪一行、哪个词越界：\n{输出}"
    );
}

// ── 那几个算得出来的数 ──
//
// 纯函数那一半（标记怎么扫、四样各怎么数）在 `xtask/src/numbers.rs` 里单元测试；这里验的是
// **真的跑一趟**：一份数写错了的丢弃仓库上，`--check` 红、`--write` 改完就绿。
//
// ⚠️ 这份丢弃仓库自带一个最小的 crate——`--check` 要跑一趟 `cargo test … -- --list` 才数得出
// 测试条数，没有 crate 它就无从数起。crate 只有两条空测试，编一趟是秒级的。

/// 一份带标记、而**每个数都写错了**的 README。
const 数写错了的README: &str = "\
# 丢弃仓库

**<!-- 数:票数 -->9/9 张票落地**，<!-- 数:ADR份数 -->9 份 ADR。

<!-- 数:测试条数 -->9 条测试、<!-- 数:测试目标数 -->9 个测试目标。
";

/// 一份带标记的丢弃仓库：两张票（落地一张）、两份 ADR、一个最小的 crate。
fn 丢弃仓库带数(tag: &str) -> 丢弃目录 {
    let dir = 丢弃目录 {
        path: 丢弃路径(tag),
    };
    let d = &dir.path;
    // `[workspace]` 那一行是封口用的，理由同 `丢弃crate带同名bin`。
    写(
        d,
        "Cargo.toml",
        "[package]\nname = \"throwaway\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    );
    写(
        d,
        "src/lib.rs",
        "//! 一份丢弃 crate。\n\n#[test]\nfn 甲() {}\n\n#[test]\nfn 乙() {}\n",
    );
    写(
        d,
        ".scratch/q/issues/01-甲.md",
        "# 甲\n\n**Status:** done\n",
    );
    写(
        d,
        ".scratch/q/issues/02-乙.md",
        "# 乙\n\n**Status:** ready-for-agent\n\n收尾时把上面那一行改成 done。\n",
    );
    // 文件名没有那两位数字的不是票——`issues/` 底下允许躺着维护者自己的文件。
    写(d, ".scratch/q/issues/记事.md", "这不是一张票。\n");
    写(d, "docs/adr/0001-一.md", "# 一\n");
    写(d, "docs/adr/0002-二.md", "# 二\n");
    写(d, "README.md", 数写错了的README);
    dir
}

/// 在 `dir` 里跑 `xtask numbers <动作>`，交回退出码绿没绿与它印的全部输出。
fn 跑数那一条(dir: &Path, 动作: &str) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("numbers")
        .arg(动作)
        .current_dir(dir)
        .output()
        .expect("能起 xtask");
    let 输出 = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), 输出)
}

#[test]
fn 数过期了这一条红并把对的那个数说出来改写之后就绿() {
    // ⭐ **两面都验，而且红的那一面是真造出来的。** 只断言「数对上了」，一条永远绿的命令
    // 也能过；这里先让四个数都是错的，看它红、看它把对的那个数印出来，再 `--write`，
    // 同一份输入必须变绿。
    let 仓库 = 丢弃仓库带数("numbers");
    let d = &仓库.path;

    let (绿, 输出) = 跑数那一条(d, "--check");
    assert!(!绿, "四个数都写错了，这一条必须退非零：\n{输出}");
    for 该有 in [
        "README.md:3",
        "「票数」写着 9/9，对的是 1/2",
        "「ADR份数」写着 9，对的是 2",
        // ⭐ 「怎么数出来的」那一句**不是写死的**：测试条数那一行是从门禁 `test` 那一条
        // 当场折出来的。下面这行字面量是手写的独立比对基准（同
        // `每一条打印出来对得上_readme_那一段`），`test` 那条哪天改了口径，这里当场红。
        "cargo test --workspace --all-features -- --list",
    ] {
        assert!(输出.contains(该有), "报红里得有「{该有}」：\n{输出}");
    }

    let (绿, 输出) = 跑数那一条(d, "--write");
    assert!(绿, "`--write` 得跑得通：\n{输出}");
    let readme = fs::read_to_string(d.join("README.md")).expect("读得回来");
    assert!(
        readme.contains("-->1/2 张票落地") && readme.contains("-->2 份 ADR"),
        "两个数都该被写回去，别处一个字节不动：\n{readme}"
    );
    assert!(
        readme.starts_with("# 丢弃仓库\n\n**<!-- 数:票数 -->"),
        "标记与正文原样留着：\n{readme}"
    );

    let (绿, 输出) = 跑数那一条(d, "--check");
    assert!(
        绿,
        "改写之后同一份输入必须绿——否则它红的不是那几个数：\n{输出}"
    );

    // ⭐ **少一处标记不许悄悄绿。** 把票数那一处连标记一起删掉：这一样从此没人看着，
    // 而「绿着但没在看」正是这个机制自己要治的病。
    写(
        d,
        "README.md",
        "# 丢弃仓库\n\n<!-- 数:ADR份数 -->2 份 ADR。\n<!-- 数:测试条数 -->2 条、<!-- 数:测试目标数 -->2 个。\n",
    );
    let (绿, 输出) = 跑数那一条(d, "--check");
    assert!(!绿, "票数那一处标记没了，这一条必须红：\n{输出}");
    assert!(
        输出.contains("「票数」") && 输出.contains("没在看"),
        "得点名是哪一样没人看着：\n{输出}"
    );
}
