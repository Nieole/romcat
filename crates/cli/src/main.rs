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
use std::time::{Duration, Instant};
use std::{fs, io};

use clap::{Args, Parser, Subcommand};
use romcat_core::catalog::Catalog;
use romcat_core::dat::HttpFetcher;
use romcat_core::dat::registry::Registry;
use romcat_core::dat::repo::DatRepo;
use romcat_core::dat::report::DatReport;
use romcat_core::dat::sync::{self, Action, SyncOptions};
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::path;
use romcat_core::platform::Manifest;
use romcat_core::report::{DuplicateDetails, HealthReport, human_bytes, thousands};
use romcat_core::scan::aggregate::{Aggregate, Limits};
use romcat_core::scan::{self, CancelToken, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::scrape::{self, Priorities};
use romcat_core::shape;
use romcat_core::workspace::{self, Slug};

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
    /// 按平台清单重新成型：把中立库里散落的条目聚成变体。改了成型规则不必重扫主库
    Shape(ShapeArgs),
    /// 拿变体的 CRC-32 加大小撞 DAT，产出带置信度与依据的候选，并报出真实命中率
    Identify(IdentifyArgs),
    /// 在识别结论上取元数据与媒体。离线档只用本地数据源，一个网络请求都不发
    Scrape(ScrapeArgs),
    /// 列出眼下生效的平台清单与成型规则，或者导出一份底稿照着改
    Platforms(PlatformsArgs),
    /// DAT 仓库：把几个哈希数据库镜像到本地，并报出每个平台有多少条可用记录
    #[command(subcommand)]
    Dat(DatCommand),
}

/// DAT 仓库的几件事。
#[derive(Debug, Subcommand)]
enum DatCommand {
    /// 同步 DAT。指纹没变的整件跳过，不必每次全量重下
    Sync(DatSyncArgs),
    /// 只从 DAT 库出报告，一个字节都不联网
    Report(DatReportArgs),
    /// 列出眼下生效的数据源清单，或者导出一份底稿照着改
    Sources(DatSourcesArgs),
}

/// 数据源清单从哪儿来。三个子命令共用。
#[derive(Debug, Args, Clone)]
struct SourcesArgs {
    /// 数据源清单 TOML 文件
    ///
    /// 不给就先看工作目录里有没有 `sources.toml`，都没有才用内置的那一份。
    /// `romcat dat sources --dump-builtin` 能导出一份底稿照着改
    #[arg(long, value_name = "文件")]
    sources: Option<PathBuf>,
}

impl SourcesArgs {
    /// 工作目录里那份可选的清单叫什么。
    const IN_WORKSPACE: &'static str = "sources.toml";

    fn load(&self, workspace: &Path) -> Result<Registry, String> {
        let path = match &self.sources {
            Some(path) => path.clone(),
            None => {
                let candidate = workspace.join(Self::IN_WORKSPACE);
                if !candidate.exists() {
                    return Ok(Registry::builtin());
                }
                candidate
            }
        };
        Registry::load(&path).map_err(|error| format!("{error}"))
    }
}

#[derive(Debug, Args)]
struct DatSyncArgs {
    /// 只同步这几个源（`No-Intro` / `Redump` / `TOSEC` / `MAME` / `GoodNES`），可重复给
    #[arg(long = "source", value_name = "源名")]
    only: Vec<String>,

    /// 无视指纹，全部重取
    #[arg(long)]
    full: bool,

    /// 只排计划、报出这一趟会干什么，不取也不写
    #[arg(long)]
    dry_run: bool,

    /// 工作目录：DAT 库与取回来的原件存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 两次请求之间至少隔多少毫秒（按主机计）
    #[arg(long, default_value_t = 1000)]
    throttle_ms: u64,

    #[command(flatten)]
    manifest: ManifestArgs,

    #[command(flatten)]
    sources: SourcesArgs,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct DatReportArgs {
    /// 工作目录：DAT 库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct DatSourcesArgs {
    /// 工作目录：不给 `--sources` 时来这里找 `sources.toml`
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 把**内置**清单写到这个文件，照着它改就是自己的一份
    #[arg(long, value_name = "文件")]
    dump_builtin: Option<PathBuf>,

    #[command(flatten)]
    sources: SourcesArgs,
}

/// 平台清单从哪儿来。三个子命令共用。
#[derive(Debug, Args, Clone)]
struct ManifestArgs {
    /// 平台清单与成型规则的 TOML 文件
    ///
    /// 不给就先看工作目录里有没有 `platforms.toml`，都没有才用内置的那一份。
    /// `romcat platforms --dump-builtin` 能导出一份底稿照着改
    #[arg(long, value_name = "文件")]
    manifest: Option<PathBuf>,
}

impl ManifestArgs {
    /// 工作目录里那份可选的清单叫什么。
    const IN_WORKSPACE: &'static str = "platforms.toml";

