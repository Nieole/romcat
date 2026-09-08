//! `cargo xtask` 的入口。眼下只有一条子命令：`gate`。
//!
//! ```text
//! cargo xtask gate                 # 门禁四条，按机器给的资源跑
//! cargo xtask gate --throttle      # 退回限流那一档（-j 1、--test-threads=2）
//! cargo xtask gate --list          # 只打印它会跑哪四条，一条都不跑
//! ```

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};
use xtask::gate::{self, Limits};

/// 仓库里的杂活。
#[derive(Debug, Parser)]
#[command(name = "xtask", about = "仓库杂活：门禁四条的唯一定义", version)]
struct Cli {
    /// 干哪件杂活。
    #[command(subcommand)]
    command: Job,
}

/// 眼下只有门禁一件。
#[derive(Debug, Subcommand)]
enum Job {
    /// 跑门禁：排版、clippy、全量测试、文档。
    Gate(GateArgs),
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

    /// 只打印它会跑哪四条，一条都不跑。
    #[arg(long)]
    list: bool,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Job::Gate(args) => gate_job(&args),
    }
}

/// 跑（或者只打印）门禁四条。
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
    // ⭐ **没跑完的那几条要点名。** 只列跑过的两条，会被读成「另外两条是绿的」——
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
