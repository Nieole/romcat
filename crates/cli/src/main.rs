//! `romcat` 命令行。
//!
//! 核心能力在 `romcat-core` 里，这里只负责把参数变成一次扫描、把 Ctrl-C 变成中断信号、
//! 把报告写到标准输出或文件。**界面不是使用它的唯一途径**（ADR-0005）：10T 的扫描迟早
//! 要挂后台跑。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use std::{env, fs};

use clap::{Args, Parser, Subcommand};
use romcat_core::fs::RealFs;
use romcat_core::report::{DuplicateDetails, human_bytes, thousands};
use romcat_core::scan::aggregate::Limits;
use romcat_core::scan::{self, CancelToken, CheckpointOptions, ScanOptions};

/// ROM 元数据自动化工具的命令行。
#[derive(Debug, Parser)]
#[command(name = "romcat", version, about = "ROM 元数据自动化：主库体检与扫描")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 对主库跑一遍只读扫描，出一份库体检报告
    Scan(ScanArgs),
}

#[derive(Debug, Args)]
struct ScanArgs {
    /// 主库根目录
    root: PathBuf,

    /// 并发线程数（默认按 CPU 数取，夹在 2 到 8 之间）
    #[arg(short, long)]
    jobs: Option<usize>,

    /// 每类文件抽样读多少个头部；0 表示不抽样
    #[arg(long, default_value_t = 32)]
    samples_per_class: usize,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 把完整的重复拷贝明细另存成文本文件——报告里只列前 10 组，这里是全部
    #[arg(long, value_name = "文件")]
    dump_duplicates: Option<PathBuf>,

    /// 工作目录：断点存这里。绝不写进主库
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 从上次中断的地方接着扫
    #[arg(long)]
    resume: bool,