    fn load(&self, workspace: &Path) -> Result<Manifest, String> {
        let path = match &self.manifest {
            Some(path) => path.clone(),
            None => {
                let candidate = workspace.join(Self::IN_WORKSPACE);
                if !candidate.exists() {
                    return Ok(Manifest::builtin());
                }
                candidate
            }
        };
        Manifest::load(&path).map_err(|error| format!("{error}"))
    }
}

#[derive(Debug, Args)]
struct ShapeArgs {
    /// 主库根目录。只用来找到对应的中立库，不会去读它；给了 `--library` 就不必再给
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 人工纠正：这几个键其实是一个变体。第一个当主文件，可重复给
    ///
    /// 纠正**优先于一切成型规则**，也不随重新成型或重扫消失——规则的缺陷不该永久污染库
    #[arg(long = "merge", value_name = "键")]
    merge: Vec<String>,

    /// 撤掉某个键上的人工纠正，可重复给
    #[arg(long = "forget-merge", value_name = "键")]
    forget_merge: Vec<String>,

    #[command(flatten)]
    manifest: ManifestArgs,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct IdentifyArgs {
    /// 主库根目录。要算裸文件的哈希才用得上；给了 `--library` 就不必再给
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库与 DAT 库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 一个字节都不读主库
    ///
    /// 容器里零解压可得的 CRC-32 照撞；裸文件与去头那套哈希则报「无判据」——
    /// 它们只能从字节里来。盘不在位时用它
    #[arg(long)]
    no_read_library: bool,

    /// 单份内容读到多大（MiB）就不读了；不给就不设上限
    #[arg(long, value_name = "MiB")]
    max_read_mib: Option<u64>,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

#[derive(Debug, Args)]
struct ScrapeArgs {
    /// 主库根目录。只有要把本地媒体收进媒体池才用得上；给了 `--library` 就不必再给
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库与媒体池存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 字段级优先级表
    ///
    /// 不给就先看工作目录里有没有 `priorities.toml`，都没有才用内置的那一份
    #[arg(long, value_name = "文件")]
    priorities: Option<PathBuf>,

    /// 把**内置**优先级表写到这个文件，照着它改就是自己的一份
    #[arg(long, value_name = "文件")]
    dump_priorities: Option<PathBuf>,

    /// 不收媒体。一个字节都不读主库，盘不在位时用它
    #[arg(long)]
    no_media: bool,

    /// 单份媒体大到多少 MiB 就不收了；不给就不设上限
    #[arg(long, value_name = "MiB")]
    max_media_mib: Option<u64>,

    /// 无视采集记录全部重采。**媒体池里的文件一个都不删**
    #[arg(long)]
    refresh: bool,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

impl ScrapeArgs {
    /// 工作目录里那份可选的优先级表叫什么。
    const IN_WORKSPACE: &'static str = "priorities.toml";

    fn load_priorities(&self, workspace: &Path) -> Result<Priorities, String> {
        let path = match &self.priorities {
            Some(path) => path.clone(),
            None => {
                let candidate = workspace.join(Self::IN_WORKSPACE);
                if !candidate.exists() {
                    return Ok(Priorities::builtin());
                }
                candidate
            }
        };
        Priorities::load(&path).map_err(|error| format!("{error}"))
    }
}

#[derive(Debug, Args)]
struct PlatformsArgs {
    /// 工作目录：不给 `--manifest` 时来这里找 `platforms.toml`
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 把**内置**清单写到这个文件，照着它改就是自己的一份
    #[arg(long, value_name = "文件")]
    dump_builtin: Option<PathBuf>,

    #[command(flatten)]
    manifest: ManifestArgs,
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

    /// 并发线程数（默认开扫前探一探介质自己定；点了名就照办）
    #[arg(short, long)]
    jobs: Option<usize>,

    /// 给这个主库起个名字，中立库跟名字走而不跟路径走
    ///
    /// macOS 重挂一次盘就可能从 `/Volumes/新加卷` 变成 `/Volumes/新加卷 1`，Windows 上盘符也会变。
    /// 起了名字，换挂载点还找得回同一份中立库；不给就维持老行为（按绝对路径取）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

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

    /// 不穿透 zip 与 7z：不去读容器内部的 CRC-32、大小与名字
    ///
    /// 库里 91% 的容量在透明容器里，关掉它报告就只知道「这里有一个 3GB 的容器」，看不见里面装着什么
    #[arg(long)]
    no_containers: bool,

    #[command(flatten)]
    manifest: ManifestArgs,

    #[command(flatten)]
    output: OutputArgs,
}

#[derive(Debug, Args)]
struct ReportArgs {
    /// 主库根目录。只用来找到对应的中立库，不会去读它；给了 `--library` 就不必再给
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    ///
    /// 用它就不必再报出主库路径——盘换了挂载点、甚至根本没插，报告照样出得来
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    #[command(flatten)]
    manifest: ManifestArgs,

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
        Command::Shape(args) => run_shape(&args),
        Command::Identify(args) => run_identify(&args, &cancel),
        Command::Scrape(args) => run_scrape(&args, &cancel),
        Command::Platforms(args) => run_platforms(&args),
        Command::Dat(DatCommand::Sync(args)) => run_dat_sync(&args),
        Command::Dat(DatCommand::Report(args)) => run_dat_report(&args),
        Command::Dat(DatCommand::Sources(args)) => run_dat_sources(&args),
    }
}

/// 从 `--library` 与主库根挑出该开哪一份中立库，并说清是靠什么找到的。
fn locate<'a>(
    library: Option<&'a str>,
    root: Option<&'a Path>,
) -> Result<(Slug<'a>, String), String> {
    match (library, root) {
        (Some(name), _) => Ok((Slug::Named(name), format!("--library {name}"))),
        (None, Some(root)) => Ok((Slug::AtPath(root), path::display(root))),
        (None, None) => Err("要么给出主库根目录，要么用 --library 报出中立库的名字。".to_string()),
    }
}

/// 打一句给用户，然后以失败收场。
///
/// 命令行这一层几乎每个岔路口都是「说清楚 + 退出」，抽出来之后每处只剩一行，
/// 也就看得出各处说的话有没有真的不一样。
fn fail(message: impl AsRef<str>) -> ExitCode {
    eprintln!("{}", message.as_ref());
    ExitCode::FAILURE
}

/// 从中立库折出报告并送到该去的地方。`report` 与 `shape` 共用这一段。
fn emit_from_catalog(catalog: &Catalog, manifest: &Manifest, output: &OutputArgs) -> ExitCode {
    let mut limits = Limits::default();
    if output.dump_duplicates.is_some() {
        // 默认每组只留几条路径当例子。要导出可据以动手的清单，得把组内每一份都记下来。
        limits.max_duplicate_paths_per_group = Limits::FULL_DUPLICATE_PATHS_PER_GROUP;
    }
    let built = catalog.aggregate(&limits, manifest).and_then(|aggregate| {
        Ok((
            HealthReport::build(&aggregate, &catalog.report_meta()?),
            aggregate,
        ))
    });
    let (report, aggregate) = match built {
        Ok(pair) => pair,
        Err(error) => return fail(format!("中立库读不出来：{error}")),
    };
    if !output.emit(&report, &aggregate) {
        return ExitCode::FAILURE;
    }
    eprintln!("以上出自中立库 {}，没有读过主库。", catalog.location());
    ExitCode::SUCCESS
}

/// 这个主库的工作目录：中立库与断点都住这里，必须在本机（ADR-0009）。
fn workspace_dir(given: Option<&Path>) -> PathBuf {
    given
        .map(Path::to_path_buf)
        .unwrap_or_else(workspace::default_dir)
}

fn open_catalog(workspace: &Path, slug: Slug<'_>, root: Option<&Path>) -> Result<Catalog, String> {
    let path = workspace::catalog_path(workspace, slug);
    // 主库只读（ADR-0004）：中立库落进主库就该在开扫之前被拦下。
    if let Some(root) = root {
        refuse_writing_into_library(root, &path)?;
    }
    Catalog::open(&path).map_err(|error| format!("中立库打不开：{error}"))
}

fn run_scan(args: &ScanArgs, cancel: &CancelToken) -> ExitCode {
    // 先拦，再扫：10T 扫上几个钟头才发现文件写不出去，代价太大。
    if let Err(message) = args.output.refuse_targets_in_library(&args.root) {
        return fail(message);
    }

    let workspace = workspace_dir(args.workspace.as_deref());
    let slug = Slug::pick(args.library.as_deref(), &args.root);
    let mut catalog = match open_catalog(&workspace, slug, Some(&args.root)) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };

