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
use romcat_core::adapter::{self, Adapter, transfer};
use romcat_core::capability::{Roster, today};
use romcat_core::catalog::Catalog;
use romcat_core::dat::HttpFetcher;
use romcat_core::dat::registry::Registry;
use romcat_core::dat::repo::DatRepo;
use romcat_core::dat::report::DatReport;
use romcat_core::dat::sync::{self as dat_sync, Action, SyncOptions};
use romcat_core::filename::Rules;
use romcat_core::fs::RealFs;
use romcat_core::identify;
use romcat_core::identify::model;
use romcat_core::path;
use romcat_core::platform::Manifest;
use romcat_core::report::{DuplicateDetails, HealthReport, human_bytes, pad, thousands};
use romcat_core::scan::aggregate::{Aggregate, Limits};
use romcat_core::scan::{self, CancelToken, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::scrape::{self, Priorities};
use romcat_core::shape;
use romcat_core::site::Site;
use romcat_core::sublibrary::{self, Sublibrary};
use romcat_core::sync;
use romcat_core::title;
use romcat_core::titledb;
use romcat_core::triage::{self, DecisionSpec, Filter};
use romcat_core::verdict::{self, Store};
use romcat_core::workspace::{self, Slug};
use romcat_core::zh;

/// ROM 元数据自动化工具的命令行。
#[derive(Debug, Parser)]
#[command(name = "romcat", version, about = "ROM 元数据自动化：主库体检与扫描")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
// 这个枚举**一个进程只构造一次**（`Cli::parse()`），最大的那一支比最小的大几百字节
// 与任何东西都无关。而 clippy 提的「装箱」在这里做不到：`clap` 的 derive 要求变体的
// 载荷自己实现 `FromArgMatches`，`Box<T>` 没有实现。
#[allow(clippy::large_enum_variant)]
enum Command {
    /// 对主库跑一遍只读扫描，把结论写进中立库并出一份库体检报告
    Scan(ScanArgs),
    /// 只从中立库出报告，一个字节都不读主库——外置盘不在位时也能看
    Report(ReportArgs),
    /// 按平台清单重新成型：把中立库里散落的条目聚成变体。改了成型规则不必重扫主库
    Shape(ShapeArgs),
    /// 拿变体的 CRC-32 加大小撞 DAT，产出带置信度与依据的候选，并报出真实命中率
    Identify(IdentifyArgs),
    /// 在识别结论上取元数据与媒体。离线档一个网络请求都不发，中文名、类型与简介都补得上；在线档补封面与开发商，默认限流
    Scrape(ScrapeArgs),
    /// 折出标题集合，挑出显示标题与排序标题，并报出多少个作品拿到了中文标题
    Titles(TitlesArgs),
    /// 把维护者手工维护的前端元数据导进中立库。原文逐字节留存，往返实测当场报出档位
    Import(ImportArgs),
    /// 把中立库导出成前端元数据。作品级收敛，检测到外部改动就停下来、不静默覆盖
    Export(ExportArgs),
    /// 列出眼下带的适配器与它们的能力档位——导出前就知道哪个格式会丢掉什么
    Adapters,
    /// 列出眼下生效的平台清单与成型规则，或者导出一份底稿照着改
    Platforms(PlatformsArgs),
    /// **能力档案**：目标设备吃得下什么、目标存储放得下什么，连每条声明的出处与核实日期
    Capability(CapabilityArgs),
    /// **待确认队列**：列出识别拿不定主意的变体，按目录 / 候选作品 / 命名规律批量裁决
    #[command(subcommand)]
    Triage(TriageCommand),
    /// 子库与选择集：一台目标设备一个子库，选择集由**规则**加**例外**组成
    #[command(subcommand, alias = "sublib")]
    Sublibrary(SublibraryCommand),
    /// DAT 仓库：把几个哈希数据库镜像到本地，并报出每个平台有多少条可用记录
    #[command(subcommand)]
    Dat(DatCommand),
    /// **中文离线数据源**：把中文条目索引取到本机，数据库覆盖不到时靠它撞文件名
    #[command(subcommand)]
    Zh(ZhCommand),
    /// **Switch**：取一份第三方 TitleID 数据库，或者读一份转储的**明文文件名表**
    #[command(subcommand)]
    Switch(SwitchCommand),
    /// **文件名剥离规则**：看看一个名字剥完剩什么、导出一份规则底稿照着改，
    /// 或者把名字还是乱码的那些容器重读一遍
    Names(NamesArgs),
}

/// Switch 那一层的几件事。
#[derive(Debug, Subcommand)]
enum SwitchCommand {
    /// 取一次第三方 TitleID 数据库并建成本机索引。`ETag` 没变就整件跳过
    Sync(SwitchSyncArgs),
    /// **读一份转储的明文文件名表**——一个密钥都不要，一个字节的内容都不解开
    Read(SwitchReadArgs),
}

#[derive(Debug, Args)]
struct SwitchSyncArgs {
    /// 工作目录：TitleID 索引与取回来的原件存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 无视 `ETag`，整份重建索引。原件在手边就不重下
    #[arg(long)]
    full: bool,

    /// 只说这一趟会干什么，不取也不写
    #[arg(long)]
    dry_run: bool,

    /// 不取 eShop 元数据（名字、发行商、语言），只取那份反查表
    ///
    /// 反查表一份 50 MB 就够认出「哪个游戏的哪个版本」；四个区的元数据加起来
    /// 两三百 MB，换来的是名字与**官中标注**（ADR-0019）
    #[arg(long)]
    no_regions: bool,

    /// 两次请求之间至少隔多少毫秒（按主机计）
    #[arg(long, value_name = "毫秒", default_value_t = 1000)]
    throttle_ms: u64,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct SwitchReadArgs {
    /// 要读的 `.xci` / `.nsp` / `.nsz` / `.xcz`，可给多个
    #[arg(value_name = "文件", required = true)]
    files: Vec<PathBuf>,

    /// 工作目录：有 TitleID 索引就顺手把 ContentId 反查一遍
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 把结果另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,
}

/// 中文离线数据源的几件事。
#[derive(Debug, Subcommand)]
enum ZhCommand {
    /// 取一次中文离线 dump 并建成本机索引。指纹没变就整件跳过
    Sync(ZhSyncArgs),
    /// 拿一个名字试一次模糊匹配——**调参数用的就是它**，一个字节都不联网
    Find(ZhFindArgs),
}

#[derive(Debug, Args)]
struct ZhSyncArgs {
    /// 工作目录：中文索引与取回来的原件存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 无视指纹，整份重建索引
    ///
    /// 改过平台别名之后要它——**原件在手边就不重下那 415 MB**
    #[arg(long)]
    full: bool,

    /// 只说这一趟会干什么，不取也不写
    #[arg(long)]
    dry_run: bool,

    /// 两次请求之间至少隔多少毫秒（按主机计）
    #[arg(long, default_value_t = 1000)]
    throttle_ms: u64,

    #[command(flatten)]
    manifest: ManifestArgs,

    #[command(flatten)]
    rules: NameRulesArgs,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct ZhFindArgs {
    /// 要撞的名字。给的是文件名就先按剥离规则剥一遍
    #[arg(value_name = "名字")]
    names: Vec<String>,

    /// 工作目录：中文索引存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 拿这个平台做交叉校验
    #[arg(long, value_name = "平台")]
    platform: Option<String>,

    /// 拿这个年份做交叉校验
    #[arg(long, value_name = "年份")]
    year: Option<u16>,

    #[command(flatten)]
    tuning: TuningArgs,

    #[command(flatten)]
    rules: NameRulesArgs,
}

/// 模糊匹配的参数。**识别与 `zh find` 共用这一组**——调参数的地方与用参数的地方
/// 写成两套，调出来的值就对不上了。
#[derive(Debug, Args, Clone)]
struct TuningArgs {
    /// 入门相似度：低于它的一条候选都不产出
    #[arg(long, value_name = "0~1")]
    similarity: Option<f64>,

    /// 够得着中置信的相似度（还得平台与年份两道交叉校验都对上）
    #[arg(long, value_name = "0~1")]
    strong_similarity: Option<f64>,

    /// 一条查询最多产出几条候选
    #[arg(long, value_name = "条数")]
    fuzzy_limit: Option<usize>,

    /// 年份差多少之内算对得上
    #[arg(long, value_name = "年数")]
    year_slack: Option<u16>,
}

impl TuningArgs {
    fn tuning(&self) -> zh::Tuning {
        let mut tuning = zh::Tuning::default();
        if let Some(value) = self.similarity {
            tuning.threshold = value;
        }
        if let Some(value) = self.strong_similarity {
            tuning.strong = value;
        }
        if let Some(value) = self.fuzzy_limit {
            tuning.limit = value;
        }
        if let Some(value) = self.year_slack {
            tuning.year_slack = value;
        }
        tuning
    }
}

/// 剥离规则从哪儿来。与 `--manifest` / `--sources` 同一个套路。
#[derive(Debug, Args, Clone)]
struct NameRulesArgs {
    /// 文件名剥离规则的 TOML 文件
    ///
    /// 不给就先看工作目录里有没有 `name-rules.toml`，都没有才用内置的那一份。
    /// 自己那份默认是**补充**内置规则（`"继承内置" = false` 才是整份换掉），
    /// `romcat names --dump-builtin` 能导出内置那份当底稿
    #[arg(long, value_name = "文件")]
    name_rules: Option<PathBuf>,
}

impl NameRulesArgs {
    /// 工作目录里那份可选的规则叫什么。
    const IN_WORKSPACE: &'static str = "name-rules.toml";

    fn load(&self, workspace: &Path) -> Result<Rules, String> {
        let path = match &self.name_rules {
            Some(path) => path.clone(),
            None => {
                let candidate = workspace.join(Self::IN_WORKSPACE);
                if !candidate.exists() {
                    return Ok(Rules::builtin());
                }
                candidate
            }
        };
        Rules::load(&path).map_err(|error| format!("{error}"))
    }
}

#[derive(Debug, Args)]
struct NamesArgs {
    /// 要剥的文件名，可给多个
    #[arg(value_name = "文件名")]
    names: Vec<String>,

    /// 工作目录：不给 `--name-rules` 时来这里找 `name-rules.toml`
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 把**内置**规则写到这个文件，照着它改就是自己的一份
    #[arg(long, value_name = "文件")]
    dump_builtin: Option<PathBuf>,

    /// **把名字还是乱码的那些容器重读一遍**，按新的编码探测解一次
    ///
    /// 只读那几个容器的中央目录（几 KB 一个），只改中立库里名字那两列——
    /// 大小、CRC-32 一个不动。要主库在位，也要 `--library` 或主库根找得到中立库
    #[arg(long)]
    recheck: bool,

    /// 主库根目录。`--recheck` 才用得上；给了 `--library` 就不必再给
    #[arg(long, value_name = "目录")]
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    #[command(flatten)]
    rules: NameRulesArgs,
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

    /// 关掉**文件名那一层**：不拿文件名去撞中文离线数据源
    ///
    /// 这一层零成本、不读盘，但它产出的候选一条都不自动通过、全部进待确认队列。
    /// 只想看前几层的命中率时关掉它
    #[arg(long)]
    no_fuzzy: bool,

    #[command(flatten)]
    rules: NameRulesArgs,

    #[command(flatten)]
    tuning: TuningArgs,

    #[command(flatten)]
    guessing: ModelArgs,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

/// **模型推断兜底**那一层的开关与闸（票 12）。
///
/// 默认**整层关着**：它是识别管线里唯一花钱的一层，而花钱这件事绝不该是默认行为。
/// 缓存里已有的答案不受这个开关影响——那些钱已经付过了，白拿。
#[derive(Debug, Args)]
struct ModelArgs {
    /// 打开**模型推断兜底**：把前面各层全部落空的变体打包问模型
    ///
    /// 它要一套凭据（`ANTHROPIC_API_KEY` 或 `ANTHROPIC_AUTH_TOKEN`），
    /// 读不到就不启动、**绝不匿名试探**。这一层的候选**永不自动通过**，
    /// 一律进待确认队列（ADR-0002）
    #[arg(long)]
    model: bool,

    /// 只算这一趟会问多少、花多少，**一个请求都不发**
    ///
    /// 不需要凭据。花钱之前先看一眼这个
    #[arg(long)]
    model_plan: bool,

    /// 问哪个模型。**必须在价目表里**——不知道 token 值多少钱就设不了花费上限
    #[arg(long, value_name = "名字", default_value = romcat_core::identify::model::DEFAULT_MODEL)]
    model_id: String,

    /// 一个请求里打包几条
    #[arg(long, value_name = "条数", default_value_t = romcat_core::identify::model::DEFAULT_BATCH)]
    model_batch: usize,

    /// 每一条最多要几个猜测
    #[arg(long, value_name = "条数", default_value_t = romcat_core::identify::model::DEFAULT_GUESSES)]
    model_guesses: usize,

    /// 这一趟最多发几个请求
    #[arg(long, value_name = "次数", default_value_t = romcat_core::identify::model::DEFAULT_BUDGET)]
    model_budget: u64,

    /// 这一趟最多花多少美元
    ///
    /// 卡的是**实际花费**：每收到一个响应就按 `usage` 里的真数字加一笔，加到头就停。
    /// 因此最多超出一个请求的顶，而那个顶在计划里印得出来
    #[arg(long, value_name = "美元", default_value = "5.00")]
    model_max_spend: f64,

    /// 一个响应最多多少输出 token。**它同时是花费上界的那一半**
    #[arg(long, value_name = "token", default_value_t = romcat_core::identify::model::DEFAULT_MAX_OUTPUT)]
    model_max_tokens: u64,

    /// 力度：`low` / `medium` / `high` / `xhigh` / `max`
    ///
    /// 力度进**提问指纹**——换一档会自动重问，不必先把缓存清掉
    #[arg(long, value_name = "力度", default_value = romcat_core::identify::model::DEFAULT_EFFORT)]
    model_effort: String,

    /// 两次请求之间至少隔多少毫秒（下限 200）
    #[arg(long, value_name = "毫秒", default_value_t = 1000)]
    model_interval_ms: u64,

    /// 换一份价目表
    #[arg(long, value_name = "文件")]
    model_pricing: Option<PathBuf>,

    /// 把内置价目表导出成一份底稿，照着改
    #[arg(long, value_name = "文件")]
    model_dump_pricing: Option<PathBuf>,
}

impl ModelArgs {
    /// 请求间隔的**下限**：默认值挡不住 `--model-interval-ms 0`，
    /// 而「频率有明确上限」说的正是挡得住。
    const MIN_INTERVAL_MS: u64 = 200;

    /// 折成库那一层的闸。
    fn limits(&self) -> romcat_core::identify::model::Limits {
        romcat_core::identify::model::Limits {
            batch: self.model_batch.max(1),
            guesses: self.model_guesses.max(1),
            budget: self.model_budget,
            // 美元折成微美元。上限是整数运算的输入，从这里就折干净。
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            spend_cap_micros: (self.model_max_spend.max(0.0) * 1_000_000.0) as u64,
            max_output_tokens: self.model_max_tokens,
            effort: self.model_effort.clone(),
            interval: Duration::from_millis(self.model_interval_ms.max(Self::MIN_INTERVAL_MS)),
            backoff: romcat_core::identify::model::BACKOFF,
        }
    }
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

    /// 策略档案：`离线`（默认，一个网络请求都不发）或 `在线`（再加联网源补**封面**与开发商）
    ///
    /// 在线档要一套 ScreenScraper 凭据，从环境变量读。它默认限流，且把配额超限
    /// 当作硬停止——配额同时按账号与 IP 计，撞穿了会被永久封禁
    #[arg(long, value_name = "档案", default_value = "离线")]
    profile: String,

    /// 在线档这一趟最多发多少个请求。**这是自己设的保守闸，不是服务端的配额**
    #[arg(long, value_name = "个数")]
    online_budget: Option<u64>,

    /// 在线档两次请求之间至少隔多少毫秒
    #[arg(long, value_name = "毫秒")]
    online_interval_ms: Option<u64>,

    /// 字段级优先级表
    ///
    /// 不给就先看工作目录里有没有 `priorities.toml`，都没有才用内置的那一份
    #[arg(long, value_name = "文件")]
    priorities: Option<PathBuf>,

    /// 把**内置**优先级表写到这个文件，照着它改就是自己的一份
    #[arg(long, value_name = "文件")]
    dump_priorities: Option<PathBuf>,

    #[command(flatten)]
    rules: NameRulesArgs,

    #[command(flatten)]
    tuning: TuningArgs,

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
    fn profile(&self) -> Result<scrape::Profile, String> {
        scrape::Profile::from_label(&self.profile).ok_or_else(|| {
            format!(
                "不认得策略档案「{}」。有两套：`离线`（只用本地数据源）与 `在线`。",
                self.profile
            )
        })
    }

    /// 命令行给的频率**有下限**。
    ///
    /// 默认值只是默认值，挡不住 `--online-interval-ms 0`——而「并发与频率有明确上限」
    /// 说的正是**挡得住**。库里那一层允许 0（测试要跑得动），产品这一层不允许：
    /// 真正会去打 ScreenScraper 的只有这一条路。
    const MIN_INTERVAL: Duration = Duration::from_millis(200);

    fn limits(&self) -> scrape::online::Limits {
        let mut limits = scrape::online::Limits::default();
        if let Some(budget) = self.online_budget {
            limits.budget = budget;
        }
        if let Some(ms) = self.online_interval_ms {
            limits.interval = Duration::from_millis(ms).max(Self::MIN_INTERVAL);
        }
        limits
    }

    fn load_priorities(&self, workspace: &Path) -> Result<Priorities, String> {
        load_priorities(self.priorities.as_deref(), workspace)
    }
}

/// 挑出这一趟用哪一份优先级表：命令行给的 > 工作目录里那份 > 内置的。
///
/// `scrape`、`titles` 与同步共用它，**而且必须共用**：几条路对「哪个源说了算」的答案
/// 不一样的话，报告里合并出来的标题与导出时挑出来的标题就会对不上。
/// 算法在核心里（`sync::prepare::priorities`），界面走的是同一个。
fn load_priorities(given: Option<&Path>, workspace: &Path) -> Result<Priorities, String> {
    sync::prepare::priorities(given, workspace)
}

/// `romcat titles` 的参数。
///
/// 它**一个字节都不读主库、一个请求都不发**：标题集合是从中立库里已有的识别与刮削
/// 结论折出来的（ADR-0001）。所以这里没有主库根之外的任何东西——那个也只是用来
/// 找到对应的中立库。
#[derive(Debug, Args)]
struct TitlesArgs {
    /// 主库根目录。只用来找到对应的中立库，不会去读它；给了 `--library` 就不必再给
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    /// 字段级优先级表。同一部作品同一档里有好几个叫法时，它定源的先后
    ///
    /// 不给就先看工作目录里有没有 `priorities.toml`，都没有才用内置的那一份
    #[arg(long, value_name = "文件")]
    priorities: Option<PathBuf>,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

/// 不指名时用哪个前端格式。**一处定死**：`--format` 的默认值与新建子库时落库的那个
/// 必须是同一个词，各写一遍的话哪天加了第二个适配器就会有一处忘了改。
const DEFAULT_FORMAT: &str = "Pegasus";

/// 用哪个适配器。两个子命令共用。
#[derive(Debug, Args, Clone)]
struct FormatArgs {
    /// 前端格式（`romcat adapters` 列得出有哪些）
    #[arg(long, value_name = "格式", default_value = DEFAULT_FORMAT)]
    format: String,
}

impl FormatArgs {
    fn load(&self) -> Result<Box<dyn Adapter>, String> {
        adapter::find(&self.format).ok_or_else(|| {
            let names: Vec<&str> = adapter::all().iter().map(|a| a.name()).collect();
            format!(
                "没有叫「{}」的适配器。眼下带的是：{}。",
                self.format,
                names.join("、")
            )
        })
    }
}

/// `romcat import` 的参数。
///
/// **它一个字节都不改那些文件**：读进来、逐字节存下快照、把手工维护的值落进中立库。
#[derive(Debug, Args)]
struct ImportArgs {
    /// 要导入的元数据文件，可以给多个
    #[arg(required = true, value_name = "文件")]
    files: Vec<PathBuf>,

    /// 主库根目录。`file:` 里的路径要靠它折成变体的键
    #[arg(long, value_name = "目录")]
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    #[command(flatten)]
    format: FormatArgs,

    /// 只跑往返实测、报出档位与会留下什么，**不写中立库**
    #[arg(long)]
    dry_run: bool,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

/// `romcat export` 的参数。
#[derive(Debug, Args)]
struct ExportArgs {
    /// 写到哪个目录
    ///
    /// 这个目录在语义上就是**主库根的替身**：文件里的 `file:` 是相对元数据文件所在
    /// 目录解析的，而中立库的键是相对主库根的。把导出来的这几份文件放到主库根下，
    /// 路径直接就对
    #[arg(long, value_name = "目录")]
    out: PathBuf,

    /// 主库根目录。只用来找到对应的中立库，不会去读它；给了 `--library` 就不必再给
    #[arg(long, value_name = "目录")]
    root: Option<PathBuf>,

    /// 按名字找中立库
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,

    #[command(flatten)]
    format: FormatArgs,

    /// 字段级优先级表
    #[arg(long, value_name = "文件")]
    priorities: Option<PathBuf>,

    /// **裁决**：这个变体是它那个作品在那个平台上的首选变体，压过「汉化 > 官中 > 日版」
    /// 那条规则。可重复给，记进中立库、永久生效
    #[arg(long = "prefer", value_name = "变体的键")]
    prefer: Vec<String>,

    /// 只排计划、报出这一趟会写什么，不写盘
    #[arg(long)]
    dry_run: bool,

    /// 外面有人动过也照写。**这会丢掉那次手改**
    #[arg(long)]
    force: bool,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
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

    /// 一个透明容器都不读：不去读容器内部的 CRC-32、大小与名字
    ///
    /// 库里 91% 的容量在透明容器里，关掉它报告就只知道「这里有一个 3GB 的容器」，看不见里面装着什么。
    /// 它盖过 `--zst`：这一条说的是「一个都别读」
    #[arg(long)]
    no_containers: bool,

    /// 也读 zst 与 tar.zst 的内部构成。**这一趟会慢几个小时**
    ///
    /// zip 与 7z 的内部清单零解压就在容器头里写着；zstd 没有这个东西，列全清单只能把
    /// 整条流解一遍。主库里这是 2,685 个文件、2.50 TiB，真机实测约 125 MB/s，一趟约 5.8 小时
    /// （瓶颈全在磁盘）。打开一次即可：结论按 (路径, 大小, 修改时间) 落进中立库，
    /// 往后的扫描原样沿用，实测二次扫描 1.0 秒
    #[arg(long)]
    zst: bool,

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
        Command::Titles(args) => run_titles(&args),
        Command::Import(args) => run_import(&args),
        Command::Export(args) => run_export(&args),
        Command::Adapters => run_adapters(),
        Command::Platforms(args) => run_platforms(&args),
        Command::Capability(args) => run_capability(&args),
        Command::Triage(TriageCommand::List(args)) => run_triage_list(&args),
        Command::Triage(TriageCommand::Show(args)) => run_triage_show(&args),
        Command::Triage(TriageCommand::Decide(args)) => run_triage_decide(&args),
        Command::Triage(TriageCommand::Undo(args)) => run_triage_undo(&args),
        Command::Triage(TriageCommand::Export(args)) => run_triage_export(&args),
        Command::Triage(TriageCommand::Import(args)) => run_triage_import(&args),
        Command::Sublibrary(SublibraryCommand::Set(args)) => run_sublibrary_set(&args),
        Command::Sublibrary(SublibraryCommand::List(args)) => run_sublibrary_list(&args),
        Command::Sublibrary(SublibraryCommand::Remove(args)) => run_sublibrary_remove(&args),
        Command::Sublibrary(SublibraryCommand::Rule(args)) => run_sublibrary_rule(&args),
        Command::Sublibrary(SublibraryCommand::Except(args)) => run_sublibrary_except(&args),
        Command::Sublibrary(SublibraryCommand::Show(args)) => run_sublibrary_show(&args),
        Command::Sublibrary(SublibraryCommand::Plan(args)) => run_sublibrary_plan(&args),
        Command::Sublibrary(SublibraryCommand::Sync(args)) => run_sublibrary_sync(&args, &cancel),
        Command::Dat(DatCommand::Sync(args)) => run_dat_sync(&args),
        Command::Dat(DatCommand::Report(args)) => run_dat_report(&args),
        Command::Dat(DatCommand::Sources(args)) => run_dat_sources(&args),
        Command::Zh(ZhCommand::Sync(args)) => run_zh_sync(&args),
        Command::Zh(ZhCommand::Find(args)) => run_zh_find(&args),
        Command::Switch(SwitchCommand::Sync(args)) => run_switch_sync(&args),
        Command::Switch(SwitchCommand::Read(args)) => run_switch_read(&args),
        Command::Names(args) => run_names(&args, &cancel),
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

/// 本机那份中文索引，打开（必要时**就地重建**）但不装进内存。
///
/// **没取过数不是错误**——识别照跑，少一层而已，所以库不在位时返回 `None`。
///
/// 与 [`load_zh_index`] 分成两步，是因为刮削那一趟**两样都要**：撞名字要内存里那份
/// 索引，取**简介**要留着这个库句柄按条目号点着读（`scrape::zh::Summaries`）。
fn open_zh_store(workspace: &Path, rules: &Rules) -> Result<Option<zh::store::Store>, String> {
    let path = workspace::zh_store_path(workspace);
    if !path.exists() {
        return Ok(None);
    }
    let mut store =
        zh::store::Store::open(&path).map_err(|error| format!("中文索引打不开：{error}"))?;
    heal_zh_store(workspace, &mut store, rules, None);
    Ok(Some(store))
}

/// 本机那份中文离线索引，装进内存。**没取过数不是错误**——识别照跑，少一层而已。
fn load_zh_index(workspace: &Path, rules: &Rules) -> Result<Option<zh::Index>, String> {
    let Some(store) = open_zh_store(workspace, rules)? else {
        return Ok(None);
    };
    let index = store
        .load()
        .map_err(|error| format!("中文索引读不出来：{error}"))?;
    Ok((!index.is_empty()).then_some(index))
}

/// 结构版本对不上时**从本机那份原件重建**：一个网络请求都不发，也不要用户重下 435 MB。
///
/// 三个用得着中文索引的子命令都走这一条。重建要把数据源写的平台名折成本工具的平台名，
/// 所以要一份**平台清单**：`zh sync` 手上有它自己那份（`--manifest` 指得了），传进来；
/// `identify` 与 `scrape` 的参数表里本来就没有这个开关，那时按 `ManifestArgs` 的老规矩
/// 解析——工作目录里那份 `platforms.toml`，没有就用内置的。
///
/// **重建不成不是错误，只是少一层。** 缓存里那个 zip 截断了、读不动了，都不该让
/// `romcat identify` 整条命令跑不起来——那与 `load_zh_index` 自己的契约（「没取过数不是
/// 错误，识别照跑」）直接相抵。何况这时旧索引还原样躺着，什么都没丢
/// （`zh::store` 的「旧数据一直留到新数据真的写进来那一刻」）。
fn heal_zh_store(
    workspace: &Path,
    store: &mut zh::store::Store,
    rules: &Rules,
    manifest: Option<&Manifest>,
) {
    let Some(pending) = store.rebuilding().map(|it| it.was) else {
        return;
    };
    let fallback = match manifest {
        Some(_) => None,
        None => match (ManifestArgs { manifest: None }).load(workspace) {
            Ok(manifest) => Some(manifest),
            Err(message) => {
                eprintln!("中文索引要重建，但平台清单读不出来：{message}。这一趟先少这一层。");
                return;
            }
        },
    };
    let Some(manifest) = manifest.or(fallback.as_ref()) else {
        return;
    };
    let outcome = zh::sync::rebuild(
        &RealFs,
        store,
        manifest,
        rules,
        &workspace::zh_cache_dir(workspace),
    );
    match outcome {
        Ok(zh::sync::Rebuilt::NotNeeded) => {}
        Ok(zh::sync::Rebuilt::Done { was, dump, games }) => eprintln!(
            "中文索引的结构版本是 {was}，本程序认得的是 {}——已从本机那份原件 {dump} \
             就地重建，{} 条游戏条目，没下载任何东西。",
            zh::store::SCHEMA_VERSION,
            thousands(games),
        ),
        Ok(zh::sync::Rebuilt::NoOriginal { was, dump }) => eprintln!(
            "中文索引的结构版本是 {was}，本程序认得的是 {}，而本机缓存里找不到原件{}——\
             跑一次 `romcat zh sync` 就补回来了。这一趟先少这一层。",
            zh::store::SCHEMA_VERSION,
            if dump.is_empty() {
                String::new()
            } else {
                format!("（{dump}）")
            },
        ),
        Err(error) => eprintln!(
            "中文索引的结构版本是 {pending}，本程序认得的是 {}，而就地重建没成：{error}。\
             旧索引原样留着，什么都没丢；跑一次 `romcat zh sync` 就补回来了。\
             这一趟先少这一层。",
            zh::store::SCHEMA_VERSION,
        ),
    }
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
    options.decompress_zst = args.zst;
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
    // 价目表底稿：与 `--dump-priorities` / `names --dump-builtin` 同一个套路。
    if let Some(path) = &args.guessing.model_dump_pricing {
        match write_file(path, model::Pricing::BUILTIN.as_bytes()) {
            Ok(()) => {
                println!(
                    "内置价目表已写入 {}。改完用 `--model-pricing {}` 生效。",
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
    // **沉淀库先说话**：裁决过的内容直接精确命中，不再进队列（ADR-0008）。
    // 它不跟中立库走，删掉中立库重扫也不会丢这些裁决。
    let store = match open_store(&workspace) {
        Ok(store) => store,
        Err(message) => return fail(message),
    };
    let verdicts = match verdict::Index::load(&store, &slug.text()) {
        Ok(index) => index,
        Err(error) => return fail(format!("沉淀库读不动：{error}")),
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

    // **文件名那一层**（票 11）：剥离规则加中文离线索引。取过数才跑得起来——
    // 没取过就如实说一句，识别照跑，只是少一层。
    let rules = match args.rules.load(&workspace) {
        Ok(rules) => rules,
        Err(message) => return fail(message),
    };
    let index = if args.no_fuzzy {
        None
    } else {
        match load_zh_index(&workspace, &rules) {
            Ok(index) => index,
            Err(message) => return fail(message),
        }
    };
    let naming = identify::fuzzy::Naming {
        rules: &rules,
        index: index.as_ref(),
        tuning: args.tuning.tuning(),
    };
    if args.no_fuzzy {
        eprintln!("文件名那一层关掉了（--no-fuzzy）。");
    } else if let Some(index) = index.as_ref() {
        eprintln!(
            "文件名那一层：中文离线数据源 {} 条条目（dump {}），相似度门槛 {:.2}。",
            thousands(index.len() as u64),
            index.dump(),
            naming.tuning.threshold,
        );
    } else {
        eprintln!("还没取过中文离线数据源，文件名那一层不跑。要它就先跑一次 `romcat zh sync`。");
    }

    // **第三方 TitleID 数据库**（票 27）：Switch 那一层拿它把 ContentId 反查成
    // (TitleID, 版本)。**没取过照样跑**——容器的明文文件名表免密钥就说得出 TitleID，
    // 查表只是把结论从「哪个游戏」抬到「哪个游戏的哪个版本」。
    let titledb = titledb::store::Store::open(&workspace::titledb_store_path(&workspace)).ok();
    let titledb = titledb.filter(|store| store.ready().unwrap_or(false));
    match &titledb {
        Some(store) => match store.stats() {
            Ok(stats) => eprintln!(
                "Switch 那一层：TitleID 索引 {} 条 ContentId、{} 个 TitleID（其中官中 {}）。",
                thousands(stats.ncas),
                thousands(stats.titles),
                thousands(stats.chinese),
            ),
            Err(error) => return fail(format!("TitleID 索引读不动：{error}")),
        },
        None => eprintln!(
            "还没取过 TitleID 索引，Switch 那一层只读得出 TitleID、读不出版本。\
             要它就先跑一次 `romcat switch sync`。"
        ),
    }

    // **模型推断兜底**（票 12）：默认整层关着。缓存里已有的答案不受这个开关影响——
    // 那些钱已经付过了，白拿；只有「真的再问一次」才要 `--model`。
    let answers = match catalog.model_answers() {
        Ok(rows) => model::Answers::build(rows),
        Err(error) => return fail(format!("问过的答案读不动：{error}")),
    };
    let pricing = match args.guessing.model_pricing.as_deref() {
        Some(path) => match model::Pricing::load(path) {
            Ok(pricing) => pricing,
            Err(error) => return fail(format!("{error}")),
        },
        None => model::Pricing::builtin(),
    };
    // **价目表里没有这个模型就整层不启动**：不知道 token 值多少钱，
    // 「花费上限」就是一句空话（`pricing.toml` 开头写的就是这件事）。
    let price = match pricing.price(&args.guessing.model_id) {
        Some(price) => price,
        None => {
            // 拒的判据是「这一层这一趟会不会做事」，不只是「要不要发请求」：
            // 库里存着问过的答案时它照样会算一份计划，而一份用 0 价算出来的计划
            // 会印出「花费 0.00 美元」——那比不印还糟。
            if args.guessing.model || args.guessing.model_plan || !answers.is_empty() {
                return fail(format!(
                    "价目表里没有模型「{}」，于是花费上限设不了，这一层不启动。\
                     表里有的是：{}。用 --model-pricing 换一份，\
                     或者 --model-dump-pricing 导一份底稿照着加。",
                    args.guessing.model_id,
                    pricing.known().join("、"),
                ));
            }
            model::Price {
                input_per_mtok: 0,
                output_per_mtok: 0,
            }
        }
    };
    let limits = args.guessing.limits();
    // 凭据读不到就不启动，**绝不匿名试探**（票 14 同一条纪律）。
    //
    // **`--model-plan` 压过 `--model`**：那个开关的帮助文本写着「一个请求都不发」，
    // 两个同给时若照发不误，就是帮助文本在撒谎——而它撒的谎正好是花钱那一头。
    let credentials = if args.guessing.model && !args.guessing.model_plan {
        match model::Credentials::from_env() {
            Some(credentials) => Some(credentials),
            None => {
                return fail(format!(
                    "模型推断这一层要一套凭据，{} 两个环境变量都没有设。\
                     **不匿名试探**——没有凭据的请求必然被拒，那一发对谁都没有好处。\
                     只想看看要花多少钱就用 --model-plan，它一个请求都不发。",
                    model::ENV_KEYS.join(" 或 "),
                ));
            }
        }
    } else {
        None
    };
    let fetcher = HttpFetcher::new();
    let net = credentials.map(|credentials| {
        model::Inference::new(&fetcher, limits.clone(), price, credentials, cancel)
    });
    // 计划**在第一个请求发出去之前**就摆出来（不是跑完之后才印）。
    let announce = |plan: &model::Plan| eprintln!("{}", render_plan(plan));
    let guessing = model::Guessing {
        answers: &answers,
        net: net.as_ref(),
        announce: Some(&announce),
        planning: args.guessing.model_plan,
        model: args.guessing.model_id.clone(),
        price,
        checked: pricing.checked().to_string(),
        limits,
    };
    if guessing.asking() {
        eprintln!(
            "模型推断兜底**开着**（{}，力度 {}，一批 {} 条，最多 {} 个请求 / {}）。\
             价目表核实于 {}（{}）。这一层的候选**永不自动通过**，全部进待确认队列。",
            guessing.model,
            guessing.limits.effort,
            guessing.limits.batch,
            guessing.limits.budget,
            model::dollars(guessing.limits.spend_cap_micros),
            pricing.checked(),
            pricing.cite(),
        );
    } else if args.guessing.model_plan {
        if args.guessing.model {
            eprintln!(
                "--model 与 --model-plan 同给，按 --model-plan 办：只算计划，一个请求都不发。"
            );
        }
        eprintln!("只算计划（--model-plan），一个请求都不发。");
    } else if !answers.is_empty() {
        eprintln!(
            "模型推断兜底关着，但库里存着 {} 条问过的答案——它们照旧折成候选，不花一分钱。",
            thousands(u64::try_from(answers.len()).unwrap_or(0)),
        );
    }

    let library = RealFs::new();
    let started = Instant::now();
    let mut last = Instant::now();
    let outcome = identify::run(
        &library,
        &mut catalog,
        &identify::Ammo {
            repo: &repo,
            verdicts: &verdicts,
            naming: &naming,
            guessing: &guessing,
            titledb: titledb.as_ref(),
        },
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
    if outcome.probed > 0 || outcome.missed > 0 {
        eprintln!(
            "光盘那一层探了 {} 份内容（读出标识 {} 份，另有 {} 份这一趟没读到），撞出 {} 条候选，其中 {} 个变体是**只靠序列号**才认出来的。",
            thousands(outcome.probed),
            thousands(outcome.with_id),
            thousands(outcome.missed),
            thousands(outcome.serial_candidates),
            thousands(outcome.serial_only),
        );
    }
    if outcome.cart.probed > 0 {
        eprintln!(
            "卡带那一层探了 {} 份内容（读出游戏码 {} 份，另有 {} 份这一趟没读到），\
             撞出 {} 条候选，其中 {} 个变体是**只靠内部头**才认出来的；\
             内部头与目录声明的平台对不上的有 {} 份。",
            thousands(outcome.cart.probed),
            thousands(outcome.cart.with_id),
            thousands(outcome.cart.missed),
            thousands(outcome.cart.candidates),
            thousands(outcome.cart.only),
            thousands(outcome.cart.conflicts),
        );
    }
    if outcome.sha1_hits > 0 {
        eprintln!(
            "SHA-1 那条窄路撞出 {} 条候选（第一命中层够不到的那批记录），\
             其中 {} 个变体是靠它才认出来的。",
            thousands(outcome.sha1_hits),
            thousands(outcome.sha1_only),
        );
    }
    if outcome.fuzzy.variants > 0 {
        eprintln!(
            "文件名那一层为 {} 个变体撞了 {} 串字，产出 {} 条候选（其中 {} 条两道交叉校验都对上、\
             够得着中置信），{} 个变体是**只靠它**才有候选的。\
             **这一层的候选一条都不自动通过**，全部进待确认队列。",
            thousands(outcome.fuzzy.variants),
            thousands(outcome.fuzzy.tried),
            thousands(outcome.fuzzy.candidates),
            thousands(outcome.fuzzy.strong),
            thousands(outcome.fuzzy.only),
        );
        if outcome.fuzzy.garbled > 0 {
            eprintln!(
                "  ⚠ 另有 {} 个名字**还是乱码**没敢撞（票 03 有损转换留下的）。\
                 容器名字现在会先探编码再解码，重扫一遍容器就好了。",
                thousands(outcome.fuzzy.garbled),
            );
        }
        if !outcome.unknown_marks.is_empty() {
            let top: Vec<String> = outcome
                .unknown_marks
                .iter()
                .take(12)
                .map(|(mark, count)| format!("{mark}×{count}"))
                .collect();
            eprintln!(
                "  剥离规则认不出的记号（照着往 name-rules.toml 里补，补完重跑一遍就看得见效果）：{}",
                top.join("、"),
            );
        }
    }
    if outcome.switch.probed > 0 {
        eprintln!(
            "Switch 那一层读了 {} 份容器的**明文文件名表**（一个密钥都没用）：\
             {} 份的 `.tik` 说出了 TitleID，{} 份靠 ContentId 反查出了精确版本，\
             {} 个变体是**只靠它**才有候选的。",
            thousands(outcome.switch.probed),
            thousands(outcome.switch.ticketed),
            thousands(outcome.switch.resolved),
            thousands(outcome.switch_only),
        );
        if outcome.switch.compressed > 0 {
            eprintln!(
                "  其中 {} 份是 `.nsz` / `.xcz`——识别不受影响（零解压），\
                 但 **ES-DE 的扩展名表里没有它们**，那个前端里默认看不见（ADR-0017）。",
                thousands(outcome.switch.compressed),
            );
        }
        if outcome.switch.multi > 0 {
            eprintln!(
                "  另有 {} 份容器里装着不止一个 TitleID（合集卡带），哪一个代表那个变体归裁决。",
                thousands(outcome.switch.multi),
            );
        }
        if outcome.switch.name_conflicts > 0 {
            eprintln!(
                "  ⚠ {} 条候选上，**文件名里写的 TitleID 与容器里读出来的对不上**——\
                 那是极强的「文件被改过 / 命名错误」信号，逐条在依据里点了名。",
                thousands(outcome.switch.name_conflicts),
            );
        }
    }
    if outcome.from_verdicts > 0 {
        eprintln!(
            "其中 {} 个变体的结论直接来自**沉淀库**（{} 条裁决）——裁决过的东西不必再撞一遍 DAT。",
            thousands(outcome.from_verdicts),
            thousands(u64::try_from(verdicts.len()).unwrap_or(0)),
        );
    }
    report_model(&outcome.model, &catalog);
    if !write_json(args.json.as_deref(), &outcome.report) {
        return ExitCode::FAILURE;
    }
    if outcome.interrupted {
        eprintln!("这一趟被中断了，已经算完的那部分留在中立库里，重跑会从头算一遍。");
        return ExitCode::from(130);
    }
    // **停下来不是错误**（票 14 那条纪律）：已经问到的答案都落库了，报告照常出。
    // 但退出码要分得出「立刻重跑就接着问」与「重跑没用，先去解决它」——
    // 这两件事在一层**花钱**的管线上代价差得很远。
    match outcome.model.halt {
        Some(identify::model::HaltKind::Self_) => ExitCode::from(4),
        Some(identify::model::HaltKind::Refused) => ExitCode::from(3),
        None => ExitCode::SUCCESS,
    }
}

/// 把一份[计划](model::Plan)写成给人看的一段。
///
/// **两处共用**：一处在第一个请求发出去之前（那是「花费可预估」真正要的时刻），
/// 一处在收尾的报告里。各写一遍的话，两处早晚会说出不一样的数。
fn render_plan(plan: &model::Plan) -> String {
    use romcat_core::identify::model::dollars;
    let mut out = format!(
        "  计划（第一个请求发出去之前就算得出）：待问 {} 个变体，打成 {} 个请求，\
         上限卡下来真发 {} 个；输入约 {} token（估的）。\
         花费 **{} 到 {}**——上界是硬的，一个响应的输出不可能超过 max_tokens。\
         一个请求的顶 {}（花费上限最多超出这么多）。价目表核实于 {}。",
        thousands(plan.variants),
        thousands(plan.batches),
        thousands(plan.requests),
        thousands(plan.input_tokens),
        dollars(plan.floor_micros),
        dollars(plan.ceiling_micros),
        dollars(plan.ceiling_per_request_micros),
        plan.priced_at,
    );
    // **花钱之前不只要看见数字，还要看见问的是什么。** 上下文拼错了在数字上一点都
    // 看不出来，在这一段上一眼就看得出来。
    if let Some(sample) = &plan.sample {
        out.push_str("\n  第一条问题长这样（一批里的头一条）：\n");
        out.push_str(sample);
    }
    out
}

/// 把**模型推断那一层**这一趟干了什么打出来。
///
/// 三样东西一样都不能省：**残渣有多少**（这一层的分母）、**这一趟花了多少**
/// （赌的是用户的钱，必须看得见）、以及**这一层的候选永不自动通过**（ADR-0002，
/// 队列里将要出现几千条模型编的东西，报告不说清就是在骗人）。
fn report_model(count: &identify::model::ModelCount, catalog: &Catalog) {
    use romcat_core::identify::model::dollars;
    if count.residue == 0 {
        return;
    }
    eprintln!(
        "模型推断兜底：**前面各层全部落空**的变体有 {} 个（一条候选都没有）——\
         其中 {} 个直接从缓存里取到答案（这一趟一分钱没花），{} 个这一趟真问了。",
        thousands(count.residue),
        thousands(count.from_cache),
        thousands(count.asked),
    );
    if count.usage.requests > 0 {
        eprintln!(
            "  实际：发了 {} 个请求（**一个请求装 {} 条**，不是一条一发），\
             输入 {} token、输出 {} token，花了 **{}**。\
             估的输入是 {} token，估偏了 {:+.0}%。",
            thousands(count.usage.requests),
            thousands(count.batch_size),
            thousands(count.usage.input_tokens),
            thousands(count.usage.output_tokens),
            dollars(count.usage.cost_micros),
            thousands(count.estimated_input_tokens),
            count.estimate_skew(),
        );
    }
    eprintln!(
        "  产出 {} 条候选，涉及 {} 个变体；另有 {} 个变体**模型自己也说不出**\
         （那批模拟器、存档、主题包本来就不是游戏——留空比编一个名字好）。\
         **这一层的候选一条都不自动通过**，全部进待确认队列（ADR-0002），\
         每一条的依据第一句就写着它是模型推断的。",
        thousands(count.candidates),
        thousands(count.with_candidates),
        thousands(count.speechless),
    );
    if let Ok((calls, micros)) = catalog.model_spend() {
        eprintln!(
            "  这个库上这一层**一共**发过 {} 个请求、花了 {}。",
            thousands(calls),
            dollars(micros),
        );
    }
    if count.unanswered() > 0 {
        eprintln!(
            "  ⚠ 另有 {} 个问出去了却一个字都没答回来（一批被截断，或者响应的形状认不出来）。\
             钱已经付了，而它们下一趟会被再问一遍——`--model-batch` 调小或者
             `--model-max-tokens` 调大再来。",
            thousands(count.unanswered()),
        );
    }
    if let Some(halted) = &count.halted {
        eprintln!("  ⚠ {halted}");
    }
}

/// 在识别结论上取元数据与媒体。**离线档一个网络请求都不发。**
///
/// 读盘只发生在一处：把主库里现成的图与视频收进**媒体池**。`--no-media` 把那一处也
/// 关掉，于是一个字节都不读主库。
///
/// ## 三种退出
///
/// | 退出码 | 什么情况 | 已经采到的东西 | 马上重跑有用吗 |
/// |---|---|---|---|
/// | 0 | 跑完了 | 在库里 | 不必 |
/// | 1 | **没跑起来**：参数不对、库不在、缺凭据 | —— | 先改参数 |
/// | 3 | **对面不让了**：配额超限、凭据被拒、被拉黑、网断了 | **在库里** | **没用**；配额那条今天都别再来 |
/// | 4 | **自己收的手**：`--online-budget` 用完了 | **在库里** | 有用，接着采 |
/// | 130 | Ctrl-C | 在库里 | 有用 |
///
/// 3 与 4 分开，是因为脚本要照着它决定「循环着跑到完」还是「今天到此为止」——
/// 而这两件事在 ScreenScraper 上的代价差得很远：接着打的那一条通向永久封禁。
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

    let profile = match args.profile() {
        Ok(profile) => profile,
        Err(message) => return fail(message),
    };
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
    options.profile = profile;
    options.media = !args.no_media;
    options.max_media_bytes = args.max_media_mib.map(|mib| mib.saturating_mul(1 << 20));
    options.refresh = args.refresh;

    // **在线档的网络句柄只在这一档造出来。** 传输层也按同一个间隔限一次流——
    // 上面那道闸管的是「一趟发几个」，这一道管的是「打得多快」。
    let limits = args.limits();
    let online = profile == scrape::Profile::Online;
    let credentials = match (online, scrape::online::Credentials::from_env()) {
        (false, _) => None,
        (true, Some(credentials)) => Some(credentials),
        // **宁可不启动也不匿名试探**：没有 devid 的请求直接 403，而那是白扣一次的。
        (true, None) => {
            return fail(format!(
                "在线档要一套 ScreenScraper 凭据，从环境变量读：{}。\n\
                 devid / devpassword 要在 ScreenScraper 的论坛人工申请（无 devid 直接 403），\n\
                 **不要拿别人的 devid 用**——那会连累对方被拉黑（426）。",
                scrape::online::ENV_KEYS.join(" / ")
            ));
        }
    };
    let fetcher = online.then(|| HttpFetcher::with_throttle(limits.interval));
    let net = match (fetcher.as_ref(), credentials) {
        (Some(fetcher), Some(credentials)) => {
            eprintln!(
                "在线档：并发上限 {}，两次请求之间至少 {} 毫秒，这一趟最多 {} 个请求。\n\
                 只对**已确认**的条目发请求；配额超限会当场停下，不重试也不换账号。",
                scrape::online::MAX_CONCURRENCY,
                limits.interval.as_millis(),
                thousands(limits.budget),
            );
            Some(scrape::online::Net::new(
                fetcher,
                limits,
                credentials,
                cancel,
            ))
        }
        _ => None,
    };

    // **中文离线源**（票 11）：取过数才参加，没取过就少一个源，别的照跑。
    let rules = match args.rules.load(&workspace) {
        Ok(rules) => rules,
        Err(message) => return fail(message),
    };
    // **库句柄留着别扔**：撞名字走内存里那份索引，取**简介**要按条目号回库里点名
    // （票 03，`scrape::zh::Summaries`）——简介不跟着索引进内存，那是九十来 MB 常驻。
    let store = match open_zh_store(&workspace, &rules) {
        Ok(store) => store,
        Err(message) => return fail(message),
    };
    let index = match store.as_ref().map(zh::store::Store::load).transpose() {
        Ok(index) => index.filter(|index: &zh::Index| !index.is_empty()),
        Err(error) => return fail(format!("中文索引读不出来：{error}")),
    };
    let naming = identify::fuzzy::Naming {
        rules: &rules,
        index: index.as_ref(),
        tuning: args.tuning.tuning(),
    };
    // 索引空着的时候这个源整个不参加，简介那条路也就无从谈起——两边跟着同一个判据走。
    let summaries: Option<&dyn scrape::zh::Summaries> = index
        .as_ref()
        .and(store.as_ref())
        .map(|store| store as &dyn scrape::zh::Summaries);
    if let Some(index) = index.as_ref() {
        eprintln!(
            "中文离线源：{} 条条目（dump {}）——只给两道交叉校验都对上的那一档中文名，\
             撞上的那条条目的中文简介一并取回（最长 {} 字，超出的截断并在报告里点名）。",
            thousands(index.len() as u64),
            index.dump(),
            thousands(scrape::zh::DESCRIPTION_LIMIT as u64),
        );
    }

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
        net.as_ref(),
        &mut scrape::RunContext {
            cancel,
            progress: &mut progress,
            naming: &naming,
            summaries,
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
            "有 {} 个「锚点 × 源」这次无话可说，上一轮的结论已清掉——\n\
             那些值背后的 DAT 条目不在了，留着只会带出一条对不上的依据。",
            thousands(outcome.forgotten)
        );
    }
    if outcome.missing_media > 0 {
        eprintln!(
            "有 {} 份媒体源说它没有，记下了——下一趟不会为同一张不存在的图再花一份配额。",
            thousands(outcome.missing_media)
        );
    }
    if outcome.not_media > 0 {
        eprintln!(
            "有 {} 份被认领的媒体，扩展名这一层却不认得——本地媒体源与媒体池的扩展名表\n\
             对不上了，这是要查的。",
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
    if outcome.skipped > 0 {
        eprintln!(
            "有 {} 个「锚点 × 源」这次没采成，**一个字都没写库**——重跑会再来一次。\n\
             头一个是：{}",
            thousands(outcome.skipped),
            outcome.first_skip.as_deref().unwrap_or("（没说）"),
        );
    }
    if let Some(online) = &outcome.report.online
        && online.refused > 0
    {
        eprintln!(
            "有 {} 个 URL 被取数闸门拦下：源给回来的地址在白名单之外。这是要看一眼的。",
            thousands(online.refused)
        );
    }
    // 标题这一层不在刮削里：**刮削给值，`titles` 把值折成标题集合**再挑显示标题——
    // 那一步要的还有识别那一侧的地区与中文记号，而且换一份优先级表就该重挑一次，
    // 不必重采。不说一句的话，用户没有任何途径知道还有这一步。
    //
    // **跟着 `--quiet` 一起闭嘴**：脚本里跑 `--quiet` 的人要的是干净的输出，
    // 而这一句是提示不是结论。
    if !args.quiet {
        eprintln!(
            "标题是**集合**：`romcat titles` 把这一趟采到的标题折成标题集合，挑出显示标题与排序标题。"
        );
    }
    if !write_json(args.json.as_deref(), &outcome.report) {
        return ExitCode::FAILURE;
    }
    // **停下来不等于白跑。** 已经采完的那批在上面就写进中立库了，报告也照出——
    // 退出码只是让脚本知道「这一趟没跑完」，以及**还能不能马上再来一趟**。
    if let Some(halt) = &outcome.halted {
        eprintln!("\n{}", halt.describe());
        return ExitCode::from(match halt {
            // **4：我们自己收的手。** 立刻重跑就接着采——脚本可以照着循环。
            scrape::online::Halt::Budget { .. } => 4,
            // **3：对面不让了**（配额超限、凭据不对、被拉黑），或者网断了。
            // 立刻重跑没有意义，配额那一条更是**今天都别再来**。
            _ => 3,
        });
    }
    if outcome.interrupted {
        eprintln!("这一趟被中断了，已经采完的那部分留在中立库里，重跑会接着采。");
        return ExitCode::from(130);
    }
    ExitCode::SUCCESS
}

/// 折出**标题集合**，挑出**显示标题**与**排序标题**。
///
/// 它是刮削与导出之间的那一折：值来自刮削（DAT 条目名、文件名），语言、地区与类型来自
/// 识别（发行版的地区与语言、候选上的中文记号）。**一个字节都不读主库，一个请求都不发。**
fn run_titles(args: &TitlesArgs) -> ExitCode {
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
    let priorities = match load_priorities(args.priorities.as_deref(), &workspace) {
        Ok(priorities) => priorities,
        Err(message) => return fail(message),
    };
    // 主库只读（ADR-0004）：报告不许落进主库。
    if let Some(root) = args.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let mut catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };

    let started = Instant::now();
    let report = match title::run(&mut catalog, &priorities) {
        Ok(report) => report,
        Err(error) => return fail(format!("标题折不出来：{error}")),
    };
    if !args.quiet {
        let text = report.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "折标题用了 {:.1} 秒，一个字节都没读主库。{} 个作品拿到了中文显示标题。",
        started.elapsed().as_secs_f64(),
        thousands(report.chinese_works),
    );
    if report.works == 0 {
        eprintln!(
            "库里一个作品都没有——标题挂在作品上，先跑一次 `romcat identify`，再跑 `romcat scrape`。"
        );
    }
    if !write_json(args.json.as_deref(), &report) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 列出眼下带的适配器与它们的能力档位。
///
/// **能力档位对用户可见**是 ADR-0003 点名的要求：导出前就该说得出「这个格式会丢掉
/// 你的什么」，而不是让用户事后发现。这里列的是每个适配器**声称的上限**——
/// 实测档位由 `romcat import` 在维护者手上那份真文件上跑一趟往返断言出来。
fn run_adapters() -> ExitCode {
    println!("适配器与能力档位");
    println!("{}", "═".repeat(24));
    println!("{}{}元数据文件", pad("格式", 12), pad("上限", 12));
    for adapter in adapter::all() {
        println!(
            "{}{}{}",
            pad(adapter.name(), 12),
            pad(adapter.ceiling().label(), 12),
            adapter.file_name()
        );
    }
    println!(
        "\n上限是**声称**的。实测档位在 `romcat import` 时断言出来：\n\
         每份文件读进来又写回去，逐字节比过——过了才算无损往返，没过自动降一档。"
    );
    ExitCode::SUCCESS
}

/// 把维护者手工维护的前端元数据导进中立库。
fn run_import(args: &ImportArgs) -> ExitCode {
    // **先认格式。** 打错一个格式名是最先该被告知的事，不该等到中立库都找过一遍。
    let adapter = match args.format.load() {
        Ok(adapter) => adapter,
        Err(message) => return fail(message),
    };
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
    if let Some(root) = args.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let mut catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    // `--dry-run` 走同一条路，只是落在一份临时的内存库上：往返实测与「会留下什么」
    // 照报，中立库一行不动。
    let report = if args.dry_run {
        let mut scratch = match Catalog::open_in_memory() {
            Ok(scratch) => scratch,
            Err(error) => return fail(format!("临时中立库开不出来：{error}")),
        };
        transfer::import(
            &mut scratch,
            adapter.as_ref(),
            &args.files,
            args.root.as_deref(),
        )
    } else {
        transfer::import(
            &mut catalog,
            adapter.as_ref(),
            &args.files,
            args.root.as_deref(),
        )
    };
    let mut report = match report {
        Ok(report) => report,
        Err(error) => return fail(format!("导入没跑完：{error}")),
    };
    report.catalog = catalog.location().to_string();
    if !args.quiet {
        let text = report.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    let 逐字节 = report.files.iter().filter(|file| file.roundtrip).count();
    eprintln!(
        "{} 份文件，其中 {} 份写回去与原文逐字节相同；实测档位是**{}**。{}",
        thousands(report.files.len() as u64),
        thousands(逐字节 as u64),
        report.tier,
        if args.dry_run {
            "（`--dry-run`：中立库一行没动。）"
        } else {
            "原文已逐字节存进中立库。"
        }
    );
    if args.root.is_none() && matches!(catalog.library_root(), Ok(None)) {
        eprintln!(
            "中立库里没记主库根、命令行也没给 `--root`：`file:` 里的路径折不成变体的键，\n\
             这一趟只存了快照。加上 `--root <主库根>` 再跑一次，值才落得进库。"
        );
    }
    if !write_json(args.json.as_deref(), &report) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 把中立库导出成前端元数据。
fn run_export(args: &ExportArgs) -> ExitCode {
    let adapter = match args.format.load() {
        Ok(adapter) => adapter,
        Err(message) => return fail(message),
    };
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
    let priorities = match load_priorities(args.priorities.as_deref(), &workspace) {
        Ok(priorities) => priorities,
        Err(message) => return fail(message),
    };
    if let Some(root) = args.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let mut catalog = match open_catalog(&workspace, slug, None) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };

    // **裁决先落库**：它是沉淀，不随这一趟导出消失。
    for key in &args.prefer {
        let variant = match catalog.variant(key) {
            Ok(Some(variant)) => variant,
            Ok(None) => return fail(format!("库里没有叫「{key}」的变体。")),
            Err(error) => return fail(format!("中立库读不动：{error}")),
        };
        let work = match variant.work_id {
            Some(id) => match catalog.work_names() {
                Ok(names) => names.get(&id).cloned().unwrap_or_else(|| key.clone()),
                Err(error) => return fail(format!("中立库读不动：{error}")),
            },
            None => key.clone(),
        };
        let platform = variant
            .platform
            .clone()
            .unwrap_or_else(|| romcat_core::report::UNKNOWN_PLATFORM_LABEL.to_string());
        if let Err(error) = catalog.set_preferred_variant(&work, &platform, key) {
            return fail(format!("裁决写不进中立库：{error}"));
        }
        eprintln!("裁决已记下：作品「{work}」在 {platform} 上默认启动 {key}。");
    }

    let started = Instant::now();
    let options = transfer::ExportOptions {
        out: args.out.clone(),
        dry_run: args.dry_run,
        force: args.force,
    };
    let report = match transfer::export(&mut catalog, adapter.as_ref(), &priorities, &options) {
        Ok(report) => report,
        Err(error) => return fail(format!("导出没跑完：{error}")),
    };
    if !args.quiet {
        let text = report.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "导出用了 {:.1} 秒，一个字节都没读主库。{} 个条目写进 {} 份文件，实测档位**{}**。",
        started.elapsed().as_secs_f64(),
        thousands(report.entries),
        thousands(report.files.len() as u64),
        report.tier,
    );
    if !report.conflicts.is_empty() {
        eprintln!(
            "有 {} 份没写——外面有人动过。**没有静默覆盖**，详见报告。",
            thousands(report.conflicts.len() as u64)
        );
        if !write_json(args.json.as_deref(), &report) {
            return ExitCode::FAILURE;
        }
        return ExitCode::FAILURE;
    }
    if report.entries == 0 {
        eprintln!("库里一个变体都没有——先跑一次 `romcat scan`，再跑 `romcat shape`。");
    }
    if !write_json(args.json.as_deref(), &report) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// **待确认队列**的几件事。
///
/// 队列是这个工具的主界面（ADR-0002）。命令行这一版把逻辑跑通并测试，界面是它的一层壳。
#[derive(Debug, Subcommand)]
enum TriageCommand {
    /// 列出待裁决的变体、它们的全部候选与依据，并报出按各个轴一次能覆盖多少
    List(TriageListArgs),
    /// 看一条：它的候选、每条候选的依据、以及裁决会钉在什么上
    Show(TriageShowArgs),
    /// **批量裁决**：按目录、按候选作品、按命名规律一次套用几百条
    Decide(TriageDecideArgs),
    /// 忘掉裁决。批量下错了得走得回来
    Undo(TriageUndoArgs),
    /// 把沉淀库导出成可分享的一份 JSON——这份数据补的正是 TOSEC 缺的中文汉化部分
    Export(TriageExportArgs),
    /// 收下别人分享的一份裁决
    Import(TriageImportArgs),
}

/// 队列这几条命令共用的：开哪一份中立库、哪一份沉淀库。
#[derive(Debug, Args, Clone)]
struct TriageCommonArgs {
    /// 主库根目录。只用来找到对应的中立库，不会去读它；给了 `--library` 就不必再给
    #[arg(value_name = "主库根")]
    root: Option<PathBuf>,
    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,
    /// 工作目录：中立库与沉淀库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,
}

/// 只开**沉淀库**：导出、导入与「忘掉裁决」不必连中立库一起开。
fn open_store(workspace: &Path) -> Result<Store, String> {
    Store::open(&workspace::verdict_store_path(workspace)).map_err(|error| format!("{error}"))
}

impl TriageCommonArgs {
    /// 开一份**现场**：中立库、沉淀库，连这份主库在**路径锚**里叫什么名字。
    ///
    /// 三样一起开在 `core::site` 里——界面走的是同一条（ADR-0005：核心是独立的库）。
    /// 命令行这一层只负责把三种给法折成一个 [`Slug`]。
    fn open(&self) -> Result<Site, String> {
        let (slug, located_by) = locate(self.library.as_deref(), self.root.as_deref())?;
        let workspace = workspace_dir(self.workspace.as_deref());
        Site::open(&workspace, slug, self.root.as_deref(), &located_by)
            .map_err(|error| format!("{error}"))
    }
}

/// 挑队列里的哪些。**几个条件之间是交集**，同一个条件给几次是并集。
#[derive(Debug, Args, Clone, Default)]
struct TriageFilterArgs {
    /// **按目录**：只要这个目录下的，如 `--under FC`。可重复给
    #[arg(long, value_name = "目录前缀")]
    under: Vec<String>,
    /// 按平台，如 `--platform GBA`。可重复给
    ///
    /// **这是选择器，不是裁决内容**——裁决记的平台是 `--set-platform`
    #[arg(long, value_name = "平台")]
    platform: Vec<String>,
    /// 按识别结论：`未命中` / `无判据` / `命中` / `跳过`。可重复给
    ///
    /// 不给就是默认那三档（命中但没自动通过 / 未命中 / 无判据）。
    /// **跳过默认不在队列里**——那不是「拿不定主意」，是「本来就不该撞 DAT」
    #[arg(long, value_name = "结论")]
    state: Vec<String>,
    /// **按命名规律**：文件名里含有这段文字（不分大小写）。可重复给
    ///
    /// 汉化组的记号常常就写在名字里，这一条一次能圈住一整批
    #[arg(long, value_name = "文字")]
    name: Vec<String>,
    /// **按候选作品**：候选里有一条指着这部作品。可重复给
    #[arg(long, value_name = "作品")]
    candidate_work: Vec<String>,
    /// 点名这个变体（用它的键）。可重复给
    #[arg(long, value_name = "变体键")]
    key: Vec<String>,
}

impl TriageFilterArgs {
    fn build(&self) -> Result<Filter, String> {
        let mut states = Vec::new();
        for label in &self.state {
            let state = romcat_core::catalog::State::from_label(label).ok_or_else(|| {
                format!("认不出结论「{label}」。写 `未命中` / `无判据` / `命中` / `跳过` 之一。")
            })?;
            states.push(state);
        }
        Ok(Filter {
            under: self.under.clone(),
            platform: self.platform.clone(),
            states,
            name_contains: self.name.clone(),
            candidate_work: self.candidate_work.clone(),
            keys: self.key.clone(),
        })
    }
}

#[derive(Debug, Args)]
struct TriageListArgs {
    #[command(flatten)]
    common: TriageCommonArgs,
    #[command(flatten)]
    filter: TriageFilterArgs,
    /// 印几条明细（分组那几张表照印不误）
    #[arg(long, value_name = "条数", default_value_t = 10)]
    limit: usize,
    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,
    /// 不打印文本报告
    #[arg(long)]
    quiet: bool,
}

#[derive(Debug, Args)]
struct TriageShowArgs {
    /// 变体的键
    #[arg(value_name = "变体键")]
    key: String,
    #[command(flatten)]
    common: TriageCommonArgs,
}

#[derive(Debug, Args)]
struct TriageDecideArgs {
    #[command(flatten)]
    common: TriageCommonArgs,
    #[command(flatten)]
    filter: TriageFilterArgs,
    /// 采用第几条**候选**（从 1 数起，序号见 `romcat triage list`）
    #[arg(long, value_name = "序号")]
    pick: Option<usize>,
    /// **手工指定**这部作品。候选集不构成天花板——队列里大多数条目一条候选都没有
    #[arg(long, value_name = "作品")]
    work: Option<String>,
    /// 裁决记的平台；不给就用变体自己的平台
    #[arg(long, value_name = "平台")]
    set_platform: Option<String>,
    /// 裁决记的地区
    #[arg(long, value_name = "地区")]
    region: Option<String>,
    /// 裁决记的序列号
    #[arg(long, value_name = "序列号")]
    serial: Option<String>,
    /// 裁决记的语言标记组，如 `Ja,Zh-Hans`
    #[arg(long, value_name = "语言")]
    languages: Option<String>,
    /// 中文身份：`汉化` 或 `官中`（ADR-0012 分的正是这两件事）
    #[arg(long, value_name = "记号")]
    chinese: Option<String>,
    /// **汉化组**：谁做的这个中文版本。自动识别只做到发行版级，这一样只能由人说
    #[arg(long, value_name = "汉化组")]
    team: Option<String>,
    /// 版本，如 `v1.2`
    #[arg(long, value_name = "版本")]
    version: Option<String>,
    /// 判定「都不对」：它**没有发行版**（同人移植、homebrew）。配 `--work` 还能挂个作品
    #[arg(long)]
    no_release: bool,
    /// 判定「都不对，而且我也认不出」。记下来别再问第二遍
    #[arg(long)]
    unknown: bool,
    /// 记一句为什么。半年后你会想知道当初凭什么这么定
    #[arg(long, value_name = "一句话")]
    note: Option<String>,
    /// 只排计划、报出这一趟会裁多少条，**一个字都不写**
    #[arg(long)]
    dry_run: bool,
    /// 看过计划之后拿它点头。**一次改不止一条时必须给**
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct TriageUndoArgs {
    #[command(flatten)]
    common: TriageCommonArgs,
    #[command(flatten)]
    filter: TriageFilterArgs,
    /// 只排计划，不忘
    #[arg(long)]
    dry_run: bool,
    /// 看过计划之后拿它点头。**一次忘不止一条时必须给**
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, Args)]
struct TriageExportArgs {
    /// 工作目录：沉淀库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,
    /// 写到哪个文件
    #[arg(long, value_name = "文件")]
    out: PathBuf,
    /// 连**只在本机成立**的路径锚一起导出
    ///
    /// 默认不带：它们对别人毫无用处，而且顺带把自己的目录结构也交出去了
    #[arg(long)]
    include_path: bool,
}

#[derive(Debug, Args)]
struct TriageImportArgs {
    /// 要收下的裁决文件，可以给多个
    #[arg(value_name = "文件", required = true)]
    files: Vec<PathBuf>,
    /// 工作目录：沉淀库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,
}

/// 子库的几件事。
///
/// **这一组只定义、查看与排计划，不搬文件**：搬文件是票 20、转格式是票 21。
/// `plan` 排出的那份计划**就是差量预览**——同步前必须先看它（ADR-0016）。
#[derive(Debug, Subcommand)]
enum SublibraryCommand {
    /// 新建或改一个子库：目标路径、前端格式、容量上限
    Set(SubSetArgs),
    /// 列出这个主库上的全部子库——**互不干扰**，规则与例外各记各的
    List(SubCommonArgs),
    /// 删掉一个子库，连它的规则与例外一起
    Remove(SubNameArgs),
    /// 规则：可重放的那一半。主库新增的、规则说得中的内容，下次自动进入
    Rule(SubRuleArgs),
    /// 例外：手动增删的那一半。**优先于规则、永久记住**
    Except(SubExceptArgs),
    /// 看选择集：选中多少条、共多少容量、装不装得下
    Show(SubShowArgs),
    /// **差量预览**：同步前先看清楚会新增什么、删除什么、净变化多少。只读，不搬任何文件
    Plan(SubPlanArgs),
    /// **同步**：把差量真正落到目标设备上。先印一遍预览，删除还要你点头
    Sync(SubSyncArgs),
}

/// 七个子命令共用的那几个参数。
#[derive(Debug, Args, Clone)]
struct SubCommonArgs {
    /// 主库根目录。只用来找到对应的中立库，不会去读它；给了 `--library` 就不必再给
    #[arg(long, value_name = "目录")]
    root: Option<PathBuf>,

    /// 按名字找中立库（扫描时用 `--library` 起的那个名字）
    #[arg(long, value_name = "名字")]
    library: Option<String>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,
}

#[derive(Debug, Args)]
struct SubNameArgs {
    /// 子库叫什么
    #[arg(value_name = "子库")]
    name: String,

    #[command(flatten)]
    common: SubCommonArgs,
}

#[derive(Debug, Args)]
struct SubSetArgs {
    /// 子库叫什么。一台目标设备一个
    #[arg(value_name = "子库")]
    name: String,

    /// 目标设备上的子库根：读卡器挂上来的那个盘上的目录。**新建时必须给**
    #[arg(long, value_name = "路径")]
    target: Option<PathBuf>,

    /// 前端格式（`romcat adapters` 列得出有哪些）。新建时默认 Pegasus
    #[arg(long, value_name = "格式")]
    format: Option<String>,

    /// 容量上限，如 `512GB`、`476GiB`。超限**不自动截断**，只报出超出量与裁剪建议
    #[arg(long, value_name = "容量")]
    capacity: Option<String>,

    /// 去掉容量上限
    #[arg(long, conflicts_with = "capacity")]
    no_capacity: bool,

    /// **能力档案**：这台设备吃得下什么、这张卡放得下什么（`romcat capability` 列得出）
    ///
    /// 不给就是「不作声称」——不转换、不检查
    #[arg(long, value_name = "档案")]
    capability: Option<String>,

    #[command(flatten)]
    common: SubCommonArgs,
}

#[derive(Debug, Args)]
struct SubRuleArgs {
    /// 子库叫什么
    #[arg(value_name = "子库")]
    name: String,

    /// 加一条规则，如 `平台=GB,GBA 且 中文=汉化`
    #[arg(long, value_name = "规则")]
    add: Option<String>,

    /// 按序号删掉一条规则（序号见 `romcat sublibrary rule <子库>`）
    #[arg(long, value_name = "序号")]
    remove: Option<i64>,

    #[command(flatten)]
    common: SubCommonArgs,
}

#[derive(Debug, Args)]
struct SubExceptArgs {
    /// 子库叫什么
    #[arg(value_name = "子库")]
    name: String,

    /// 强行收入这个变体：规则没选中也带上
    #[arg(long, value_name = "变体的键")]
    include: Option<String>,

    /// 强行排除这个变体：规则选中了也不带
    #[arg(long, value_name = "变体的键")]
    exclude: Option<String>,

    /// 忘掉这个变体上的例外，从此听规则的
    #[arg(long, value_name = "变体的键")]
    forget: Option<String>,

    /// 记一句为什么。半年后你会想知道当初为什么排除它
    #[arg(long, value_name = "一句话")]
    note: Option<String>,

    #[command(flatten)]
    common: SubCommonArgs,
}

#[derive(Debug, Args)]
struct SubShowArgs {
    /// 子库叫什么
    #[arg(value_name = "子库")]
    name: String,

    /// 把报告另存为 JSON
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不往标准输出打报告
    #[arg(long)]
    quiet: bool,

    #[command(flatten)]
    common: SubCommonArgs,
}

#[derive(Debug, Args)]
struct SubPlanArgs {
    /// 子库叫什么
    #[arg(value_name = "子库")]
    name: String,

    /// 把计划另存为 JSON。**它就是预览本身**，不是另算的一份
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不看子库定义里的目标路径，改看这个目录（拿本地目录当目标设备演练）
    #[arg(long, value_name = "目录")]
    target: Option<PathBuf>,

    /// 把「清单说有、实际没了」的那些补回去
    ///
    /// 默认**不补**：那可能是你在掌机上有意删的（ADR-0015）。它们照样会被报出来
    #[arg(long)]
    restore: bool,

    /// 字段级优先级表。**与 `sync` 必须给同一份**，不然预览与同步算出的差量会漂开
    #[arg(long, value_name = "文件")]
    priorities: Option<PathBuf>,

    /// 不往标准输出打预览
    #[arg(long)]
    quiet: bool,

    #[command(flatten)]
    common: SubCommonArgs,
}

#[derive(Debug, Args)]
struct SubSyncArgs {
    /// 子库叫什么
    #[arg(value_name = "子库")]
    name: String,

    /// 把计划另存为 JSON。**它就是预览本身**，不是另算的一份
    #[arg(long, value_name = "文件")]
    json: Option<PathBuf>,

    /// 不看子库定义里的目标路径，改看这个目录（拿本地目录当目标设备演练）
    #[arg(long, value_name = "目录")]
    target: Option<PathBuf>,

    /// 只印预览，**一个字节都不写**
    #[arg(long)]
    dry_run: bool,

    /// 计划里有删除时，看过预览之后拿它点头
    #[arg(long)]
    yes: bool,

    /// 把「清单说有、实际没了」的那些补回去
    ///
    /// 默认**不补**：那可能是你在掌机上有意删的（ADR-0015）。它们照样会被报出来
    #[arg(long)]
    restore: bool,

    /// 主库根目录：搬 ROM 要读它。不给就用中立库里记着的那个
    #[arg(long, value_name = "目录")]
    library_root: Option<PathBuf>,

    /// 字段级优先级表。不给就先看工作目录里有没有 `priorities.toml`，都没有才用内置的
    #[arg(long, value_name = "文件")]
    priorities: Option<PathBuf>,

    /// **转换缓存目录**。不给就边转边流式写进目标，中间不落第三份
    ///
    /// 转换很贵，而缓存要再占一份等同空间，因此**默认不缓存**（ADR-0017）。
    /// 几台设备要的是同一种格式时给一个目录，转一次用多次
    #[arg(long, value_name = "目录")]
    convert_cache: Option<PathBuf>,

    #[command(flatten)]
    common: SubCommonArgs,
}

impl SubCommonArgs {
    /// 找到并打开这个主库的中立库。
    ///
    /// 子库的每件事都只读中立库——**目标设备不在位、外置盘不在位都照样干得了**
    /// （ADR-0009）。子库是持久实体，不是「插上卡才存在的东西」。
    fn open(&self) -> Result<Catalog, String> {
        let (slug, located_by) = locate(self.library.as_deref(), self.root.as_deref())?;
        let workspace = workspace_dir(self.workspace.as_deref());
        if !workspace::catalog_path(&workspace, slug).exists() {
            return Err(format!(
                "还没有 {located_by} 这份中立库。先跑一次 `romcat scan`。"
            ));
        }
        open_catalog(&workspace, slug, self.root.as_deref())
    }
}

/// 取出这个子库，取不到就说清怎么建。
fn load_sublibrary(catalog: &Catalog, name: &str) -> Result<Sublibrary, String> {
    match catalog.sublibrary(name) {
        Ok(Some(sublibrary)) => Ok(sublibrary),
        Ok(None) => Err(format!(
            "没有叫「{name}」的子库。\n\
             建一个：`romcat sublibrary set {name} --target <目标设备上的目录>`"
        )),
        Err(error) => Err(format!("中立库读不动：{error}")),
    }
}

/// 列出**待确认队列**。
///
/// **一个字节都不读主库**：队列由中立库与沉淀库折出来（ADR-0001），盘不在位时照样看得见。
fn run_triage_list(args: &TriageListArgs) -> ExitCode {
    let site = match args.common.open() {
        Ok(site) => site,
        Err(message) => return fail(message),
    };
    // 主库只读（ADR-0004）：报告不许落进主库。
    if let Some(root) = args.common.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let filter = match args.filter.build() {
        Ok(filter) => filter,
        Err(message) => return fail(message),
    };
    let (mut survey, counts) = match collect_queue(&site, &filter) {
        Ok(loaded) => loaded,
        Err(message) => return fail(message),
    };
    if !survey.identified {
        let report = triage::report::QueueReport::not_identified(
            site.catalog.location(),
            site.store.location(),
            counts,
        );
        if !args.quiet {
            print!("{}", report.render_text());
        }
        if !write_json(args.json.as_deref(), &report) {
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }
    // 只为要印出来的那几条算判据：那一步要为每个变体查两次中立库，
    // 一万多条的队列不该在只是列一眼的时候整份算出来。
    let shown = args.limit.min(survey.items.len());
    if let Err(error) = triage::fill_prints(&site.catalog, &mut survey.items[..shown]) {
        return fail(format!("中立库读不动：{error}"));
    }
    let report = triage::report::QueueReport::build(
        site.catalog.location(),
        site.store.location(),
        survey.queue,
        &survey.items,
        counts,
        args.limit,
    );
    if !args.quiet {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(report.render_text().as_bytes());
        let _ = stdout.flush();
    }
    if !write_json(args.json.as_deref(), &report) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 折一次队列，连沉淀库眼下的账。
fn collect_queue(
    site: &Site,
    filter: &Filter,
) -> Result<(triage::Survey, verdict::Counts), String> {
    let counts = site
        .store
        .counts()
        .map_err(|error| format!("沉淀库读不动：{error}"))?;
    let index = verdict::Index::load(&site.store, &site.library)
        .map_err(|error| format!("沉淀库读不动：{error}"))?;
    let survey = triage::survey(&site.catalog, &index, filter)
        .map_err(|error| format!("中立库读不动：{error}"))?;
    Ok((survey, counts))
}

/// 看一条队列条目的全部候选与依据。
fn run_triage_show(args: &TriageShowArgs) -> ExitCode {
    let site = match args.common.open() {
        Ok(site) => site,
        Err(message) => return fail(message),
    };
    let filter = Filter {
        keys: vec![args.key.clone()],
        // 点名一条时**四档全看**：用户已经说出它的键了，再拿默认那三档把它筛掉
        // 只会让人以为库里没有这个变体。
        states: romcat_core::catalog::State::ALL.to_vec(),
        ..Filter::default()
    };
    let (mut survey, counts) = match collect_queue(&site, &filter) {
        Ok(loaded) => loaded,
        Err(message) => return fail(message),
    };
    if survey.items.is_empty() {
        return fail(format!(
            "队列里没有「{}」。它要么已经自动通过、要么已经裁决过、要么根本不在库里\n\
             （`romcat triage list --under <目录>` 看得到有哪些）。",
            args.key
        ));
    }
    if let Err(error) = triage::fill_prints(&site.catalog, &mut survey.items) {
        return fail(format!("中立库读不动：{error}"));
    }
    let shown = survey.items.len();
    let report = triage::report::QueueReport::build(
        site.catalog.location(),
        site.store.location(),
        survey.queue,
        &survey.items,
        counts,
        shown,
    );
    print!("{}", report.render_text());
    ExitCode::SUCCESS
}

/// **批量裁决**。
///
/// 先印计划再动手，与同步那一侧的差量预览同源（ADR-0016）：一条命令改几百条记录，
/// 看不见它要改什么就按下去，错了没处找。
fn run_triage_decide(args: &TriageDecideArgs) -> ExitCode {
    // 先把话说清再开库：不成立的裁决不该等到读完一遍队列才被挡下。
    let draft = match args.draft().and_then(|draft| draft.check().map(|()| draft)) {
        Ok(draft) => draft,
        Err(message) => return fail(message),
    };
    let mut site = match args.common.open() {
        Ok(site) => site,
        Err(message) => return fail(message),
    };
    let filter = match args.filter.build() {
        Ok(filter) => filter,
        Err(message) => return fail(message),
    };
    let (mut survey, _) = match collect_queue(&site, &filter) {
        Ok(loaded) => loaded,
        Err(message) => return fail(message),
    };
    if !survey.identified {
        return fail("还没跑过识别，队列无从谈起。先跑一次 `romcat identify`。");
    }
    if survey.items.is_empty() {
        return fail(
            "一条都没选中。选择器写宽一点，或者先 `romcat triage list` 看看队列里有什么。",
        );
    }
    if let Err(error) = triage::fill_prints(&site.catalog, &mut survey.items) {
        return fail(format!("中立库读不动：{error}"));
    }
    let decide = match draft.build(&site.library) {
        Ok(decide) => decide,
        Err(message) => return fail(message),
    };
    let plan = match triage::plan(&site.store, &survey.items, &decide) {
        Ok(plan) => plan,
        Err(error) => return fail(format!("排不出计划：{error}")),
    };
    print_verdict_plan(&plan, &decide);
    if plan.decided.is_empty() {
        return fail("一条都落不下去（上面说了各自的原因）。");
    }
    if args.dry_run {
        eprintln!("这是 --dry-run，一个字都没写。");
        return ExitCode::SUCCESS;
    }
    if plan.decided.len() > 1 && !args.yes {
        return fail(format!(
            "这一趟要裁 {} 条。看过上面的计划之后加 --yes 点头，或者用 --dry-run 只看不做。",
            thousands(plan.decided.len() as u64)
        ));
    }
    let applied = match triage::apply(&mut site.catalog, &mut site.store, &survey.items, &plan) {
        Ok(applied) => applied,
        Err(error) => return fail(format!("裁决写不进去：{error}")),
    };
    println!(
        "\n裁决已沉淀 {} 条（新增 {}、盖掉 {}）：钉在内容上的 {} 条（**可导出分享**），\n\
         只钉得住本机路径的 {} 条。中立库当场兑现：{} 条转成命中、{} 条转成跳过。",
        thousands(applied.verdicts),
        thousands(applied.added),
        thousands(applied.replaced),
        thousands(applied.content_anchored),
        thousands(applied.path_anchored),
        thousands(applied.matched),
        thousands(applied.skipped),
    );
    println!(
        "沉淀库在 {}——它**不跟中立库走**，删掉中立库重扫也不会丢这些裁决。",
        site.store.location()
    );
    ExitCode::SUCCESS
}

impl TriageDecideArgs {
    /// 这条命令起草的那次裁决。
    ///
    /// **判据在核心库里**（`triage::Draft`）：命令行的开关与界面上的单选钮说的是同一件
    /// 事，「一次只说一种裁决」这类话写两遍迟早会漂开。这里只负责把开关搬过去，
    /// 外加一样命令行独有的活——把 `--chinese` 那串文字认回一个记号。
    fn draft(&self) -> Result<triage::Draft, String> {
        let chinese = match self.chinese.as_deref() {
            None => None,
            Some(label) => Some(
                romcat_core::dat::chinese::ChineseMark::from_label(label).ok_or_else(|| {
                    format!("认不出中文记号「{label}」。只有 `汉化` 与 `官中` 两种（ADR-0012）。")
                })?,
            ),
        };
        Ok(triage::Draft {
            pick: self.pick,
            work: self.work.clone(),
            no_release: self.no_release,
            unknown: self.unknown,
            // **`--pick` 也吃这几样**——「就是这条候选，另外汉化组是某某」是最常见的
            // 一句话，收下再忽略是最坏的一种「实现了」。
            overrides: triage::Overrides {
                platform: self.set_platform.clone(),
                region: self.region.clone(),
                serial: self.serial.clone(),
                languages: self.languages.clone(),
                chinese,
                team: self.team.clone(),
                version: self.version.clone(),
            },
            note: self.note.clone(),
        })
    }
}

/// 把一次批量裁决的计划印出来。
fn print_verdict_plan(plan: &triage::Plan, decide: &triage::Decide) {
    println!("批量裁决计划");
    println!("{}", "═".repeat(24));
    println!("裁成            {}", describe_spec(&decide.spec));
    for (label, value) in [
        ("平台", &decide.overrides.platform),
        ("地区", &decide.overrides.region),
        ("序列号", &decide.overrides.serial),
        ("语言", &decide.overrides.languages),
        ("汉化组", &decide.overrides.team),
        ("版本", &decide.overrides.version),
    ] {
        if let Some(value) = value {
            println!("{}{value}", pad(label, 16));
        }
    }
    if let Some(mark) = decide.overrides.chinese {
        println!("{}{}", pad("中文", 16), mark.label());
    }
    println!(
        "要裁            {} 条（其中 {} 条会盖掉已有的裁决）",
        thousands(plan.decided.len() as u64),
        thousands(plan.replacing() as u64),
    );
    println!(
        "锚              内容 {} 条（可分享）／路径 {} 条（只在本机成立）",
        thousands(plan.content_anchored() as u64),
        thousands(plan.path_anchored() as u64),
    );
    let show = 10;
    for row in plan.decided.iter().take(show) {
        println!("  + {}", row.key);
    }
    if plan.decided.len() > show {
        println!(
            "  …… 还有 {} 条",
            thousands((plan.decided.len() - show) as u64)
        );
    }
    if !plan.blocked.is_empty() {
        println!("\n落不下去的 {} 条：", thousands(plan.blocked.len() as u64));
        for row in plan.blocked.iter().take(show) {
            println!("  ! {}：{}", row.key, row.why);
        }
        if plan.blocked.len() > show {
            println!(
                "  …… 还有 {} 条",
                thousands((plan.blocked.len() - show) as u64)
            );
        }
    }
}

fn describe_spec(spec: &DecisionSpec) -> String {
    match spec {
        DecisionSpec::Pick(nth) => format!("采用各自的第 {nth} 条候选"),
        DecisionSpec::Manual(work) => format!("作品《{work}》"),
        DecisionSpec::NoRelease { work } => format!(
            "确认没有发行版{}",
            work.as_deref()
                .map(|w| format!("，挂在作品《{w}》下"))
                .unwrap_or_default()
        ),
        DecisionSpec::Unknown => "都不对，而且认不出是什么".to_string(),
    }
}

/// 忘掉裁决。
fn run_triage_undo(args: &TriageUndoArgs) -> ExitCode {
    let mut site = match args.common.open() {
        Ok(site) => site,
        Err(message) => return fail(message),
    };
    let filter = match args.filter.build() {
        Ok(filter) => filter,
        Err(message) => return fail(message),
    };
    if filter.is_empty() {
        return fail("`undo` 不接受空的选择器——那是把整份沉淀库忘掉。至少给一个条件。");
    }
    let plan = match triage::plan_forget(&site.catalog, &site.store, &filter, &site.library) {
        Ok(plan) => plan,
        Err(error) => return fail(format!("中立库或沉淀库读不动：{error}")),
    };
    if plan.rows.is_empty() {
        return fail("这些变体上一条裁决都没有。");
    }
    println!("要忘掉 {} 条裁决：", thousands(plan.rows.len() as u64));
    for (key, anchor) in plan.rows.iter().take(10) {
        println!("  - {key}（{}）", anchor.describe());
    }
    if plan.rows.len() > 10 {
        println!("  …… 还有 {} 条", thousands((plan.rows.len() - 10) as u64));
    }
    if args.dry_run {
        eprintln!("这是 --dry-run，一个字都没写。");
        return ExitCode::SUCCESS;
    }
    if plan.rows.len() > 1 && !args.yes {
        return fail("一次忘不止一条时要加 --yes 点头。");
    }
    match triage::forget(&mut site.store, &plan) {
        Ok(gone) => {
            println!("已忘掉 {} 条。", thousands(gone));
            println!("中立库那一半下一趟 `romcat identify` 会重算——那时它们回到队列里。");
            ExitCode::SUCCESS
        }
        Err(error) => fail(format!("沉淀库写不动：{error}")),
    }
}

/// 把沉淀库导出成可分享的一份 JSON。
fn run_triage_export(args: &TriageExportArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let store = match open_store(&workspace) {
        Ok(store) => store,
        Err(message) => return fail(message),
    };
    let export = match store.export(args.include_path) {
        Ok(export) => export,
        Err(error) => return fail(format!("沉淀库读不动：{error}")),
    };
    let text = match serde_json::to_string_pretty(&export) {
        Ok(text) => text,
        Err(error) => return fail(format!("序列化失败：{error}")),
    };
    if let Err(error) = write_file(&args.out, format!("{text}\n").as_bytes()) {
        return fail(format!("写不出 {}：{error}", path::display(&args.out)));
    }
    println!(
        "已导出 {} 条裁决到 {}。",
        thousands(export.verdicts.len() as u64),
        path::display(&args.out)
    );
    if !args.include_path {
        let counts = store.counts().unwrap_or_default();
        if counts.path > 0 {
            println!(
                "另有 {} 条只钉得住本机路径，没有导出（`--include-path` 带上它们，但它们对别人没用）。",
                thousands(counts.path)
            );
        }
    }
    ExitCode::SUCCESS
}

/// 收下别人分享的裁决。
fn run_triage_import(args: &TriageImportArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let mut store = match open_store(&workspace) {
        Ok(store) => store,
        Err(message) => return fail(message),
    };
    for file in &args.files {
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => return fail(format!("读不动 {}：{error}", path::display(file))),
        };
        match store.import(&text) {
            Ok(account) => println!(
                "{}：读到 {} 条，新收 {}、盖掉 {}、认不出锚丢掉 {}。",
                path::display(file),
                thousands(account.read),
                thousands(account.added),
                thousands(account.replaced),
                thousands(account.unreadable),
            ),
            Err(error) => return fail(format!("{}：{error}", path::display(file))),
        }
    }
    println!("收下的裁决下一趟 `romcat identify` 就会直接命中，那些变体不再进队列。");
    ExitCode::SUCCESS
}

fn run_sublibrary_set(args: &SubSetArgs) -> ExitCode {
    let mut catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    let existing = match catalog.sublibrary(&args.name) {
        Ok(existing) => existing,
        Err(error) => return fail(format!("中立库读不动：{error}")),
    };
    // **两种形式一次填齐**（ADR-0020、挂账 D82）：NFC 那一份当键、进报告，
    // 系统给的原始那一份留着读盘。只存 NFC 的话，目标目录名是分解形式、又挂在
    // 分解敏感的文件系统上时，同步会打不开它然后报「目标不在位」——最不该说的谎。
    let (target, target_raw) = match (&args.target, &existing) {
        (Some(target), _) => {
            // **两种形式怎么折，由 `Sublibrary::at` 一处说了算。** 在这儿再抄一遍
            // `nfc` 与 `to_str`，两处迟早漂开——而漂开的后果正是 D82 那个 bug。
            // 格式与容量下面单算，这里只借它折路径。
            let folded = Sublibrary::at(
                &args.name,
                &path::normalize_existing(target),
                DEFAULT_FORMAT,
                None,
            );
            (folded.target, folded.target_raw)
        }
        (None, Some(existing)) => (existing.target.clone(), existing.target_raw.clone()),
        (None, None) => {
            return fail(
                "新建子库要给 `--target <目标设备上的目录>`——子库总得知道往哪儿导。\n\
                 一律走读卡器（ADR-0015）：SD 卡挂成普通盘，给它在卡上的那个目录。",
            );
        }
    };
    if target_raw.is_none() {
        eprintln!(
            "⚠️ 这条目标路径不是有效的 UTF-8，存不下系统给的原始形式。\n\
             同步时会退回用规范化过的那一份去开目录——目录名要是分解形式、\n\
             又挂在分解敏感的文件系统上，可能会打不开。"
        );
    }
    let format = match &args.format {
        Some(format) => match adapter::find(format) {
            Some(adapter) => adapter.name().to_string(),
            None => {
                let names: Vec<&str> = adapter::all().iter().map(|a| a.name()).collect();
                return fail(format!(
                    "没有叫「{format}」的适配器。眼下带的是：{}。",
                    names.join("、")
                ));
            }
        },
        None => existing
            .as_ref()
            .map_or_else(|| DEFAULT_FORMAT.to_string(), |sub| sub.format.clone()),
    };
    // **挑不存在的档案当场拦下来。** 存一个名册里没有的名字，同步时会悄悄退回
    // 「不作声称」——于是用户以为配了 FAT32 的检查，其实一条都没查。
    let workspace = workspace_dir(args.common.workspace.as_deref());
    let capability = match &args.capability {
        Some(name) => {
            let roster = match romcat_core::capability::Roster::in_workspace(&workspace) {
                Ok(roster) => roster,
                Err(error) => return fail(format!("{error}")),
            };
            if roster.find(name).is_none() {
                return fail(format!(
                    "没有叫「{name}」的能力档案。眼下带的是：{}。\n\
                     `romcat capability` 看它们各自对着什么设备。",
                    roster.names().join("、"),
                ));
            }
            Some(name.clone())
        }
        None => existing.as_ref().and_then(|sub| sub.capability.clone()),
    };
    let capacity = if args.no_capacity {
        None
    } else {
        match &args.capacity {
            Some(text) => match sublibrary::rule::parse_size(text) {
                Some(bytes) => Some(bytes),
                None => {
                    return fail(format!(
                        "读不懂容量「{text}」。写法是 `512GB`、`476GiB`、`64MiB`——\n\
                         单位必须写全：`GiB` 是 1024³，`GB` 是 1000³，两者差 7%。"
                    ));
                }
            },
            None => existing.as_ref().and_then(|sub| sub.capacity),
        }
    };
    let sublibrary = Sublibrary {
        name: args.name.clone(),
        target,
        target_raw,
        format,
        capacity,
        capability,
    };
    if let Err(error) = catalog.put_sublibrary(&sublibrary) {
        return fail(format!("子库写不进中立库：{error}"));
    }
    println!(
        "{}子库「{}」：目标 {}，格式 {}，容量上限 {}，能力档案 {}。",
        if existing.is_some() {
            "已改"
        } else {
            "已建"
        },
        sublibrary.name,
        sublibrary.target,
        sublibrary.format,
        sublibrary
            .capacity
            .map_or_else(|| "不设限".to_string(), human_bytes),
        sublibrary
            .capability
            .as_deref()
            .unwrap_or(romcat_core::capability::DEFAULT_PROFILE),
    );
    if sublibrary.capability.is_none() {
        println!(
            "  没挑能力档案：**不转换、不检查**。`romcat capability` 看有哪些，\n\
             挑一份之后差量预览才会告诉你哪些在掌机上打不开、哪些放不进这张卡。"
        );
    }
    if existing.is_none() {
        println!(
            "下一步写规则：`romcat sublibrary rule {} --add \"平台=GB,GBA 且 中文=汉化\"`",
            sublibrary.name
        );
    }
    ExitCode::SUCCESS
}

fn run_sublibrary_list(args: &SubCommonArgs) -> ExitCode {
    let catalog = match args.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    let subs = match catalog.sublibraries() {
        Ok(subs) => subs,
        Err(error) => return fail(format!("中立库读不动：{error}")),
    };
    if subs.is_empty() {
        println!(
            "这个主库上还没有子库。\n\
             建一个：`romcat sublibrary set 掌机 --target /Volumes/SDCARD/Games --capacity 512GB`"
        );
        return ExitCode::SUCCESS;
    }
    println!("子库");
    println!("{}", "═".repeat(24));
    println!(
        "{}{}{}{}{}目标",
        pad("名字", 12),
        pad("规则", 6),
        pad("例外", 6),
        pad("容量上限", 12),
        pad("能力档案", 20),
    );
    for sub in &subs {
        // **不吞错误。** `unwrap_or(0)` 会让「读不动」与「一条都没有」印出来一模一样，
        // 而这两件事该做的处置完全相反。
        let counts = catalog
            .sublibrary_rules(&sub.name)
            .and_then(|rules| Ok((rules.len(), catalog.sublibrary_exceptions(&sub.name)?.len())));
        let (rules, exceptions) = match counts {
            Ok(counts) => counts,
            Err(error) => return fail(format!("中立库读不动：{error}")),
        };
        println!(
            "{}{}{}{}{}{}",
            pad(&sub.name, 12),
            pad(&rules.to_string(), 6),
            pad(&exceptions.to_string(), 6),
            pad(
                &sub.capacity.map_or_else(|| "—".to_string(), human_bytes),
                12
            ),
            pad(
                sub.capability
                    .as_deref()
                    .unwrap_or(romcat_core::capability::DEFAULT_PROFILE),
                20,
            ),
            sub.target,
        );
    }
    println!("\n一个主库上可以同时有好几个子库，**互不干扰**：规则各写各的，例外各记各的。");
    ExitCode::SUCCESS
}

fn run_sublibrary_remove(args: &SubNameArgs) -> ExitCode {
    let mut catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    match catalog.remove_sublibrary(&args.name) {
        Ok(true) => {
            println!("子库「{}」已删掉，它的规则与例外一起没了。", args.name);
            ExitCode::SUCCESS
        }
        Ok(false) => fail(format!("没有叫「{}」的子库。", args.name)),
        Err(error) => fail(format!("中立库写不动：{error}")),
    }
}

fn run_sublibrary_rule(args: &SubRuleArgs) -> ExitCode {
    let mut catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    if let Err(message) = load_sublibrary(&catalog, &args.name) {
        return fail(message);
    }
    if let Some(text) = &args.add {
        // **先读懂再写。** `add_rule` 收的就是读通了的规则，这一步过不去就写不进去。
        let rule = match sublibrary::Rule::parse(text) {
            Ok(rule) => rule,
            Err(error) => return fail(format!("这条规则读不懂：{error}")),
        };
        match catalog.add_rule(&args.name, &rule) {
            Ok(ordinal) => println!("规则 {ordinal} 已加进子库「{}」：{text}", args.name),
            Err(error) => return fail(format!("规则写不进中立库：{error}")),
        }
    }
    if let Some(ordinal) = args.remove {
        match catalog.remove_rule(&args.name, ordinal) {
            Ok(true) => println!("规则 {ordinal} 已从子库「{}」删掉。", args.name),
            Ok(false) => return fail(format!("子库「{}」里没有 {ordinal} 号规则。", args.name)),
            Err(error) => return fail(format!("中立库写不动：{error}")),
        }
    }
    let rules = match catalog.sublibrary_rules(&args.name) {
        Ok(rules) => rules,
        Err(error) => return fail(format!("中立库读不动：{error}")),
    };
    println!("\n子库「{}」的规则", args.name);
    println!("{}", "═".repeat(24));
    if rules.is_empty() {
        println!("（一条都没有）");
    } else {
        for rule in &rules {
            let broken = sublibrary::Rule::parse(&rule.text)
                .err()
                .map(|error| format!("   ⚠️ {error}"))
                .unwrap_or_default();
            println!("{}{}{broken}", pad(&rule.ordinal.to_string(), 6), rule.text);
        }
        println!("\n多条规则之间是**并集**，一条之内的子句用 ` 且 ` 连起来是**交集**。");
    }
    println!("\n能筛的维度");
    println!("{}", "─".repeat(16));
    for dimension in sublibrary::Dimension::all() {
        println!("{}{}", pad(dimension.label(), 8), dimension.hint());
    }
    println!(
        "\n运算符：= != ~（含有）<= < >= >。写法举例：\n  \
         平台=GB,GBA 且 中文=汉化\n  \
         平台=FC 且 体积<=4MiB\n  \
         作品~火焰纹章 且 年份>=2000"
    );
    ExitCode::SUCCESS
}

fn run_sublibrary_except(args: &SubExceptArgs) -> ExitCode {
    let mut catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    if let Err(message) = load_sublibrary(&catalog, &args.name) {
        return fail(message);
    }
    let wanted = [
        (
            args.include.as_deref(),
            Some(sublibrary::Exception::Include),
        ),
        (
            args.exclude.as_deref(),
            Some(sublibrary::Exception::Exclude),
        ),
        (args.forget.as_deref(), None),
    ];
    // **同一个变体不许在一条命令里领两个决定。** 挨个执行的话后一个会静默盖掉前一个，
    // 而两行「已记下」都打了出来——对着「例外优先于规则、永久记住」这条纪律，
    // 让用户以为自己记下的是第一个，是最坏的一种错。
    for (index, (key, _)) in wanted.iter().enumerate() {
        let Some(key) = key else { continue };
        if wanted[index + 1..]
            .iter()
            .any(|(other, _)| *other == Some(*key))
        {
            return fail(format!(
                "「{key}」在同一条命令里被给了不止一个决定。一次只对一个变体说一件事。"
            ));
        }
    }
    for (key, kind) in wanted {
        let Some(key) = key else { continue };
        match kind {
            Some(kind) => {
                if let Err(error) =
                    catalog.set_exception(&args.name, key, kind, args.note.as_deref())
                {
                    return fail(format!("例外写不进中立库：{error}"));
                }
                println!(
                    "例外已记下：子库「{}」{}「{key}」。",
                    args.name,
                    kind.label()
                );
                // 库里眼下没有这个变体也照记不误——例外是**永久记住**的（ADR-0016）。
                if matches!(catalog.variant(key), Ok(None)) {
                    println!(
                        "  ⚠️ 库里眼下没有这个变体（盘没插、目录改了名都会这样）。例外照旧记着。"
                    );
                }
            }
            None => match catalog.clear_exception(&args.name, key) {
                Ok(true) => println!("例外已忘掉：「{key}」从此听规则的。"),
                Ok(false) => {
                    return fail(format!("子库「{}」在「{key}」上没有例外。", args.name));
                }
                Err(error) => return fail(format!("中立库写不动：{error}")),
            },
        }
    }
    let exceptions = match catalog.sublibrary_exceptions(&args.name) {
        Ok(exceptions) => exceptions,
        Err(error) => return fail(format!("中立库读不动：{error}")),
    };
    println!("\n子库「{}」的例外", args.name);
    println!("{}", "═".repeat(24));
    if exceptions.is_empty() {
        println!("（一条都没有——眼下全听规则的）");
    } else {
        println!("{}{}变体", pad("方向", 6), pad("为什么", 20));
        for row in &exceptions {
            println!(
                "{}{}{}",
                pad(row.kind.label(), 6),
                pad(row.note.as_deref().unwrap_or("—"), 20),
                row.variant_key,
            );
        }
        println!("\n例外**优先于规则**：规则改了、重跑识别、重新成型，这几条一条都不动。");
    }
    ExitCode::SUCCESS
}

fn run_sublibrary_show(args: &SubShowArgs) -> ExitCode {
    let catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    let sublibrary = match load_sublibrary(&catalog, &args.name) {
        Ok(sublibrary) => sublibrary,
        Err(message) => return fail(message),
    };
    // 主库只读（ADR-0004）：报告不许落进主库。
    if let Some(root) = args.common.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let loaded = match catalog.selection(&args.name) {
        Ok(loaded) => loaded,
        Err(error) => return fail(format!("中立库读不动：{error}")),
    };
    let started = Instant::now();
    let facts = match sublibrary::facts(&catalog) {
        Ok(facts) => facts,
        Err(error) => return fail(format!("中立库读不动：{error}")),
    };
    let selected = sublibrary::select(&loaded.selection, &facts);
    let report = sublibrary::report::SelectionReport::build(
        catalog.location(),
        &sublibrary,
        &loaded,
        &facts,
        &selected,
    );
    if !args.quiet {
        let text = report.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "求值用了 {:.1} 秒，一个字节都没读主库、也没碰目标设备。选中 {} 个变体、{}。",
        started.elapsed().as_secs_f64(),
        thousands(report.picked),
        human_bytes(report.bytes),
    );
    if report.variants == 0 {
        eprintln!("库里一个变体都没有——先跑一次 `romcat scan`，再跑 `romcat shape`。");
    }
    if !write_json(args.json.as_deref(), &report) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 排一次计划要的全套东西。
///
/// **`plan` 与 `sync` 共用它，而且必须共用**：预览说的与同步要做的必须是同一份差量。
/// 两条命令各折一遍期望状态的话，两份账迟早漂开，而那正是 ADR-0016 那条硬要求
/// （同步前必须先呈现差量预览）要防的事。
/// 排一次计划。**算法在核心里**（[`sync::prepare`]）——命令行与界面得到的是同一份差量，
/// 而不是两条各自演化的代码（ADR-0016 那句「预览说的就是同步会做的」）。
fn prepare(
    catalog: &Catalog,
    common: &SubCommonArgs,
    name: &str,
    target: Option<&Path>,
    restore: bool,
    priorities: Option<&Path>,
) -> Result<sync::Prepared, String> {
    sync::prepare(
        catalog,
        &workspace_dir(common.workspace.as_deref()),
        name,
        &sync::Request {
            target,
            restore_missing: restore,
            priorities,
        },
    )
}

/// 排一次同步计划，也就是**差量预览**。
///
/// **只读**：选择集从中立库折出来，目标设备走 `LibraryFs` 那道只读接缝走一遍，
/// 排计划的那一步是纯函数。这条命令一个文件都不写（`--json` 那份报告除外）。
fn run_sublibrary_plan(args: &SubPlanArgs) -> ExitCode {
    let catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    // 主库只读（ADR-0004）：报告不许落进主库。
    if let Some(root) = args.common.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let started = Instant::now();
    let ready = match prepare(
        &catalog,
        &args.common,
        &args.name,
        args.target.as_deref(),
        args.restore,
        args.priorities.as_deref(),
    ) {
        Ok(ready) => ready,
        Err(message) => return fail(message),
    };
    if !args.quiet {
        let text = ready.plan.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "排计划用了 {:.1} 秒：读了中立库、看了一遍目标，**一个文件都没写**。\n\
         清单里 {} 个文件，目标上 {} 个；这次要动 {} 个。",
        started.elapsed().as_secs_f64(),
        thousands(ready.manifest.files.len() as u64),
        thousands(ready.actual.files.len() as u64),
        thousands(ready.plan.touched()),
    );
    warn_about(&ready);
    if !write_json(args.json.as_deref(), &ready.plan) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 折期望状态时那几件要说出口的怪事。**`plan` 与 `sync` 印同一份**，
/// 而且**与界面印同一份**——那几句话在核心里（[`sync::Prepared::concerns`]）。
fn warn_about(ready: &sync::Prepared) {
    for concern in ready.concerns() {
        eprintln!("{concern}");
    }
}

/// **同步**：把差量真正落到目标设备上。
///
/// 顺序是硬要求不是排版：**先印一遍差量预览，再动手**（ADR-0016）。计划里有删除时
/// 还要 `--yes` 点头——删掉的是维护者掌机上的东西，而这条命令是这个工具唯一有破坏力
/// 的动作。
fn run_sublibrary_sync(args: &SubSyncArgs, cancel: &CancelToken) -> ExitCode {
    let mut catalog = match args.common.open() {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    if let Some(root) = args.common.root.as_deref()
        && let Some(target) = args.json.as_deref()
        && let Err(message) = refuse_writing_into_library(root, target)
    {
        return fail(message);
    }
    let started = Instant::now();
    let ready = match prepare(
        &catalog,
        &args.common,
        &args.name,
        args.target.as_deref(),
        args.restore,
        args.priorities.as_deref(),
    ) {
        Ok(ready) => ready,
        Err(message) => return fail(message),
    };
    // **目标不许落在主库里。** `plan` 只读，指哪儿都无所谓；`sync` 从这张票起是真的
    // 往目标上写字节，一个手滑的 `--target` 就会在 10 TiB 只读主库里建目录写文件
    // （ADR-0004）。用中立库记着的主库根来判，`--root` 给不给都拦得住。
    if let Err(message) =
        refuse_target_in_library(&catalog, args.common.root.as_deref(), &ready.root)
    {
        return fail(message);
    }

    // ── 一、**先看预览**。这一步不是可选的（ADR-0016）：`--quiet` 都关不掉它，
    //    因为「删除前必须先呈现干跑预览」说的就是这一句。
    {
        let text = ready.plan.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    warn_about(&ready);
    if !write_json(args.json.as_deref(), &ready.plan) {
        return ExitCode::FAILURE;
    }
    if args.dry_run {
        eprintln!(
            "干跑：上面那份就是会做的事，**一个字节都没写**。\n\
             去掉 `--dry-run` 真的同步。"
        );
        return ExitCode::SUCCESS;
    }
    // ── 二、**删除要点头**。新增与更新不必——它们最坏是白传一遍，而删除删的是
    //    维护者掌机上的东西。
    if ready.plan.deletes.files > 0 && !args.yes {
        eprintln!(
            "这份计划里有 {} 个**删除**（{}）。看过上面的预览之后，加 `--yes` 再跑一次。\n\
             不想删某一个：`romcat sublibrary except {} --include <变体的键>` 把它留下。",
            thousands(ready.plan.deletes.files),
            human_bytes(ready.plan.deletes.bytes),
            args.name,
        );
        return ExitCode::FAILURE;
    }

    // ── 三、找到主库。搬 ROM 要真的去读它——这是这条命令里唯一需要盘在位的部分，
    //    而**只有真要搬 ROM 时才需要**：一趟只删文件、只重写元数据、或者一步都不用做
    //    的同步，盘不在位照样跑得完（ADR-0009 那句「扫描是唯一需要盘在位的操作」）。
    let library_root = if ready.needs_library() {
        match library_root_for(&catalog, args.library_root.as_deref()) {
            Ok(root) => Some(root),
            Err(message) => return fail(message),
        }
    } else {
        None
    };

    let sources = sync::Sources {
        library: &RealFs,
        library_root: library_root.as_deref(),
        target_root: &ready.root,
        from_pool: &ready.from_pool,
        generated: &ready.generated,
        // 探测的源那一头是**媒体池自己的临时目录**：两头都得是工具的地盘，
        // 拿主库里的文件去试链接会改到主库那一侧的 inode（ADR-0004）。
        link_probe_dir: Some(&ready.scratch),
        // **默认不缓存**（ADR-0017）：不给 `--convert-cache` 就边转边流式写进目标。
        convert_cache: args.convert_cache.as_deref(),
    };
    let outcome = match sync::execute::run(
        &ready.plan,
        &ready.desired,
        &ready.actual,
        &ready.manifest,
        &sources,
        cancel,
    ) {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("目标写不了：{error}")),
    };

    // ── 四、**清单落库**。中断的那一趟也要落——那份清单记的是「到中断为止目标上
    //    真实有什么」，下一趟才接得上。
    if let Err(error) = catalog.put_manifest(&args.name, &outcome.manifest) {
        eprintln!(
            "⚠️ **清单写不回中立库：{error}**\n\
             目标上的文件已经动过了，而清单还是旧的那一份——下一趟同步会把这次\n\
             放上去的东西当成「清单之外」，于是碰都不敢碰。先修好中立库再跑一次。"
        );
        return ExitCode::FAILURE;
    }

    {
        let text = outcome.render_text();
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(text.as_bytes());
        let _ = stdout.flush();
    }
    eprintln!(
        "同步用了 {:.1} 秒：动了 {} 个文件，折出 {} 个前端条目。",
        started.elapsed().as_secs_f64(),
        thousands(outcome.touched()),
        thousands(ready.entries),
    );
    if outcome.touched() == 0 {
        // 一步都不用做，**清单仍然要刷**：目标上少了、多了、被改过的那些，
        // 「清单更新为目标的真实状态」说的正是这一句。
        eprintln!("一个字节都没写；清单照目标眼下的样子记了一遍。");
    }
    if outcome.interrupted || outcome.gave_up || !outcome.failures.is_empty() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 这一趟去哪儿读主库。**算法在核心里**——界面那个「同步」按钮走的是同一条。
fn library_root_for(catalog: &Catalog, given: Option<&Path>) -> Result<PathBuf, String> {
    sync::prepare::library_root(catalog, given)
}

/// 目标落在主库里就拦下来（ADR-0004）。**这道红线在核心里**，界面与命令行共用一条。
fn refuse_target_in_library(
    catalog: &Catalog,
    given: Option<&Path>,
    target: &Path,
) -> Result<(), String> {
    sync::prepare::refuse_target_in_library(catalog, given, target)
}

#[derive(Debug, Args)]
struct CapabilityArgs {
    /// 只看这一份档案的全文，连每条声明的一手来源与核实日期
    #[arg(value_name = "档案")]
    name: Option<String>,

    /// 看这个文件里的名册，而不是内置的那一份
    #[arg(long, value_name = "文件")]
    file: Option<PathBuf>,

    /// 把内置名册导出成 TOML 底稿。改完放进工作目录叫 `capability.toml` 就自动生效
    #[arg(long, value_name = "文件")]
    dump_builtin: Option<PathBuf>,

    /// 工作目录：中立库存这里
    #[arg(long, value_name = "目录")]
    workspace: Option<PathBuf>,
}

/// 列出眼下生效的**能力档案**，或者看其中一份的全文。
///
/// **这条命令的全部意义是让矩阵可被复核。** ADR-0017：矩阵错误比不转换更糟——用户会
/// 以为工具已经处理妥当，直到在掌机上打不开才发现。于是 `show` 印的不是「支持哪些格式」
/// 这一行结论，而是**结论加它的出处**：哪个源码文件、哪份官方文档、哪天核实的，
/// 以及**它是不是已经陈旧**。
fn run_capability(args: &CapabilityArgs) -> ExitCode {
    if let Some(path) = &args.dump_builtin {
        match write_file(path, Roster::builtin_text().as_bytes()) {
            Ok(()) => {
                println!(
                    "内置能力档案已写入 {}。改完放进工作目录叫 {} 自动生效，\n\
                     或者 `romcat capability --file {}` 看一眼改成了什么。",
                    path.display(),
                    Roster::IN_WORKSPACE,
                    path.display(),
                );
            }
            Err(error) => {
                eprintln!("写不进 {}：{error}", path.display());
                return ExitCode::FAILURE;
            }
        }
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    let roster = match &args.file {
        Some(path) => Roster::load(path),
        None => Roster::in_workspace(&workspace),
    };
    let roster = match roster {
        Ok(roster) => roster,
        Err(error) => return fail(format!("{error}")),
    };
    let today = today();
    let text = match &args.name {
        Some(name) => match roster.find(name) {
            Some(profile) => profile.render_text(&today),
            None => {
                return fail(format!(
                    "没有叫「{name}」的能力档案。眼下带的是：{}。",
                    roster.names().join("、"),
                ));
            }
        },
        None => roster.render_text(&today),
    };
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
    if args.file.is_none() && !workspace.join(Roster::IN_WORKSPACE).exists() {
        eprintln!(
            "用的是**内置**那一份（工作目录里没有 {}）。",
            Roster::IN_WORKSPACE
        );
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

/// 取一趟**中文离线数据源**。**这是这个程序里第二个联网的子命令**（另一个是
/// `dat sync`），走的是同一道取数闸门。
fn run_zh_sync(args: &ZhSyncArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let manifest = match args.manifest.load(&workspace) {
        Ok(manifest) => manifest,
        Err(message) => return fail(message),
    };
    let rules = match args.rules.load(&workspace) {
        Ok(rules) => rules,
        Err(message) => return fail(message),
    };
    let mut store = match zh::store::Store::open(&workspace::zh_store_path(&workspace)) {
        Ok(store) => store,
        Err(error) => return fail(format!("中文索引打不开：{error}")),
    };
    // **先就地重建再取数**：结构版本一变，本机那份原件就够把索引补回来，
    // 而重建完指纹也记回去了——下面那句「指纹没变就整件跳过」于是照样成立。
    //
    // **`--dry-run` 不重建**：那一档说的是「只说这一趟会干什么，不取也不写」，而重建
    // 要读一份 435 MB 的原件再整份写回去，是这句话的反面。
    if args.dry_run {
        if let Some(pending) = store.rebuilding() {
            eprintln!(
                "（这一份索引的结构版本是 {}，本程序认得的是 {}——真跑一趟会先从本机那份\
                 原件 {} 就地重建。`--dry-run` 不动它。）",
                pending.was,
                zh::store::SCHEMA_VERSION,
                pending.dump,
            );
        }
    } else {
        heal_zh_store(&workspace, &mut store, &rules, Some(&manifest));
    }
    let options = zh::sync::Options {
        cache: workspace::zh_cache_dir(&workspace),
        full: args.full,
        dry_run: args.dry_run,
    };
    let fetcher = HttpFetcher::with_throttle(Duration::from_millis(args.throttle_ms));
    let library = RealFs;
    let started = Instant::now();
    let outcome = match zh::sync::sync(&fetcher, &library, &mut store, &manifest, &rules, &options)
    {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("取中文数据源失败：{error}")),
    };
    if outcome.dry_run {
        eprintln!(
            "这是 --dry-run：最新的一版是 {}（{}），什么都没取、什么都没写。",
            outcome.dump,
            human_bytes(outcome.bytes)
        );
        return ExitCode::SUCCESS;
    }
    if outcome.skipped {
        eprintln!(
            "指纹没变（{}），整件跳过——本机这份索引就是最新的。",
            outcome.dump
        );
    } else {
        eprintln!(
            "取回 {}（{}），读了 {} 条记录，留下 {} 条游戏条目，{:.1} 秒。",
            outcome.dump,
            human_bytes(outcome.bytes),
            thousands(outcome.records),
            thousands(outcome.games),
            started.elapsed().as_secs_f64(),
        );
    }
    print_zh_stats(&outcome.stats);
    if !write_json(args.json.as_deref(), &outcome) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 取一趟**第三方 TitleID 数据库**。**这是这个程序里第三个联网的子命令**
/// （另两个是 `dat sync` 与 `zh sync`），走的是同一道取数闸门——
/// `raw.githubusercontent.com` 本来就在白名单上，这一层一个主机都没往里加。
fn run_switch_sync(args: &SwitchSyncArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let mut store = match titledb::store::Store::open(&workspace::titledb_store_path(&workspace)) {
        Ok(store) => store,
        Err(error) => return fail(format!("TitleID 索引打不开：{error}")),
    };
    let options = titledb::sync::Options {
        cache: workspace::titledb_cache_dir(&workspace),
        full: args.full,
        dry_run: args.dry_run,
        regions: !args.no_regions,
    };
    let fetcher = HttpFetcher::with_throttle(Duration::from_millis(args.throttle_ms));
    let started = Instant::now();
    let outcome = match titledb::sync::sync(&fetcher, &mut store, &options) {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("取 titledb 失败：{error}")),
    };
    for file in &outcome.files {
        let what = if outcome.dry_run {
            "只排计划".to_string()
        } else if file.skipped {
            "ETag 没变，整件跳过".to_string()
        } else if file.downloaded {
            format!("下回来了，读出 {} 条", thousands(file.rows))
        } else {
            format!("原件在手边，重建出 {} 条", thousands(file.rows))
        };
        eprintln!("  {}：{what}", file.file);
    }
    if !outcome.dry_run {
        eprintln!("{:.1} 秒。", started.elapsed().as_secs_f64());
    }
    print_titledb_stats(&outcome.stats);
    if !write_json(args.json.as_deref(), &outcome) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 索引里现在有什么。**官中那两行是这份数据源对本项目的价值**：Switch 的中文覆盖是
/// 全库最好的（8,447 个 TitleID），而其中多区共用的那些，中文只是**语言属性**
/// 不是独立发行版（ADR-0019）。
fn print_titledb_stats(stats: &titledb::store::Stats) {
    println!(
        "TitleID 索引：{} 条 ContentId → (TitleID, 版本)，涉及 {} 个 TitleID",
        thousands(stats.ncas),
        thousands(stats.titles),
    );
    println!(
        "  eShop 元数据 {} 行、{} 个 TitleID；其中官中 {}，而**多区共用**同一个 TitleID 的有 {}",
        thousands(stats.entries),
        thousands(stats.named),
        thousands(stats.chinese),
        thousands(stats.chinese_shared),
    );
    if !stats.by_region.is_empty() {
        let mut line = String::new();
        for (region, count) in &stats.by_region {
            if !line.is_empty() {
                line.push('、');
            }
            line.push_str(&format!("{region} {}", thousands(*count)));
        }
        println!("  按区：{line}");
    }
}

/// ⭐ **读一份 Switch 转储的明文文件名表。**
///
/// 它存在的理由是这条纪律得看得见：**容器层不加密**，所以「哪个游戏的哪个版本」
/// 免密钥就说得出来。这个子命令一个密钥都不要、一个字节的 NCA 都不解开，
/// 每份只读几 KB。
fn run_switch_read(args: &SwitchReadArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let store = titledb::store::Store::open(&workspace::titledb_store_path(&workspace)).ok();
    let ready = store
        .as_ref()
        .and_then(|store| store.ready().ok())
        .unwrap_or(false);
    if !ready {
        eprintln!(
            "本机还没取过 titledb，只读得出 TitleID、读不出版本——`romcat switch sync` 取一次即可。"
        );
    }
    let library = RealFs;
    let mut rows: Vec<serde_json::Value> = Vec::new();
    for file in &args.files {
        let name = path::display(file);
        let facts = match romcat_core::fs::LibraryFs::open(&library, file) {
            Ok(mut handle) => {
                let mut source =
                    identify::switch::Seeked::new(handle.as_mut(), identify::switch::BUDGET);
                let facts = identify::switch::probe(&name, &mut source);
                println!("{name}（读了 {} 字节）", thousands(source.read()));
                facts
            }
            Err(error) => {
                println!("{name}：读不动（{error}）");
                continue;
            }
        };
        print_switch_facts(&facts, if ready { store.as_ref() } else { None });
        rows.push(serde_json::json!({ "文件": name, "事实": facts }));
    }
    if !write_json(args.json.as_deref(), &rows) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// 一份转储读出来的东西，摊开给人看。
fn print_switch_facts(facts: &identify::switch::Facts, store: Option<&titledb::store::Store>) {
    use identify::switch::{Kind, Structure, Wrapper};
    if let Some(note) = &facts.note {
        println!("  {note}");
    }
    let Some(wrapper) = facts.wrapper.as_deref().and_then(Wrapper::from_code) else {
        return;
    };
    let structure = facts
        .structure
        .as_deref()
        .and_then(Structure::from_code)
        .map_or("认不出", Structure::label);
    println!(
        "  容器 {}，结构 {structure}{}",
        wrapper.label(),
        if facts.compressed {
            "，**压缩过的**（ES-DE 看不见 .nsz / .xcz）"
        } else {
            ""
        }
    );
    if !facts.partitions.is_empty() {
        println!("  分区：{}", facts.partitions.join(" / "));
    }
    for name in &facts.entries {
        println!("    {name}");
    }
    for id in &facts.ids {
        let kind = Kind::of(&id.key).map_or("认不出", Kind::label);
        println!("  TitleID {}（{kind}）——{}", id.shown, id.from);
    }
    for content_id in &facts.content_ids {
        let found = store
            .and_then(|store| store.content(content_id).ok().flatten())
            .map(|content| format!("{} v{}", content.title_id, content.version));
        println!(
            "  ContentId {content_id} → {}",
            found.unwrap_or_else(|| "titledb 里查不到".to_string())
        );
    }
}

/// 索引里现在有什么。**老平台深度浅是事实不是缺陷**，按平台摆出来，
/// 免得「这个平台命中低」被误读成「匹配算法不行」。
fn print_zh_stats(stats: &zh::store::Stats) {
    println!(
        "中文离线数据源：{} 条游戏条目（dump {}）",
        thousands(stats.subjects),
        stats.dump
    );
    println!(
        "  有中文名 {}、说得出平台 {}、说得出年份 {}",
        thousands(stats.with_chinese),
        thousands(stats.with_platform),
        thousands(stats.with_year),
    );
    println!(
        "  有中文简介 {}、写得出类型 {}、开发商 {}、发行商 {}",
        thousands(stats.with_summary),
        thousands(stats.with_genre),
        thousands(stats.with_developer),
        thousands(stats.with_publisher),
    );
    if !stats.fields.is_empty() {
        // **这一版索引带了哪些字段**：换了这一行就该重刮一遍（它进刮削那一侧的输入指纹）。
        println!("  这一版取了：{}", stats.fields);
    }
    let mut line = String::new();
    for (platform, count) in stats.by_platform.iter().take(24) {
        if !line.is_empty() {
            line.push('、');
        }
        line.push_str(&format!("{platform} {}", thousands(*count)));
    }
    if !line.is_empty() {
        println!("  按平台：{line}");
    }
}

fn run_zh_find(args: &ZhFindArgs) -> ExitCode {
    let workspace = workspace_dir(args.workspace.as_deref());
    let rules = match args.rules.load(&workspace) {
        Ok(rules) => rules,
        Err(message) => return fail(message),
    };
    let mut store = match zh::store::Store::open(&workspace::zh_store_path(&workspace)) {
        Ok(store) => store,
        Err(error) => return fail(format!("中文索引打不开：{error}")),
    };
    heal_zh_store(&workspace, &mut store, &rules, None);
    let index = match store.load() {
        Ok(index) => index,
        Err(error) => return fail(format!("中文索引读不出来：{error}")),
    };
    if index.is_empty() {
        return fail("中文索引是空的。先跑一次 `romcat zh sync`。");
    }
    let tuning = args.tuning.tuning();
    for name in &args.names {
        let parsed = rules.parse(name);
        println!("{name}");
        println!(
            "  剥完剩「{}」{}{}",
            parsed.title,
            parsed
                .team
                .as_deref()
                .map_or_else(String::new, |team| format!("，汉化组「{team}」")),
            parsed
                .version
                .as_deref()
                .map_or_else(String::new, |version| format!("，版本 {version}")),
        );
        if !parsed.unknown.is_empty() {
            println!("  认不出的记号：{}", parsed.unknown.join("、"));
        }
        let mut found = 0;
        for (label, text) in parsed.queries() {
            let matches = index.lookup(
                &zh::Query {
                    text,
                    platform: args.platform.as_deref(),
                    year: args.year.or(parsed.year),
                },
                &tuning,
            );
            for one in &matches {
                found += 1;
                println!(
                    "  {:.2} {}「{}」 平台{} 年份{} —— 拿{}「{}」撞的",
                    one.score,
                    one.kind.label(),
                    one.matched,
                    one.platform.label(),
                    one.year.label(),
                    label,
                    text,
                );
                println!(
                    "        条目 {} 中文名「{}」 原名「{}」 平台「{}」 {}",
                    one.entry.id,
                    one.entry.name_cn,
                    one.entry.name,
                    one.entry.platform_text,
                    one.entry
                        .year
                        .map_or_else(|| "年份没写".to_string(), |year| format!("{year} 年")),
                );
            }
        }
        if found == 0 {
            println!("  一条都没撞上（阈值 {:.2}）", tuning.threshold);
        }
    }
    ExitCode::SUCCESS
}

/// 把名字还是乱码的那些容器重读一遍（票 11 的补救路径）。
///
/// 它**不遍历主库**：要读哪几个容器是中立库说的（`lossy = 1`），一个容器只读它的
/// 中央目录。真机上那是 21,901 条名字散在若干个容器里，全库重扫要 27 分钟，
/// 而这一趟只有几秒。
fn run_names_recheck(args: &NamesArgs, cancel: &CancelToken) -> ExitCode {
    let (slug, located_by) = match locate(args.library.as_deref(), args.root.as_deref()) {
        Ok(pair) => pair,
        Err(message) => return fail(message),
    };
    let workspace = workspace_dir(args.workspace.as_deref());
    let mut catalog = match open_catalog(&workspace, slug, args.root.as_deref()) {
        Ok(catalog) => catalog,
        Err(message) => return fail(message),
    };
    let root = match args.root.clone() {
        Some(root) => Some(root),
        None => catalog.library_root().ok().flatten().map(PathBuf::from),
    };
    let Some(root) = root else {
        return fail(format!(
            "不知道 {located_by} 那份主库在哪：给出主库根目录。重读容器要主库在位。"
        ));
    };
    let library = RealFs::new();
    let started = Instant::now();
    let mut last = Instant::now();
    let outcome = scan::names::recheck(&library, &mut catalog, &root, cancel, &mut |so_far| {
        if last.elapsed() >= Duration::from_secs(5) {
            last = Instant::now();
            eprintln!(
                "  已重读 {} / {} 个容器，改掉 {} 条名字",
                thousands(so_far.reread),
                thousands(so_far.containers),
                thousands(so_far.renamed),
            );
        }
    });
    let outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => return fail(format!("重读失败：{error}")),
    };
    println!(
        "名字还是乱码的容器 {} 个：重读成功 {}，**改掉 {} 条名字**，\n         仍然解不出来 {} 条（GBK / Big5 / Shift_JIS 都不是），\n         读不动 {} 个，文件已经变了、这一趟不动它的 {} 个。{:.1} 秒。",
        thousands(outcome.containers),
        thousands(outcome.reread),
        thousands(outcome.renamed),
        thousands(outcome.still_lossy),
        thousands(outcome.unreadable),
        thousands(outcome.moved_on),
        started.elapsed().as_secs_f64(),
    );
    if !outcome.samples.is_empty() {
        // **这几条是唯一可核对的产出**：三种编码字节分布相近，猜错会把一种乱码换成
        // 另一种，而一列数字看不出猜没猜对——一眼扫过这几行看得出。
        println!("\n改之前 → 改之后（人眼扫一遍，猜错了看得出来）");
        for (was, now) in &outcome.samples {
            println!("  {was}\n    → {now}");
        }
    }
    if outcome.renamed > 0 {
        println!("\n名字改了，下一趟 `romcat identify` 的文件名那一层就撞得上它们了。");
    }
    if outcome.interrupted {
        return ExitCode::from(130);
    }
    ExitCode::SUCCESS
}

fn run_names(args: &NamesArgs, cancel: &CancelToken) -> ExitCode {
    if args.recheck {
        return run_names_recheck(args, cancel);
    }
    let workspace = workspace_dir(args.workspace.as_deref());
    if let Some(path) = args.dump_builtin.as_deref() {
        if let Err(error) = fs::write(path, Rules::BUILTIN) {
            return fail(format!("写不进 {}：{error}", path::display(path)));
        }
        eprintln!("内置剥离规则已写到 {}。", path::display(path));
        if args.names.is_empty() {
            return ExitCode::SUCCESS;
        }
    }
    let rules = match args.rules.load(&workspace) {
        Ok(rules) => rules,
        Err(message) => return fail(message),
    };
    if args.names.is_empty() {
        eprintln!(
            "给几个文件名看看剥完剩什么，例如：\n  \
             romcat names '超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip'\n\
             要把名字还是乱码的那些容器重读一遍，用 `romcat names --recheck`。"
        );
        return ExitCode::SUCCESS;
    }
    for name in &args.names {
        let parsed = rules.parse(name);
        println!("{name}");
        println!("  正题「{}」", parsed.title);
        if let Some(chinese) = &parsed.chinese {
            println!("  中文「{chinese}」");
        }
        if let Some(latin) = &parsed.latin {
            println!("  拉丁「{latin}」");
        }
        if let Some(team) = &parsed.team {
            println!("  汉化组「{team}」");
        }
        if let Some(version) = &parsed.version {
            println!("  版本「{version}」");
        }
        if let Some(year) = parsed.year {
            println!("  年份 {year}");
        }
        if !parsed.languages.is_empty() {
            println!("  语言地区 {}", parsed.languages.join("、"));
        }
        for strip in &parsed.stripped {
            println!("  剥掉 {:<10} 「{}」", strip.why.label(), strip.what);
        }
        if !parsed.unknown.is_empty() {
            println!(
                "  ⚠ 认不出的记号：{}——照着它往规则里补一条",
                parsed.unknown.join("、")
            );
        }
    }
    ExitCode::SUCCESS
}

/// 同步一趟 DAT。**联网的子命令之一**（另两个是 `zh sync` 与 `scrape --profile 在线`），
/// 每一个 URL 都过同一道闸门。
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
    let outcome = match dat_sync::run(&fetcher, &mut repo, &registry, &options) {
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
