//! `romcat` 命令行。
//!
//! 核心能力在 `romcat-core` 里，这里只负责把参数变成一次扫描、把 Ctrl-C 变成中断信号、
//! 把报告写到标准输出或文件。**界面不是使用它的唯一途径**（ADR-0005）：10T 的扫描迟早
//! 要挂后台跑。
//!
//! 两个子命令的分工是这张票的核心：`scan` 碰盘，`report` 不碰。体检报告由**中立库**
//! 折出来，因此外置盘不在位时 `report` 照样出得来（ADR-0009）。

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use std::{fs, io};

use clap::{Args, Parser, Subcommand};
use romcat_core::catalog::Catalog;
use romcat_core::fs::RealFs;
use romcat_core::report::{DuplicateDetails, HealthReport, human_bytes, thousands};
use romcat_core::scan::aggregate::{Aggregate, Limits};
use romcat_core::scan::{self, CancelToken, CheckpointOptions, ScanOptions};
use romcat_core::workspace;

/// ROM 元数据自动化工具的命令行。
#[derive(Debug, Parser)]
#[command(name = "romcat", version, about = "ROM 元数据自动化：主库体检与扫描")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// 对主库跑一遍只读扫描，把结论写进中立库并出一份库体检报告
    Scan(ScanArgs),
    /// 只从中立库出报告，一个字节都不读主库——外置盘不在位时也能看
    Report(ReportArgs),
}

/// 报告的去处。两个子命令共用。
#[derive(Debug, Args)]
struct OutputArgs {
    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 把完整的重复拷贝明细另存成文本文件——报告里只列前 10 组，这里是全部
    #[arg(long, value_name = "文件")]
    dump_duplicates: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
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

    /// 工作目录：中立库与断点存这里。绝不写进主库
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 从上次中断的地方接着扫
    #[arg(long)]
    resume: bool,

    /// 不写断点（也就不能续跑）
    #[arg(long)]
    no_checkpoint: bool,

    /// 当作从没扫过：不按三元组跳过未变文件，每个文件重新看一遍
    #[arg(long)]
    full: bool,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct ReportArgs {
    /// 主库根目录。只用来找到对应的中立库，不会去读它
    root: PathBuf,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    #[command(flatten)]
    output: OutputArgs,
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
        Command::Report(args) => run_report(&args),
    }
}

/// 这个主库的工作目录：中立库与断点都住这里，必须在本机（ADR-0009）。
fn workspace_dir(given: Option<&Path>) -> PathBuf {
    given
        .map(Path::to_path_buf)
        .unwrap_or_else(workspace::default_dir)
}

fn open_catalog(workspace: &Path, root: &Path) -> Result<Catalog, String> {
    let path = workspace::catalog_path(workspace, root);
    // 主库只读（ADR-0004）：中立库落进主库就该在开扫之前被拦下。
    refuse_writing_into_library(root, &path)?;
    Catalog::open(&path).map_err(|error| format!("中立库打不开：{error}"))
}