    let mut options = ScanOptions::new(&args.root);
    if let Some(jobs) = args.jobs {
        options.jobs = Jobs::Fixed(jobs.max(1));
    }
    options.samples_per_class = args.samples_per_class;
    options.manifest = match args.manifest.load(&workspace) {
        Ok(manifest) => manifest,
        Err(message) => return fail(message),
    };
    options.incremental = !args.full;
    options.penetrate_containers = !args.no_containers;
    if args.output.dump_duplicates.is_some() {
        // 默认每组只留几条路径当例子。要导出可据以动手的清单，得把组内每一份都记下来。
        options.limits.max_duplicate_paths_per_group = Limits::FULL_DUPLICATE_PATHS_PER_GROUP;
    }

    let checkpoint_path = if args.no_checkpoint {
        None
    } else {
        Some(workspace::checkpoint_path(&workspace, slug))
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

    if outcome.shaped {
        eprintln!(
            "成型完毕：{} 个变体。",
            thousands(outcome.report.shaping.variants)
        );
    } else if outcome.interrupted {
        eprintln!("这一趟被中断，没有重新成型——半个库上成出来的变体是错的。");
    }
    if let Some(probe) = &outcome.probe {
        // 并发是量出来的不是猜出来的，那就把量到的数说出来——用户看得见依据才敢信它，
        // 不服也知道该拿 `-j` 覆盖成多少。
        eprintln!("{}", probe.describe());
    }

    let wrote_everything = args.output.emit(&outcome.report, &outcome.aggregate);
    eprintln!("中立库：{}", catalog.location());

    if !wrote_everything {
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
    // 只给了 `--library` 时连主库路径都不必知道——这正是名字那条路的用处：
    // 盘换了挂载点、甚至根本没插，报告照样出得来（ADR-0009）。
    let (slug, located_by) = match locate(args.library.as_deref(), args.root.as_deref()) {
        Ok(pair) => pair,
        Err(message) => return fail(message),
    };
    if let Some(root) = args.root.as_deref()
        && let Err(message) = args.output.refuse_targets_in_library(root)
    {
        return fail(message);
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    let path = workspace::catalog_path(&workspace, slug);
    // 只出报告不该顺手建一个空库出来。
    if !path.exists() {
        return fail(format!(
            "还没有 {located_by} 这份中立库。先跑一次 `romcat scan`。"
        ));
    }
    let catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };

    // 只给了名字时，主库在哪只有中立库知道。**这道守卫不能因此漏掉**——
    // 主库只读（ADR-0004），报告写不进去这条与用没用 `--library` 无关。
    if args.root.is_none()
        && let Ok(Some(recorded)) = catalog.library_root()
        && let Err(message) = args.output.refuse_targets_in_library(Path::new(&recorded))
    {
        return fail(message);
    }

    match catalog.is_empty() {
        Ok(true) => {
            eprintln!(
                "中立库 {} 里还没有东西。先跑一次 `romcat scan`。",
                catalog.location()
            );
            return ExitCode::FAILURE;
        }
        Ok(false) => {}
        Err(error) => {
            eprintln!("中立库读不出来：{error}");
            return ExitCode::FAILURE;
        }
    }

    let manifest = match args.manifest.load(&workspace) {
        Ok(manifest) => manifest,
        Err(message) => return fail(message),
    };
    emit_from_catalog(&catalog, &manifest, &args.output)
}

/// 按平台清单重新成型一遍。**一个字节都不读主库**——成型是中立库上的纯计算，
/// 改一条规则不必重扫 8.6 TiB。
fn run_shape(args: &ShapeArgs) -> ExitCode {
    let (slug, located_by) = match locate(args.library.as_deref(), args.root.as_deref()) {
        Ok(pair) => pair,
        Err(message) => return fail(message),
    };
    if let Some(root) = args.root.as_deref()
        && let Err(message) = args.output.refuse_targets_in_library(root)
    {
        return fail(message);
    }
    if args.merge.len() == 1 {
        return fail("--merge 至少要给两个键：一个变体由哪几个条目组成，一个键说不清。");
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    let path = workspace::catalog_path(&workspace, slug);
    if !path.exists() {
        return fail(format!(
            "还没有 {located_by} 这份中立库。先跑一次 `romcat scan`。"
        ));
    }
    let manifest = match args.manifest.load(&workspace) {
        Ok(manifest) => manifest,
        Err(message) => return fail(message),
    };
    let mut catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };

    // 人工纠正先落库，再成型——顺序反了的话这一趟成型还用的是旧的纠正。
    for key in &args.forget_merge {
        match catalog.clear_shaping_override(key) {
            Ok(true) => eprintln!("撤掉了 {key} 上的人工纠正。"),
            Ok(false) => eprintln!("{key} 上本来就没有人工纠正。"),
            Err(error) => {
                eprintln!("中立库写不进：{error}");
                return ExitCode::FAILURE;
            }
        }
    }
    // 键打错了要当场说，别静悄悄地记一条永远不生效的纠正。
    for key in args.merge.iter().chain(&args.forget_merge) {
        match catalog.contains(key) {
            Ok(true) => {}
            Ok(false) => {
                eprintln!(
                    "中立库里没有 {key} 这条记录。键是**相对主库根**的路径，分隔符是 `/`（ADR-0020）。"
                );
                return ExitCode::FAILURE;
            }
            Err(error) => {
                eprintln!("中立库读不出来：{error}");
                return ExitCode::FAILURE;
            }
        }
    }
    if let Some((main, rest)) = args.merge.split_first() {
        for key in std::iter::once(main).chain(rest) {
            if let Err(error) = catalog.set_shaping_override(key, main) {
                eprintln!("中立库写不进：{error}");
                return ExitCode::FAILURE;
            }
        }
        eprintln!(
            "记下人工纠正：{} 个条目并成一个变体，主文件是 {main}。",
            args.merge.len()
        );
    }

    let scan = match catalog.last_traversal() {
        Ok(Some(traversal)) => traversal.scan,
        Ok(None) => {
            eprintln!(
                "中立库 {} 里还没有遍历记录。先跑一次 `romcat scan`。",
                catalog.location()
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("中立库读不出来：{error}");
            return ExitCode::FAILURE;
        }
    };
    let plan = match shape::reshape(&mut catalog, &manifest, scan) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("成型失败：{error}");
            return ExitCode::FAILURE;
        }
    };
    eprintln!(
        "成型完毕：{} 个变体（另有 {} 个范围之内的文件不是可玩的东西，没进变体）。",
        thousands(u64::try_from(plan.variants.len()).unwrap_or(u64::MAX)),
        thousands(plan.unshaped_files)
    );

    emit_from_catalog(&catalog, &manifest, &args.output)
}

/// 拿变体去撞 DAT。**不联网、不写任何 ROM 文件。**
///
/// 读盘只发生在一处：裸文件的哈希只能从字节里来，容器里的 CRC-32 零解压就有（票 03）。
/// `--no-read-library` 把那一处也关掉，于是一个字节都不读主库。
fn run_identify(args: &IdentifyArgs, cancel: &CancelToken) -> ExitCode {
    let (slug, located_by) = match locate(args.library.as_deref(), args.root.as_deref()) {
        Ok(pair) => pair,
        Err(message) => return fail(message),
    };
    let workspace = workspace_dir(args.workspace.as_deref());
    let catalog_path = workspace::catalog_path(&workspace, slug);
    if !catalog_path.exists() {
        return fail(format!(
            "还没有 {located_by} 这份中立库。先跑一次 `romcat scan`。"
        ));
    }
    let dat_path = workspace::dat_repo_path(&workspace);
    if !dat_path.exists() {
        return fail(format!(
            "还没有 DAT 库（该在 {}）。先跑一次 `romcat dat sync`——没有弹药就没有命中率。",
            dat_path.display()
        ));
    }
    let mut catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    let repo = match DatRepo::open(&dat_path) {
        Ok(repo) => repo,
        Err(error) => return fail(format!("DAT 库打不开：{error}")),
    };

    // 主库根：命令行给的优先，没给就问中立库——盘换了挂载点时那一份才是对的。
    let root = match args.root.clone() {
        Some(root) => Some(root),
        None => catalog.library_root().ok().flatten().map(PathBuf::from),
    };
    if root.is_none() && !args.no_read_library {
        return fail(
            "不知道主库在哪：给出主库根目录，或者加 --no-read-library 只用容器里那套零解压的 CRC-32。",
        );
    }
    // 主库只读（ADR-0004）：报告不许落进主库。守一次就够——上面已经把「主库在哪」
    // 定下来了（命令行给的优先，没给就问中立库），两处各守一遍只会让人以为它们守的
    // 不是同一件事。
    if let Some(root) = &root
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }

