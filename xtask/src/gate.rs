//! 门禁五条的**唯一**定义。
//!
//! 在这份文件之前，「门禁」是 `README.md`「开发」一节里的四行命令：没有任何机器执行它的
//! 地方，漏跑了没有人拦，本地与 CI 也无从对齐（挂单 `Q187`）。现在五条只在下面 [`steps`]
//! 里写一遍，README 指向 `cargo xtask gate`、GitHub Actions 也只调这一句——**要漂就一起漂**。
//!
//! ## 顺序：从便宜到贵
//!
//! `fmt` 两秒出结果，`test` 要几分钟。默认在第一处红上就停（[`run`] 的 `keep_going`
//! 传 `false`），于是「忘了跑 `cargo fmt --all`」这种最常见的红，代价是两秒而不是
//! 一整趟冷编译。CI 上想一次看全五条时给 `--keep-going`。
//!
//! ## 两份特性配置，各有一条盯着
//!
//! `clippy` / `test` / `doc` 三条都带 `--all-features`——不带的话 `romcat-gui`
//! 那七个 `required-features = ["demo"]` 的测试目标整份都不生成二进制，全仓库少跑
//! 一百多条**而退出码照样是 0**。可 `cargo build --release` 交出去的是 `demo`
//! **关掉**的那份：三条全带 `--all-features` 就意味着没有任何一条再编译交付的那份配置，
//! 谁在 `crates/gui/src/` 里写下一处只有开着 `demo` 才编得过的引用，门禁全绿而发版
//! 当场编不过。`check` 那一条补的正是这一格——它是五条里**唯一**跑在默认特性上的。
//! （票 `parking-3/01` 收尾审查报的第 1 条，票 `parking-3/02` 落的。）
//!
//! ## 两档资源
//!
//! 默认按机器给的资源跑（不递 `-j`，也不限测试线程）。开发机内存吃紧时退回
//! [`Limits::throttled`]——见那儿的说明，两个开关**各管一段**，只限一个是拦不住的。

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

/// `cargo doc` 这一条额外递的 `RUSTDOCFLAGS`：**rustdoc 的告警一律当错**。
///
/// 在票 `parking-3/02` 之前这里是空的，也就是「软红」：`cargo doc --workspace --no-deps`
/// 退出码是 0，可仓库里躺着 **67** 条 rustdoc 告警（`romcat-core` 独占 58 条，
/// 挂单 `Q189`）——四条里只有这一条从来没写过标准，`clippy` 那条明写「零警告」，它没有。
/// 那 67 条清到零之后，这一格换成了 `-D warnings`：**从此断一条文档链接，门禁当场红。**
///
/// 这不是一句声明，`xtask/tests/gate.rs` 里那条
/// `文档里断一条链接就红改成不带链接的写法就绿` 两面都验过：同一份丢弃 crate，
/// 文档里链到一个不存在的条目就退非零，把链接改成不带链接的代码体就退 0。
///
/// # 代价，说清楚
///
/// 硬红是**全仓面**的：往后任何一处新的 rustdoc 告警都会让门禁红，而不是攒着。
/// 这正是要的——但也意味着写文档链接时得当真，尤其是这四类（本票清掉的就是它们）：
/// 多余的显式链接目标、解析不了的 intra-doc 链接、「既是函数又是模块」的歧义
/// （要用 `mod@` 或 `()` 消歧）、以及**公开文档链到私有条目**。最后那一类不许靠把
/// 私有条目改成公开来消警——那是改 API 面；改成不带链接的代码体即可。
const DOC_ENV: &[(&str, &str)] = &[("RUSTDOCFLAGS", "-D warnings")];

/// 跑门禁时给这台机器留多少余量。
///
/// 默认两项都是 `None`——按机器给的资源跑，不递任何开关。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Limits {
    /// 并行编译任务数（`cargo -j`）。`None` = 交给 cargo 自己按核数定。
    pub jobs: Option<u16>,
    /// 跑测试时同时活着的线程数（`--test-threads`）。`None` = 交给测试框架按核数定。
    pub test_threads: Option<u16>,
}

impl Limits {
    /// 限流那一档：`-j 1`、`--test-threads=2`。
    ///
    /// **两个开关各管一段，少一个都不成立**：`-j` 管的是编译期同时活着的 `rustc`
    /// （峰值内存大头在这儿），`--test-threads` 管的是跑起来之后同时活着的测试线程
    /// （这个仓库的测试要造临时库、解压样本、开 SQLite）。只限其中一个，另一段照样能把
    /// 内存吃光。
    ///
    /// 这两个数不是拍的：票 `queue-followups/11` 就是拿这一档在一台 15 GB 的开发机上
    /// 跑完整趟全量测试的。
    #[must_use]
    pub const fn throttled() -> Self {
        Self {
            jobs: Some(1),
            test_threads: Some(2),
        }
    }
}