fn run_scan(args: &ScanArgs, cancel: &CancelToken) -> ExitCode {
    // 先拦，再扫：10T 扫上几个钟头才发现文件写不出去，代价太大。
    if let Err(message) = args.output.refuse_targets_in_library(&args.root) {
        eprintln!("{message}");
        return ExitCode::FAILURE;
    }

    let workspace = workspace_dir(args.workspace.as_deref());
    let mut catalog = match open_catalog(&workspace, &args.root) {
        Ok(catalog) => catalog,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    let mut options = ScanOptions::new(&args.root);
    if let Some(jobs) = args.jobs {
        options.jobs = jobs.max(1);
    }
    options.samples_per_class = args.samples_per_class;
    options.incremental = !args.full;
    if args.output.dump_duplicates.is_some() {
        // 默认每组只留几条路径当例子。要导出可据以动手的清单，得把组内每一份都记下来。
        options.limits.max_duplicate_paths_per_group = Limits::FULL_DUPLICATE_PATHS_PER_GROUP;
    }

    let checkpoint_path = if args.no_checkpoint {
        None
    } else {
        Some(workspace::checkpoint_path(&workspace, &args.root))
    };
    if let Some(path) = &checkpoint_path {
        options.checkpoint = Some(CheckpointOptions {
            path: path.clone(),
            interval: Duration::from_secs(15),
            resume: args.resume,
        });
    }

    let library = RealFs::new();
    let outcome = match scan::scan(&library, &mut catalog, &options, cancel) {
        Ok(outcome) => outcome,
        Err(error) => {
            eprintln!("扫描失败：{error}");
            return ExitCode::FAILURE;
        }
    };

    let write_failed = args
        .output
        .emit(&outcome.report, &outcome.aggregate)
        .is_err();
    eprintln!("中立库：{}", catalog.location());

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

fn run_report(args: &ReportArgs) -> ExitCode {
    if let Err(message) = args.output.refuse_targets_in_library(&args.root) {
        eprintln!("{message}");
        return ExitCode::FAILURE;
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    let path = workspace::catalog_path(&workspace, &args.root);
    // 只出报告不该顺手建一个空库出来。
    if !path.exists() {
        eprintln!(
            "还没有 {} 这个主库的中立库。先跑一次 `romcat scan {}`。",
            args.root.display(),
            args.root.display()
        );
        return ExitCode::FAILURE;
    }
    let catalog = match open_catalog(&workspace, &args.root) {
        Ok(catalog) => catalog,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    match catalog.is_empty() {
        Ok(true) => {
            eprintln!(
                "中立库 {} 里还没有东西。先跑一次 `romcat scan {}`。",
                catalog.location(),
                args.root.display()
            );
            return ExitCode::FAILURE;
        }
        Ok(false) => {}
        Err(error) => {
            eprintln!("中立库读不出来：{error}");
            return ExitCode::FAILURE;
        }
    }

    let mut limits = Limits::default();
    if args.output.dump_duplicates.is_some() {
        limits.max_duplicate_paths_per_group = Limits::FULL_DUPLICATE_PATHS_PER_GROUP;
    }
    let built = catalog.aggregate(&limits).and_then(|aggregate| {
        Ok((
            HealthReport::build(&aggregate, &catalog.report_meta()?),
            aggregate,
        ))
    });
    let (report, aggregate) = match built {
        Ok(pair) => pair,
        Err(error) => {
            eprintln!("中立库读不出来：{error}");
            return ExitCode::FAILURE;
        }
    };
    if args.output.emit(&report, &aggregate).is_err() {
        return ExitCode::FAILURE;
    }
    eprintln!("以上出自中立库 {}，没有读过主库。", catalog.location());
    ExitCode::SUCCESS
}

impl OutputArgs {
    /// 主库只读（ADR-0004）：工具写出去的任何文件都不许落进主库。
    fn refuse_targets_in_library(&self, root: &Path) -> Result<(), String> {
        for target in [self.json.as_deref(), self.dump_duplicates.as_deref()]
            .into_iter()
            .flatten()
        {
            refuse_writing_into_library(root, target)?;
        }
        Ok(())
    }

    /// 把报告送到该去的地方。一份写不出去不该带走另一份：这些输出是几个钟头扫出来的，
    /// 能落盘一份是一份。
    fn emit(&self, report: &HealthReport, aggregate: &Aggregate) -> Result<(), ()> {
        if !self.quiet {
            let text = report.render_text();
            let mut stdout = io::stdout().lock();
            let _ = stdout.write_all(text.as_bytes());
            let _ = stdout.flush();
        }

        let mut failed = false;

        if let Some(path) = &self.json {
            match serde_json::to_vec_pretty(report) {
                Ok(bytes) => match write_file(path, &bytes) {
                    Ok(()) => eprintln!("报告已写入 {}", path.display()),
                    Err(error) => {
                        eprintln!("报告写不进 {}：{error}", path.display());
                        failed = true;
                    }
                },
                Err(error) => {
                    eprintln!("报告序列化失败：{error}");
                    failed = true;
                }
            }
        }

        if let Some(path) = &self.dump_duplicates {
            let details = DuplicateDetails::build(aggregate, report);
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
                    failed = true;
                }
            }
        }

        if failed { Err(()) } else { Ok(()) }
    }
}

/// 主库只读（ADR-0004）：工具写出去的任何文件都不许落进主库。
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

fn write_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 参数能解析() {
        let cli = Cli::parse_from(["romcat", "scan", "/lib", "-j", "4", "--resume"]);
        let Command::Scan(args) = cli.command else {
            panic!("解析出的该是 scan");
        };
        assert_eq!(args.root, PathBuf::from("/lib"));
        assert_eq!(args.jobs, Some(4));
        assert!(args.resume);
        assert!(!args.full, "默认是增量扫描");
        assert_eq!(args.samples_per_class, 32);
        assert_eq!(args.output.dump_duplicates, None);
    }

    #[test]
    fn 只出报告的子命令不必碰主库() {
        let cli = Cli::parse_from(["romcat", "report", "/lib", "--json", "/tmp/体检.json"]);
        let Command::Report(args) = cli.command else {
            panic!("解析出的该是 report");
        };
        assert_eq!(args.root, PathBuf::from("/lib"));
        assert_eq!(args.output.json, Some(PathBuf::from("/tmp/体检.json")));
    }

    #[test]
    fn 当作从没扫过的开关认得出来() {
        let cli = Cli::parse_from(["romcat", "scan", "/lib", "--full"]);
        let Command::Scan(args) = cli.command else {
            panic!("解析出的该是 scan");
        };
        assert!(args.full);
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
        let Command::Scan(args) = cli.command else {
            panic!("解析出的该是 scan");
        };
        assert_eq!(
            args.output.dump_duplicates,
            Some(PathBuf::from("/tmp/重复.txt"))
        );
    }

    #[test]
    fn 重复拷贝明细也不许写进主库() {
        let root = Path::new("/Volumes/ROMs");
        assert!(refuse_writing_into_library(root, Path::new("/Volumes/ROMs/重复.txt")).is_err());
        assert!(refuse_writing_into_library(root, Path::new("/tmp/重复.txt")).is_ok());
    }

    #[test]
    fn 中立库落在工作目录里且不许落进主库() {
        let root = Path::new("/Volumes/ROMs");
        let path = workspace::catalog_path(Path::new("/work"), root);
        assert!(path.starts_with("/work/catalog"));
        assert!(refuse_writing_into_library(root, &path).is_ok());
        // 真要有人把工作目录指到主库里，扫描之前就得被拦下来
        let 落在库里 = workspace::catalog_path(root, root);
        assert!(refuse_writing_into_library(root, &落在库里).is_err());
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
        let path = workspace::checkpoint_path(&workspace, Path::new("/Volumes/ROMs"));
        assert!(!path.starts_with("/Volumes/ROMs"));
    }
}