    let mut options = identify::Options::new(root.unwrap_or_default());
    options.read_library = !args.no_read_library;
    options.max_read_bytes = args.max_read_mib.map(|mib| mib.saturating_mul(1 << 20));

    let library = RealFs::new();
    let started = Instant::now();
    let mut last = Instant::now();
    let outcome = identify::run(
        &library,
        &mut catalog,
        &repo,
        &options,
        cancel,
        &mut |progress| {
            // 46,444 个变体、可能几十分钟：不说进度的话，用户分不清它是在干活还是卡住了。
            if last.elapsed() >= Duration::from_secs(5) {
                last = Instant::now();
                eprintln!(
                    "  已算 {} / {} 个变体，命中 {}，读盘 {}",
                    thousands(progress.done),
                    thousands(progress.total),
                    thousands(progress.matched),
                    human_bytes(progress.read_bytes),
                );
            }
        },
    );
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("识别失败：{error}")),
    };

    if !args.quiet {
        let text = outcome.report.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "识别用了 {:.1} 秒，回盘读了 {}（{} 份内容），另有 {} 份哈希是从中立库直接取回来的。",
        started.elapsed().as_secs_f64(),
        human_bytes(outcome.read_bytes),
        thousands(outcome.read_files),
        thousands(outcome.reused_hashes),
    );
    if !write_json(args.json.as_deref(), &outcome.report) {
        return ExitCode::FAILURE;
    }
    if outcome.interrupted {
        eprintln!("这一趟被中断了，已经算完的那部分留在中立库里，重跑会从头算一遍。");
        return ExitCode::from(130);
    }
    ExitCode::SUCCESS
}

