//! `cargo xtask` 的入口：`gate`，以及门禁里扫词表那一条单独的入口 `glossary`。
//!
//! ```text
//! cargo xtask gate                 # 跑门禁，按机器给的资源跑
//! cargo xtask gate --throttle      # 退回限流那一档（-j 1、--test-threads=2）
//! cargo xtask gate --list          # 只打印它会跑哪几条，一条都不跑
//! cargo xtask glossary             # 只跑门禁里扫词表的那一条
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use xtask::gate::{self, Limits};
use xtask::glossary::{self, Place, Scope};

/// 仓库里的杂活。
#[derive(Debug, Parser)]
#[command(name = "xtask", about = "仓库杂活：门禁的唯一定义", version)]
struct Cli {
    /// 干哪件杂活。
    #[command(subcommand)]
    command: Job,
}

/// 门禁，以及门禁里扫词表那一条单独的入口。
#[derive(Debug, Subcommand)]
enum Job {
    /// 跑门禁：排版、词表、默认特性编得过、clippy、全量测试、文档。
    Gate(GateArgs),
    /// 扫新写的代码撞没撞词表 `_Gate_` 的词。门禁里 `glossary` 那一条跑的就是它。
    ///
    /// 范围是相对 `main` 的 merge base 以来的改动，含未提交的；站在 `main` 上时只看未提交的。
    Glossary,
}

/// `cargo xtask gate` 的开关。
#[derive(Debug, Args)]
struct GateArgs {
    /// 退回限流那一档：`-j 1`、`--test-threads=2`。
    ///
    /// 15 GB 一档的开发机上跑全量测试会被 OOM 杀掉，这一档是为它留的。
    /// CI 上**不要**给——runner 没有这个约束，限流只会让它白白慢上几倍。
    #[arg(long)]
    throttle: bool,

    /// 并行编译任务数，覆盖 `--throttle` 给的那个。不给就交给 cargo 按核数定。
    ///
    /// **下界是 1**：`-j 0` cargo 自己会骂「jobs may not be 0」，而那时门禁表格上印出来的
    /// 是「红 clippy」——把一个打错的命令行参数说成 clippy 红了，是门禁最不该撒的那种谎。
    #[arg(short = 'j', long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    jobs: Option<u16>,

    /// 跑测试时同时活着的线程数，覆盖 `--throttle` 给的那个。下界同样是 1，理由见 [`GateArgs::jobs`]。
    #[arg(long, value_name = "N", value_parser = clap::value_parser!(u16).range(1..))]
    test_threads: Option<u16>,

    /// 红了也把剩下几条跑完。默认在第一处红上就停——`fmt` 排在最前，两秒就出结果。
    #[arg(long)]
    keep_going: bool,

    /// 只打印它会跑哪几条，一条都不跑。
    #[arg(long)]
    list: bool,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Job::Gate(args) => gate_job(&args),
        Job::Glossary => glossary_job(),
    }
}

/// 扫一趟新写的代码，把范围与每一处命中印出来。
///
/// 站在哪个目录跑，扫的就是那个目录所在的仓库——门禁起它时把工作目录设成了仓库根。
fn glossary_job() -> ExitCode {
    let dir = std::env::current_dir().unwrap_or_else(|_| repo_root());
    let report = match glossary::scan(&dir) {
        Ok(report) => report,
        Err(err) => {
            eprintln!("词表那一条跑不下去：{err}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "词表：`CONTEXT.md` 里读出 {} 个 `_Gate_` 词，每一个都在它那条 `_Avoid_` 里。",
        report.gate_words
    );
    match &report.scope {
        // ⭐ **跳过要说成跳过。** 退 0 是因为拿不到历史不是谁写错了什么，
        // 但这句话得明说一行都没扫——否则一个绿就会被读成「扫过了、干净」。
        Scope::Skipped { reason } => {
            println!("跳过：{reason}。");
            println!("这一趟一行代码都没扫——不是扫过了没撞上。");
            return ExitCode::SUCCESS;
        }
        Scope::SinceMergeBase {
            base,
            merge_base,
            on_base: true,
        } => println!(
            "范围：merge base 就是 `HEAD`（{merge_base}）——站在 `{base}` 上，\
             或者这条分支还没有自己的提交——于是只看未提交的改动。"
        ),
        Scope::SinceMergeBase {
            base, merge_base, ..
        } => println!("范围：相对 `{base}` 的 merge base（{merge_base}）以来的改动，含未提交的。"),
    }
    println!(
        "扫了 `crates/` 下 {} 份 `.rs` 里新写的 {} 行：标识符与字符串字面量，不含注释。",
        report.files, report.lines
    );
    if report.lines == 0 {
        // 一行都没扫时印「没撞上」，会被读成扫过了、干净。
        println!("没有新写的代码可扫。");
        return ExitCode::SUCCESS;
    }
    if report.findings.is_empty() {
        println!("没撞上。");
        return ExitCode::SUCCESS;
    }
    for finding in &report.findings {
        let hit = &finding.hit;
        let place = match hit.place {
            Place::Identifier => "标识符",
            Place::Literal => "字符串字面量",
        };
        println!(
            "{}:{}  {place}里撞上「{}」→ 该用「{}」（CONTEXT.md:{}）",
            finding.path, hit.line, hit.gate.word, hit.gate.term, hit.gate.line
        );
        println!("    {}", finding.excerpt);
    }
    println!(
        "红：{} 处新写的代码撞上词表 `_Gate_`。改的是名字或屏上的话；注释里谈论这个词不算撞。",
        report.findings.len()
    );
    ExitCode::FAILURE
}

/// 跑（或者只打印）门禁那几条。
fn gate_job(args: &GateArgs) -> ExitCode {
    let limits = limits(args);
    if args.list {
        for step in gate::steps(limits) {
            println!("{}", step.display());
        }
        return ExitCode::SUCCESS;
    }

    let 总条数 = gate::steps(limits).len();
    let ran = gate::run(limits, &repo_root(), args.keep_going);
    println!("\n── 门禁 ──");
    for one in &ran {
        let 记号 = if one.green { "绿" } else { "红" };
        println!("{记号}  {:<7} {:>4}s  {}", one.name, one.seconds, one.line);
    }
    if ran.len() == 总条数 && ran.iter().all(|one| one.green) {
        println!("{总条数} 条全绿。");
        return ExitCode::SUCCESS;
    }
    // ⭐ **没跑完的那几条要点名。** 只列跑过的，会被读成「没列出来的都是绿的」——
    // 门禁最不该做的事就是让人以为它盖过了它其实没碰的地方。
    let 没跑 = 总条数 - ran.len();
    if 没跑 > 0 {
        println!("在第一处红上停了，还有 {没跑} 条没跑（要跑全给 `--keep-going`）。");
    }
    ExitCode::FAILURE
}

/// 开关折成一档资源限制：`--throttle` 是底，两个具体开关各自覆盖它。
fn limits(args: &GateArgs) -> Limits {
    let base = if args.throttle {
        Limits::throttled()
    } else {
        Limits::default()
    };
    Limits {
        jobs: args.jobs.or(base.jobs),
        test_threads: args.test_threads.or(base.test_threads),
    }
}

/// 仓库根。
///
/// 从**编译期**记下的 `xtask/` 位置往上一级取，因此 `cargo xtask gate` 在仓库里
/// 哪个子目录跑都是同一趟——门禁的范围不该取决于人当时站在哪儿。
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ 上面就是仓库根")
        .to_path_buf()
}
