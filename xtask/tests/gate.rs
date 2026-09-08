//! 门禁那条子命令的钉子。
//!
//! 这份文件只钉三件事，别的都不该在这儿验：
//!
//! - **fmt 那一条真的会拦人。** 给它一份故意不 fmt-clean 的输入，它红；
//!   跑一趟 `cargo fmt --all` 改回来，它绿。这条走的是**真的子进程**——
//!   门禁的价值全在「它真的会退非零」上，断言参数串对不对是验不到这件事的。
//! - **doc 那一条真的会拦人。** 同样的两面验法：给它一份文档里断了一条链接的输入，
//!   它红；把那条链接改成不带链接的写法，它绿。这一条是票 `parking-3/02` 的题目本身
//!   ——在那张票之前 `cargo doc` 那一条退出码**永远**是 0，「跑绿」只在字面上成立。
//! - **定义只有一份。** 五条的名字与形态从 `gate::steps` 一处来，
//!   README 与 CI 都只指向它。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use xtask::gate::{Limits, Step, steps};

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
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let unique = 序号.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!(
        "romcat-xtask-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
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
    let 五条 = steps(Limits::default());
    let 它 = 五条
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
fn 门禁就是五条而且顺序是从便宜到贵() {
    // ⭐ **顺序不是随手排的**：fmt 两秒就出结果，`test` 要几分钟。默认在第一处红上就停，
    // 于是「忘了跑 fmt」这种最常见的红，代价是两秒而不是一整趟编译。
    let 五条 = steps(Limits::default());
    let 名字: Vec<&str> = 五条.iter().map(|it| it.name).collect();
    assert_eq!(名字, ["fmt", "check", "clippy", "test", "doc"]);
}

#[test]
fn 交付出去的那份配置还有一条在看着() {
    // ⭐ **这一条是票 `parking-3/02` 给 `doc` 补 `--all-features` 时接住的那一格。**
    // `clippy` / `test` / `doc` 三条都带 `--all-features`，于是它们编的都是**开着 `demo`**
    // 的那份代码；而 `cargo build --release` 交付出去的是 `demo` 关掉的那份。
    // 少了下面这一条，「交付的那份还编不编得过」在门禁里一个把门的都没有：
    // 谁在 `crates/gui/src/` 里写下一处只有开着 `demo` 才编得过的引用，门禁全绿，
    // 而 `cargo build --release` 当场编不过。
    let 五条 = steps(Limits::default());
    let 盖默认特性的: Vec<&str> = 五条
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
fn 五条打印出来对得上_readme_那一段() {
    // ⭐ 这条钉的是**定义只有一份**：README 的门禁块从此只写 `cargo xtask gate`，
    // 「它到底跑什么」由 `cargo xtask gate --list` 当场打印，不再有第二份手抄的
    // 清单在旁边慢慢漂。下面这五行字面量是手写的**独立比对基准**，不是从 `steps`
    // 反推出来的：其中三行（`fmt` / `clippy` / `test`）原样抄自门禁落地之前 README
    // 的开发块，另两行是票 `parking-3/02` 添的——`check` 那一条，以及敲成硬红、
    // 补上 `--all-features` 之后的 `doc`。
    let 打印: Vec<String> = steps(Limits::default()).iter().map(Step::display).collect();
    assert_eq!(
        打印,
        [
            "cargo fmt --all --check",
            "cargo check --workspace",
            "cargo clippy --workspace --all-targets --all-features",
            "cargo test --workspace --all-features",
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
    // ——四条门禁里只有它从来没写过标准（挂单 `Q189`）。清零之后把 `-D warnings`
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
    // 门禁五条全绿。这是票 `parking-3/02` 的 `/code-review` 抓出来的，
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