/// 在识别结论上取元数据与媒体。**离线档一个网络请求都不发。**
///
/// 读盘只发生在一处：把主库里现成的图与视频收进**媒体池**。`--no-media` 把那一处也
/// 关掉，于是一个字节都不读主库。
fn run_scrape(args: &ScrapeArgs, cancel: &CancelToken) -> ExitCode {
    if let Some(path) = &args.dump_priorities {
        match write_file(path, Priorities::builtin_text().as_bytes()) {
            Ok(()) => {
                println!(
                    "内置优先级表已写入 {}。改完用 `--priorities {}` 生效，\n\
                     或者放进工作目录叫 priorities.toml 自动生效。",
                    path.display(),
                    path.display()
                );
                return ExitCode::SUCCESS;
            }
            Err(error) => return fail(format!("写不进 {}：{error}", path.display())),
        }
    }

    let (slug, located_by) = match locate(args.library.as_deref(), args.root.as_deref()) {
        Ok(pair) => pair,
        Err(message) => return fail(message),
    };
    let workspace = workspace_dir(args.workspace.as_deref());
    let catalog_path = workspace::catalog_path(&workspace, slug);
    if !catalog_path.exists() {
        return fail(format!(
            "还没有 {located_by} 这份中立库。先跑一次 `romcat scan`。"
        ));
    }
    let priorities = match args.load_priorities(&workspace) {
        Ok(priorities) => priorities,
        Err(message) => return fail(message),
    };
    let mut catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };

    // 主库根：命令行给的优先，没给就问中立库——盘换了挂载点时那一份才是对的。
    let root = match args.root.clone() {
        Some(root) => Some(root),
        None => catalog.library_root().ok().flatten().map(PathBuf::from),
    };
    if root.is_none() && !args.no_media {
        return fail("不知道主库在哪：给出主库根目录，或者加 --no-media 只采元数据不收媒体。");
    }
    // 主库只读（ADR-0004）：报告不许落进主库。
    if let Some(root) = &root
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }

    let pool_dir = workspace::media_pool_dir(&workspace);
    // 媒体池也不许落进主库：它是要往里写文件的（ADR-0009 说它必须在本机）。
    if let Some(root) = &root
        && let Err(message) = refuse_writing_into_library(root, &pool_dir)
    {
        return fail(message);
    }

    let mut options = scrape::Options::new(root.clone().unwrap_or_default(), pool_dir);
    options.media = !args.no_media;
    options.max_media_bytes = args.max_media_mib.map(|mib| mib.saturating_mul(1 << 20));
    options.refresh = args.refresh;

    let library = RealFs::new();
    let started = Instant::now();
    let mut last = Instant::now();
    let mut progress = |progress: scrape::Progress| {
        if last.elapsed() >= Duration::from_secs(5) {
            last = Instant::now();
            eprintln!(
                "  已采 {} / {} 个锚点，收进媒体 {} 份，读盘 {}",
                thousands(progress.done),
                thousands(progress.total),
                thousands(progress.media),
                human_bytes(progress.read_bytes),
            );
        }
    };
    let outcome = scrape::run(
        &library,
        &mut catalog,
        &priorities,
        &options,
        &mut scrape::RunContext {
            cancel,
            progress: &mut progress,
        },
    );
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("刮削失败：{error}")),
    };

    if !args.quiet {
        let text = outcome.report.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "刮削用了 {:.1} 秒。回盘读了 {}（{} 份媒体），另有 {} 份媒体的哈希从中立库直接取回；\n         {} 个「锚点 × 源」因为输入没变整条跳过。池里新增 {} 份，另有 {} 次算出来发现池里已经有了（那就是「只存一份」）。",
        started.elapsed().as_secs_f64(),
        human_bytes(outcome.read_bytes),
        thousands(outcome.read_files),
        thousands(outcome.reused_hashes),
        thousands(outcome.reused_probes),
        thousands(outcome.new_blobs),
        thousands(outcome.deduped),
    );
    // 两种跳过分开说。**它们不是同一件事**：超上限的那些文件好好的，
    // 是自己设了上限；读不动的那些是 ADR-0021 的第三态。合成一句话，
    // 用户会以为盘出了问题。
    if outcome.forgotten > 0 {
        eprintln!(
            "有 {} 个「锚点 × 源」这次无话可说，上一轮的结论已清掉——\n             那些值背后的 DAT 条目不在了，留着只会带出一条对不上的依据。",
            thousands(outcome.forgotten)
        );
    }
    if outcome.not_media > 0 {
        eprintln!(
            "有 {} 份被认领的媒体，扩展名这一层却不认得——本地媒体源与媒体池的扩展名表\n             对不上了，这是要查的。",
            thousands(outcome.not_media)
        );
    }
    if outcome.oversized_media > 0 {
        eprintln!(
            "有 {} 份媒体超过了 --max-media-mib 的上限，没收进来（文件本身没问题）。",
            thousands(outcome.oversized_media)
        );
    }
    if outcome.unreadable_media > 0 {
        eprintln!(
            "有 {} 份媒体读不动，跳过了（ADR-0021 的第三态，不是错误）。",
            thousands(outcome.unreadable_media)
        );
    }
    if !write_json(args.json.as_deref(), &outcome.report) {
        return ExitCode::FAILURE;
    }
    if outcome.interrupted {
        eprintln!("这一趟被中断了，已经采完的那部分留在中立库里，重跑会接着采。");
        return ExitCode::from(130);
    }
    ExitCode::SUCCESS
}