/// 门禁里的一条命令。程序一律是 `cargo`（见 [`cargo`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// 这一条叫什么——`fmt` / `check` / `clippy` / `test` / `doc`。
    pub name: &'static str,
    /// 递给 `cargo` 的参数，按顺序。
    pub args: Vec<String>,
    /// 这一条额外要的环境变量。
    pub env: Vec<(&'static str, String)>,
}

impl Step {
    /// 人读的那一行，形如 `cargo fmt --all --check`。
    ///
    /// `cargo xtask gate --list` 打印的就是它——「门禁到底跑什么」由这里当场答，
    /// 而不是靠文档里抄一份会漂的清单。
    #[must_use]
    pub fn display(&self) -> String {
        let mut out = String::new();
        for (key, value) in &self.env {
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(value);
            out.push_str("\" ");
        }
        out.push_str("cargo");
        for arg in &self.args {
            out.push(' ');
            out.push_str(arg);
        }
        out
    }

    /// 折成一条可以跑的命令，工作目录设成 `dir`。
    #[must_use]
    pub fn command_in(&self, dir: &Path) -> Command {
        let mut command = Command::new(cargo());
        command.args(&self.args).current_dir(dir);
        for (key, value) in &self.env {
            command.env(key, value);
        }
        command
    }
}

/// 该拿哪个 `cargo` 起子进程。
///
/// 优先用环境变量 `CARGO`——`cargo xtask` 起我们的时候会把**它自己**那份的绝对路径写进去，
/// 于是子进程用的必定是同一条工具链（仓库根的 `rust-toolchain.toml` 钉的那一版）。
/// 拿不到才退回 `PATH` 上的 `cargo`。
#[must_use]
pub fn cargo() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"))
}

/// 门禁五条，按从便宜到贵排。**这是它们唯一的定义。**
#[must_use]
pub fn steps(limits: Limits) -> Vec<Step> {
    vec![
        fmt(),
        check(limits),
        clippy(limits),
        test(limits),
        doc(limits),
    ]
}

/// 排版归机器管。`cargo fmt` 不吃 `-j`，限流那一档在这条上没有开关可递。
fn fmt() -> Step {
    Step {
        name: "fmt",
        args: ["fmt", "--all", "--check"].map(String::from).to_vec(),
        env: Vec::new(),
    }
}

/// 交付出去的那份配置还编得过。**五条里唯一不带 `--all-features` 的一条。**
///
/// ⚠️ **这一条的价值全在它没带的那个开关上。** `clippy` / `test` / `doc` 都带
/// `--all-features`，编的是 `demo` 开着的那份代码；而 `cargo build --release`
/// 交付的是 `demo` 关掉的那份。少了这一条，「交付的那份还编不编得过」在门禁里
/// 一个把门的都没有——谁在 `crates/gui/src/` 里写下一处只有开着 `demo` 才编得过的
/// 引用，门禁全绿而发版当场编不过。
///
/// 不带 `--all-targets`：要盯的是 `cargo build --release` 那份配置，也就是库与二进制。
///
/// ⚠️ **于是「默认特性下的测试目标」五条里一条都没编。** `clippy` 编的那批测试目标是
/// `demo` **开着**的那一份，盖不住这一格。漏的是这么一个文件：**不带**
/// `#[cfg(feature = "demo")]`、却引用了 `demo` 后面的东西——五条全绿，而
/// `cargo test --workspace`（不带 `--all-features`）编不过。眼下不存在这种文件
/// （`demo` 那七个测试目标走的是 `required-features`）。要盖住就给这一条补
/// `--all-targets`，代价是多编一遍默认特性下的测试目标。挂单 `Q208`。
fn check(limits: Limits) -> Step {
    let mut args = ["check", "--workspace"].map(String::from).to_vec();
    push_jobs(&mut args, limits);
    Step {
        name: "check",
        args,
        env: Vec::new(),
    }
}

/// 零警告。`--all-features` 不能省，理由见 [`test`]。
fn clippy(limits: Limits) -> Step {
    let mut args = ["clippy", "--workspace", "--all-targets", "--all-features"]
        .map(String::from)
        .to_vec();
    push_jobs(&mut args, limits);
    Step {
        name: "clippy",
        args,
        env: Vec::new(),
    }
}

