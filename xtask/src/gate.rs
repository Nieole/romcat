//! 门禁四条的**唯一**定义。
//!
//! 在这份文件之前，「门禁」是 `README.md`「开发」一节里的四行命令：没有任何机器执行它的
//! 地方，漏跑了没有人拦，本地与 CI 也无从对齐（挂单 `Q187`）。现在四条只在下面 [`steps`]
//! 里写一遍，README 指向 `cargo xtask gate`、GitHub Actions 也只调这一句——**要漂就一起漂**。
//!
//! ## 顺序：从便宜到贵
//!
//! `fmt` 两秒出结果，`test` 要几分钟。默认在第一处红上就停（[`run`] 的 `keep_going`
//! 传 `false`），于是「忘了跑 `cargo fmt --all`」这种最常见的红，代价是两秒而不是
//! 一整趟冷编译。CI 上想一次看全四条时给 `--keep-going`。
//!
//! ## 两档资源
//!
//! 默认按机器给的资源跑（不递 `-j`，也不限测试线程）。开发机内存吃紧时退回
//! [`Limits::throttled`]——见那儿的说明，两个开关**各管一段**，只限一个是拦不住的。

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

/// `cargo doc` 这一条要不要额外递 `RUSTDOCFLAGS`。
///
/// **眼下是空的，也就是「软红」**：`cargo doc --workspace --no-deps` 退出码是 0，
/// 可仓库里躺着 68 条 rustdoc 告警（`romcat-core` 独占 56 条，挂单 `Q189`）。
/// 四条里只有这一条从来没写过标准——`clippy` 那条明写「零警告」，它没有。
///
/// 清零并把这里换成 `&[("RUSTDOCFLAGS", "-D warnings")]`（外加给命令补 `--all-features`）
/// 是票 `parking-3/02` 的活。**在那之前不能敲成硬红**：`02` 落地之前的每一次门禁都会红，
/// 而那一片红与谁改了什么毫无关系。
///
/// # ⚠️ 给票 `02` 的一句话：补 `--all-features` 之前先把这一格接住
///
/// `clippy` 与 `test` 都带 `--all-features`，于是眼下**只有 `doc` 这一条跑在默认特性上**
/// ——也就是 `demo` 关掉、`cargo build --release` 真正交付出去的那份配置。
/// 换句话说，那份配置**编不编得过，眼下是靠 rustdoc 顺带保住的**。
///
/// 一旦给 `doc` 补上 `--all-features`，门禁四条就**没有任何一条**再编译默认特性了：
/// 谁在 `crates/gui/src/` 里写下一处只有开着 `demo` 才编得过的引用，四条全绿，
/// 而 `cargo build --release` 当场编不过。补 `--all-features` 的同时请把这一格接住——
/// 要么另加一条默认特性的 `cargo check --workspace`，要么把 `doc` 留在默认特性、
/// 另开一条 `--all-features` 的。（票 `parking-3/01` 收尾审查报的第 1 条。）
const DOC_ENV: &[(&str, &str)] = &[];

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
    /// 这一条叫什么——`fmt` / `clippy` / `test` / `doc`。
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

/// 门禁四条，按从便宜到贵排。**这是它们唯一的定义。**
#[must_use]
pub fn steps(limits: Limits) -> Vec<Step> {
    vec![fmt(), clippy(limits), test(limits), doc(limits)]
}

/// 排版归机器管。`cargo fmt` 不吃 `-j`，限流那一档在这条上没有开关可递。
fn fmt() -> Step {
    Step {
        name: "fmt",
        args: ["fmt", "--all", "--check"].map(String::from).to_vec(),
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

/// 文档编得出来。眼下是**软红**，见 [`DOC_ENV`]。
fn doc(limits: Limits) -> Step {
    let mut args = ["doc", "--workspace", "--no-deps"]
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