    /// 不写断点（也就不能续跑）
    #[arg(long)]
    no_checkpoint: bool,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

fn main() -> ExitCode {
    // 中断处理装在最前面：晚一步装上，早来的 Ctrl-C 就会走默认动作直接杀掉进程，
    // 断点也就存不下来了。
    let cancel = CancelToken::new();
    let handler_token = cancel.clone();
    if let Err(error) = ctrlc::set_handler(move || {
        eprintln!("\n收到中断，正在收尾并保存断点……");
        handler_token.cancel();
    }) {
        eprintln!("装不上中断处理：{error}。扫描照常开始，但 Ctrl-C 不会保存断点。");
    }

    let cli = Cli::parse();
    match cli.command {
        Command::Scan(args) => run_scan(&args, &cancel),
    }
}

fn run_scan(args: &ScanArgs, cancel: &CancelToken) -> ExitCode {
    // 先拦，再扫：10T 扫上几个钟头才发现文件写不出去，代价太大。
    for target in [args.json.as_deref(), args.dump_duplicates.as_deref()]
        .into_iter()
        .flatten()
    {
        if let Err(message) = refuse_writing_into_library(&args.root, target) {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    }

    let mut options = ScanOptions::new(&args.root);
    if let Some(jobs) = args.jobs {
        options.jobs = jobs.max(1);
    }
    options.samples_per_class = args.samples_per_class;
    if args.dump_duplicates.is_some() {
        // 默认每组只留几条路径当例子。要导出可据以动手的清单，得把组内每一份都记下来。
        options.limits.max_duplicate_paths_per_group = Limits::FULL_DUPLICATE_PATHS_PER_GROUP;
    }

    let checkpoint_path = if args.no_checkpoint {
        None
    } else {
        let workspace = args.workspace.clone().unwrap_or_else(default_workspace);
        Some(checkpoint_path_for(&workspace, &args.root))
    };
    if let Some(path) = &checkpoint_path {
        options.checkpoint = Some(CheckpointOptions {
            path: path.clone(),
            interval: Duration::from_secs(15),
            resume: args.resume,
        });
    }

    let library = RealFs::new();
    let outcome = match scan::scan(&library, &options, cancel) {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("扫描失败：{error}");
            return ExitCode::FAILURE;
        }
    };

    if !args.quiet {
        let text = outcome.report.render_text();
        let mut stdout = std::io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }

    // 一份写不出去不该带走另一份：这些输出是几个钟头扫出来的，能落盘一份是一份。
    let mut write_failed = false;

    if let Some(path) = &args.json {
        match serde_json::to_vec_pretty(&outcome.report) {
            Ok(bytes) => match write_file(path, &bytes) {
                Ok(()) => eprintln!("报告已写入 {}", path.display()),
                Err(error) => {
                    eprintln!("报告写不进 {}：{error}", path.display());
                    write_failed = true;
                }
            },
            Err(error) => {
                eprintln!("报告序列化失败：{error}");
                write_failed = true;
            }
        }
    }

    if let Some(path) = &args.dump_duplicates {
        let details = DuplicateDetails::build(&outcome.aggregate, &outcome.report);
        match write_file(path, details.render_text().as_bytes()) {
            Ok(()) => eprintln!(
                "重复拷贝完整明细已写入 {}（{} 组、{} 个文件，每组留一份可腾出 {}）",
                path.display(),
                thousands(details.group_count()),
                thousands(details.files),
                human_bytes(details.reclaimable_bytes)
            ),
            Err(error) => {
                eprintln!("重复拷贝明细写不进 {}：{error}", path.display());
                write_failed = true;
            }
        }
    }

    if write_failed {
        return ExitCode::FAILURE;
    }

    if outcome.interrupted {
        if let Some(path) = &outcome.checkpoint_path {
            eprintln!(
                "扫描被中断，进度已存在 {}。加 --resume 接着扫。",
                path.display()
            );
        } else {
            eprintln!("扫描被中断，且没有写断点，下次要从头扫。");
        }
        // 130 是被 SIGINT 打断的惯例退出码。
        return ExitCode::from(130);
    }
    ExitCode::SUCCESS
}

/// 主库只读（ADR-0004）：工具写出去的任何文件都不许落进主库。
///
/// 核心库自己会拦断点，这里拦的是命令行才有的输出——报告与重复拷贝清单。
fn refuse_writing_into_library(root: &Path, target: &Path) -> Result<(), String> {
    let root = romcat_core::path::normalize_existing(root);
    let target = romcat_core::path::normalize_existing(target);
    if romcat_core::path::is_inside(&root, &target) {
        return Err(format!(
            "输出文件 {} 落在主库内。主库只读，请写到别处。",
            romcat_core::path::display(&target)
        ));
    }
    Ok(())
}

fn write_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

/// 工作目录：断点住这里，票 02 的中立库与媒体池也会落在这里。
///
/// 它必须在**本机**而不是外置盘上——外置盘不常挂载，库若跟着盘走，盘不在时连浏览
/// 元数据都做不到（ADR-0009）。
fn default_workspace() -> PathBuf {
    for key in ["ROMCAT_HOME", "XDG_DATA_HOME", "APPDATA"] {
        if let Some(value) = env::var_os(key) {
            let path = PathBuf::from(value);
            if !path.as_os_str().is_empty() {
                return if key == "ROMCAT_HOME" {
                    path
                } else {
                    path.join("romcat")
                };
            }
        }
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("romcat");
    }
    env::temp_dir().join("romcat")
}

/// 每个主库一份断点，按路径取名，互不覆盖。
fn checkpoint_path_for(workspace: &Path, root: &Path) -> PathBuf {
    let text = root.to_string_lossy();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    let name: String = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "library".to_string())
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(24)
        .collect();
    let name = if name.is_empty() {
        "library".to_string()
    } else {
        name
    };
    workspace
        .join("scans")
        .join(format!("{name}-{hash:016x}.checkpoint.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 参数能解析() {
        let cli = Cli::parse_from(["romcat", "scan", "/lib", "-j", "4", "--resume"]);
        let Command::Scan(args) = cli.command;
        assert_eq!(args.root, PathBuf::from("/lib"));
        assert_eq!(args.jobs, Some(4));
        assert!(args.resume);
        assert_eq!(args.samples_per_class, 32);
        assert_eq!(args.dump_duplicates, None);
    }

    #[test]
    fn 完整重复明细要另存到指定文件() {
        let cli = Cli::parse_from([
            "romcat",
            "scan",
            "/lib",
            "--dump-duplicates",
            "/tmp/重复.txt",
        ]);
        let Command::Scan(args) = cli.command;
        assert_eq!(args.dump_duplicates, Some(PathBuf::from("/tmp/重复.txt")));
    }

    #[test]
    fn 重复拷贝明细也不许写进主库() {
        let root = Path::new("/Volumes/ROMs");
        assert!(refuse_writing_into_library(root, Path::new("/Volumes/ROMs/重复.txt")).is_err());
        assert!(refuse_writing_into_library(root, Path::new("/tmp/重复.txt")).is_ok());
    }

    #[test]
    fn 不同主库的断点互不覆盖() {
        let workspace = PathBuf::from("/work");
        let a = checkpoint_path_for(&workspace, Path::new("/Volumes/ROMs"));
        let b = checkpoint_path_for(&workspace, Path::new("/Volumes/ROMs2"));
        assert_ne!(a, b);
        assert!(a.starts_with("/work/scans"));
        assert!(a.to_string_lossy().contains("ROMs"));
    }

    #[test]
    fn 报告不许写进主库() {
        let root = Path::new("/Volumes/ROMs");
        assert!(refuse_writing_into_library(root, Path::new("/Volumes/ROMs/体检.json")).is_err());
        assert!(
            refuse_writing_into_library(root, Path::new("/Volumes/ROMs/FC/x/体检.json")).is_err()
        );
        assert!(refuse_writing_into_library(root, Path::new("/tmp/体检.json")).is_ok());
    }

    #[test]
    fn 断点路径不落在主库里() {
        let workspace = PathBuf::from("/work");
        let path = checkpoint_path_for(&workspace, Path::new("/Volumes/ROMs"));
        assert!(!path.starts_with("/Volumes/ROMs"));
    }
}