/// 全量测试。
///
/// ⚠️ **`--all-features` 不能省。** `romcat-gui` 有一个默认关掉的 `demo` feature，
/// 不带它，那七个 `required-features = ["demo"]` 的测试目标**整份都不生成二进制**，
/// 全仓库少跑一百多条——**而退出码照样是 0**。这正是门禁必须写成一条子命令、
/// 而不是留给人记四行命令的原因。
fn test(limits: Limits) -> Step {
    let mut args = ["test", "--workspace", "--all-features"]
        .map(String::from)
        .to_vec();
    push_jobs(&mut args, limits);
    if let Some(threads) = limits.test_threads {
        args.push("--".to_string());
        args.push(format!("--test-threads={threads}"));
    }
    Step {
        name: "test",
        args,
        env: Vec::new(),
    }
}

/// 文档编得出来，而且**一条 rustdoc 告警都没有**——硬红，见 [`DOC_ENV`]。
///
/// `--all-features` 与 `clippy` / `test` 同一个理由（见 [`test`]）：不带它，
/// `demo` 后面那一整块的文档谁都没读过。它让出去的那一格由 [`check`] 接住。
///
/// ⚠️ **`--lib --bins` 不能省。** `cargo doc` 默认**跳过与 lib 同名的 bin**
/// （两者都往 `target/doc/<crate>/index.html` 里写，撞文件名）。本仓库
/// `romcat-gui` 与 `xtask` 都是 lib 加一个同名 bin，于是不带这两个开关时
/// `crates/gui/src/main.rs`（四百多行，里头七处 intra-doc 链接）与
/// `xtask/src/main.rs` **一个字都没被 rustdoc 读过**——谁往它们里头写一条断链，
/// 门禁五条全绿。加上之后 cargo 会为那两个包各印一行
/// `warning: output filename collision`：那是 **cargo** 的告警不是 rustdoc 的，
/// `-D warnings` 不把它当错，而门禁不消费 `target/doc/`（人自己跑
/// `cargo doc --open` 时不带 `--bins`，拿到的仍是正常的 lib 文档）。
/// 代价记在挂单 `Q209`。
fn doc(limits: Limits) -> Step {
    let mut args = [
        "doc",
        "--workspace",
        "--no-deps",
        "--all-features",
        "--lib",
        "--bins",
    ]
    .map(String::from)
    .to_vec();
    push_jobs(&mut args, limits);
    Step {
        name: "doc",
        args,
        env: DOC_ENV
            .iter()
            .map(|(key, value)| (*key, (*value).to_string()))
            .collect(),
    }
}

/// 有限流就把 `-j N` 递下去，没有就一个字都不加。
fn push_jobs(args: &mut Vec<String>, limits: Limits) {
    if let Some(jobs) = limits.jobs {
        args.push("-j".to_string());
        args.push(jobs.to_string());
    }
}

/// 一条跑完之后的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// 哪一条。
    pub name: &'static str,
    /// 跑的是哪一行。
    pub line: String,
    /// 绿了没有。
    pub green: bool,
    /// 花了多少秒。
    pub seconds: u64,
}

/// 跑一趟门禁，边跑边把每一条的输出直接透给终端。
///
/// `keep_going` 为 `false` 时在第一处红上就停——后面那几条的红多半是同一个原因的回声，
/// 而它们各要几分钟。
///
/// 返回跑过的那几条；调用方按「有没有一条不绿」定退出码。
#[must_use]
pub fn run(limits: Limits, dir: &Path, keep_going: bool) -> Vec<Ran> {
    let mut out = Vec::new();
    for step in steps(limits) {
        let line = step.display();
        println!("\n── {} ──\n$ {line}", step.name);
        let started = Instant::now();
        let green = match step.command_in(dir).stdin(Stdio::null()).status() {
            Ok(status) => status.success(),
            // 起不来（`cargo` 不在 `PATH` 上、`dir` 不存在）与「跑了但红了」同样算红，
            // 但得把原因说出来——否则终端上只剩一个光秃秃的红叉。
            Err(err) => {
                eprintln!("起不来：{err}");
                false
            }
        };
        out.push(Ran {
            name: step.name,
            line,
            green,
            seconds: started.elapsed().as_secs(),
        });
        if !green && !keep_going {
            break;
        }
    }
    out
}