/// 列出眼下生效的平台清单与成型规则。
///
/// 「平台清单是数据不是代码」要落地，用户得看得见眼下到底生效的是哪一份、里面有什么。
fn run_platforms(args: &PlatformsArgs) -> ExitCode {
    if let Some(path) = &args.dump_builtin {
        match write_file(path, Manifest::builtin_text().as_bytes()) {
            Ok(()) => {
                println!(
                    "内置平台清单已写入 {}。改完用 `--manifest {}` 生效，\n\
                     或者放进工作目录叫 platforms.toml 自动生效。",
                    path.display(),
                    path.display()
                );
            }
            Err(error) => {
                eprintln!("写不进 {}：{error}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    let manifest = match args.manifest.load(&workspace) {
        Ok(manifest) => manifest,
        Err(message) => return fail(message),
    };
    println!("成型规则");
    for rule in manifest.rules() {
        println!("  {}（{}）", rule.name, rule.kind.label());
    }
    println!("\n平台（{} 个）", manifest.platforms().len());
    for platform in manifest.platforms() {
        let rules = if platform.rules.is_empty() {
            "一文件一变体".to_string()
        } else {
            platform.rules.join("、")
        };
        println!(
            "  {:<10} 目录 {:<28} 成型 {rules}",
            platform.name,
            platform.dirs.join("、")
        );
    }
    if !manifest.out_of_scope().is_empty() {
        println!("\n明确排除的目录");
        for dir in manifest.out_of_scope() {
            println!("  {} —— {}", dir.dir, dir.reason);
        }
    }
    ExitCode::SUCCESS
}

/// 同步一趟 DAT。**这是这个程序里唯一联网的子命令。**
fn run_dat_sync(args: &DatSyncArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let registry = match args.sources.load(&workspace) {
        Ok(registry) => registry,
        Err(message) => return fail(message),
    };
    let manifest = match args.manifest.load(&workspace) {
        Ok(manifest) => manifest,
        Err(message) => return fail(message),
    };
    // 平台名打错一个字，那份 DAT 就归到一个谁也查不到的平台上。开工前说出来。
    let unknown = registry.unknown_platforms(&manifest);
    if !unknown.is_empty() {
        return fail(format!(
            "数据源清单里这几个平台名在平台清单里查无此人：{}。\n\
             `romcat platforms` 看得到眼下有哪些平台。",
            unknown.join("、")
        ));
    }

    let mut repo = match DatRepo::open(&workspace::dat_repo_path(&workspace)) {
        Ok(repo) => repo,
        Err(error) => return fail(format!("DAT 库打不开：{error}")),
    };
    let mut options = SyncOptions::new(&workspace);
    options.only = args.only.clone();
    options.full = args.full;
    options.dry_run = args.dry_run;
    if !options.only.is_empty()
        && let Some(unknown) = options
            .only
            .iter()
            .find(|name| registry.source(name).is_none())
    {
        return fail(format!(
            "数据源清单里没有 {unknown} 这个源。`romcat dat sources` 看得到有哪些。"
        ));
    }

    let fetcher = HttpFetcher::with_throttle(Duration::from_millis(args.throttle_ms));
    let outcome = match sync::run(&fetcher, &mut repo, &registry, &options) {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("同步失败：{error}")),
    };

    for plan in &outcome.plans {
        eprintln!(
            "{:<10} 取 {}，已是最新 {}，不入库 {}",
            plan.source,
            plan.to_fetch(),
            plan.up_to_date(),
            plan.out_of_scope()
        );
        // 被闸门拦下的一条都不能沉默——那正是这张票要防的事。
        for item in &plan.items {
            if let Action::Refused(refusal) = &item.action {
                eprintln!("  ⚠ {} 被拦下：{refusal}", item.name);
            }
        }
    }
    if args.dry_run {
        eprintln!("这是 --dry-run，什么都没取、什么都没写。");
        return ExitCode::SUCCESS;
    }
    eprintln!(
        "同步完毕：取了 {} 件、跳过 {} 件，写进 {} 份 DAT（另有 {} 份取回来了但没有映射命中，\
         不入库）、{} 条条目（汉化 {}、官中 {}）。",
        outcome.fetched,
        outcome.skipped,
        outcome.dats,
        outcome.unmapped_dats,
        thousands(outcome.counts.games),
        thousands(outcome.counts.fan),
        thousands(outcome.counts.official),
    );
    for problem in &outcome.problems {
        eprintln!("  ⚠ {problem}");
    }

    emit_dat_report(&repo, args.json.as_deref())
}

/// 只从 DAT 库出报告。**一个字节都不联网。**
fn run_dat_report(args: &DatReportArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let path = workspace::dat_repo_path(&workspace);
    if !path.exists() {
        return fail(format!(
            "还没有 DAT 库（该在 {}）。先跑一次 `romcat dat sync`。",
            path.display()
        ));
    }
    let repo = match DatRepo::open(&path) {
        Ok(repo) => repo,
        Err(error) => return fail(format!("DAT 库打不开：{error}")),
    };
    emit_dat_report(&repo, args.json.as_deref())
}

fn emit_dat_report(repo: &DatRepo, json: Option<&Path>) -> ExitCode {
    let report = match DatReport::build(repo) {
        Ok(report) => report,
        Err(error) => return fail(format!("DAT 库读不出来：{error}")),
    };
    let text = report.render_text();
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
    if !write_json(json, &report) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 把一份报告另存成 JSON，返回成没成。`path` 是 `None` 就什么都不做。
///
/// 三个子命令都要这一段（体检、DAT 仓库、识别），各写一遍只会让「写不出去时说什么」
/// 三处各不相同。
fn write_json<T: serde::Serialize>(path: Option<&Path>, report: &T) -> bool {
    let Some(path) = path else {
        return true;
    };
    match serde_json::to_vec_pretty(report) {
        Ok(bytes) => match write_file(path, &bytes) {
            Ok(()) => {
                eprintln!("报告已写入 {}", path.display());
                true
            }
            Err(error) => {
                eprintln!("报告写不进 {}：{error}", path.display());
                false
            }
        },
        Err(error) => {
            eprintln!("报告序列化失败：{error}");
            false
        }
    }
}

/// 列出眼下生效的数据源清单。
fn run_dat_sources(args: &DatSourcesArgs) -> ExitCode {
    if let Some(path) = &args.dump_builtin {
        match write_file(path, Registry::builtin_text().as_bytes()) {
            Ok(()) => println!(
                "内置数据源清单已写入 {}。改完用 `--sources {}` 生效，\n\
                 或者放进工作目录叫 sources.toml 自动生效。",
                path.display(),
                path.display()
            ),
            Err(error) => {
                eprintln!("写不进 {}：{error}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    let registry = match args.sources.load(&workspace) {
        Ok(registry) => registry,
        Err(message) => return fail(message),
    };
    println!("数据源（{} 个）", registry.sources().len());
    for source in registry.sources() {
        println!(
            "\n  {}（{}，默认口径 {}）",
            source.name,
            source.shape.label(),
            source.convention.label()
        );
        println!("    取自 {}", source.origin.describe());
        for line in source.note.trim().lines() {
            println!("    {line}");
        }
    }
    let mapped = registry
        .mappings()
        .iter()
        .filter(|mapping| mapping.platform.is_some())
        .count();
    println!(
        "\n映射 {} 条（{} 条归到平台，{} 条明确不要）。没有任何映射命中的 DAT 整份不入库。",
        registry.mappings().len(),
        mapped,
        registry.mappings().len() - mapped
    );
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

    /// 把报告送到该去的地方，返回是不是每一份都写出去了。
    ///
    /// 一份写不出去不该带走另一份：这些输出是几个钟头扫出来的，能落盘一份是一份。
    /// 出错的原因当场打给用户，因此这里只需回答成没成。
    fn emit(&self, report: &HealthReport, aggregate: &Aggregate) -> bool {
        if !self.quiet {
            let text = report.render_text();
            let mut stdout = io::stdout().lock();
            let _ = stdout.write_all(text.as_bytes());
            let _ = stdout.flush();
        }

        let mut failed = false;

        if !write_json(self.json.as_deref(), report) {
            failed = true;
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

        !failed
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
        assert_eq!(args.root, Some(PathBuf::from("/lib")));
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
        let path = workspace::catalog_path(Path::new("/work"), Slug::AtPath(root));
        assert!(path.starts_with("/work/catalog"));
        assert!(refuse_writing_into_library(root, &path).is_ok());
        // 真要有人把工作目录指到主库里，扫描之前就得被拦下来
        let 落在库里 = workspace::catalog_path(root, Slug::AtPath(root));
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
        let path = workspace::checkpoint_path(&workspace, Slug::AtPath(Path::new("/Volumes/ROMs")));
        assert!(!path.starts_with("/Volumes/ROMs"));
    }
}
