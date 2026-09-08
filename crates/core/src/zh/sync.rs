//! **取一次中文离线数据源**：下载 Bangumi Archive 的 dump，就地建成本机索引。
//!
//! ## 走的是票 06 那道取数闸门，不另开一条路
//!
//! 每一个 URL 都过 [`dat::guard`](crate::dat::guard)——那是这个库里**唯一一处**
//! 「可以连哪儿」的声明。dump 的两个落点本来就在白名单上（`raw.githubusercontent.com`
//! 与 `github.com`，后者会 302 到 `release-assets.githubusercontent.com`，而闸门
//! **看每一跳**），所以这一层一个字都不必往白名单里加。**这是有意的**：加一条白名单
//! 是一个需要想清楚的决定，不该由一张新数据源清单顺手带进来。
//!
//! ## 增量：指纹没变就整件跳过
//!
//! dump 每周三导出一次，而这份索引一建就是几个月不用动。指纹取的是
//! `aux/latest.json` 里那个 `digest`（官方算好的 sha256）加文件名——**用官方给的那个数
//! 而不是自己算**：自己算要先把 435 MB 下回来，而增量的全部意义正是不下那 435 MB。
//!
//! **这个指纹只盯着远端那份 dump**，盯不住本机这两张表（平台清单与剥离规则）。
//! 补一条平台别名之后再跑一趟不带 `--full` 的 `zh sync`，会如实报「本机这份就是最新的」
//! 而整件跳过——dump 确实没换。要让新补的那条别名生效**得走 `--full`**
//! （[`Options::full`]）。这不是遗漏：本机那两张表变没变，不下那 435 MB 是问不出来的，
//! 而拿「那两张表现在长什么样」当依据，会在「改了别名但还没重建」时反过来说谎。
//!
//! ## 折出来的那张表跟着索引一起落库
//!
//! 建索引时每条条目的平台名都折成本工具的平台名（`entry_of` → [`platform_of`]），
//! 折出来的那一串决定交叉校验。**折叠发生在建索引那一刻**，所以「这一趟折出来了什么」
//! 要跟着索引一起记进库（[`zh::PlatformFold`](crate::zh::PlatformFold)），
//! 由刮削那一侧当输入指纹使。少了它，补一条别名重建之后刮削会整片复用旧的采集记录，
//! 新折得动的条目永远不产出。
//!
//! ## 一次 435 MB 的下载，一次 960 MB 的流
//!
//! zip 里那份 `subject.jsonlines` 解开是 960 MB，**绝不整份读进内存**：
//! [`container::read_entries`](crate::container::read_entries) 交出来的是一条流，
//! 这里一行一行读过去，只把游戏条目留下来（8.7 万条，几十 MB）。
//! 本机只剩几个 GiB，而这条链路上任何一处「先读进内存再说」都会当场撑爆。
//!
//! ## 那一遍流要几分钟：报得出进度、按得停
//!
//! **取数与重建走的是同一遍流**（[`read_dump`]），所以「走到哪儿了」与「停下」落在
//! 这一层，两条路一并有了：[`Context`] 里那个回调每读一批报一次，那个中断信号每读
//! 一条看一眼。这正是[任务](crate::task)那一层说的「一步内部还想更细的，把
//! `Handle::cancel` 那个信号往下传」——**不是另造一套**。
//!
//! **那 435 MB 的下载也按同一条接住**：同一个中断信号折成「还要不要接着搬」递给
//! [`Fetcher::download_while`]，
//! 于是按下停下的代价是**读完手上那一块**，不是把剩下那 400 多 MB 读完。
//! 半截的那个 `.partial` 当场删掉——留着的话下一趟会以为原件已经在手边了。
//!
//! **停下的地方是干净的**：收手在两条记录之间，那时一个字都还没写进库
//! （换结构与写新数据在 [`Store::replace`] 那一个事务里），手上那份索引原样可用。

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::container::{self, Demand, ReadPlan};
use crate::dat::fetch::{FetchError, Fetcher};
use crate::filename::Rules;
use crate::fs::LibraryFs;
use crate::platform::Manifest;
use crate::scan::CancelToken;
use crate::task::Halted;

use super::store::{Stats, Store, StoreError};
use super::{Entry, PlatformFold, dump};

/// 最新那一版 dump 的地址索引。官方仓库里的一个文件，几百字节。
pub const LATEST_URL: &str =
    "https://raw.githubusercontent.com/bangumi/Archive/master/aux/latest.json";

/// dump 里那份条目表叫什么。
pub const SUBJECTS: &str = "subject.jsonlines";

/// 取数跑不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// 取数失败（含闸门拦下）。
    ///
    /// **这一支特意不带 `#[from]`**：`FetchError` 上有一支
    /// [`Halted`](FetchError::Halted)，而 `#[from]` 生成的那个转换会把它一并折进这一格
    /// ——于是随手 `?` 一下就把「被按停了」悄悄变成了「取数失败」。折支的活由底下
    /// 那条手写的 `From` 干，`?` 就再也漏不掉。
    #[error(transparent)]
    Fetch(FetchError),
    /// 索引读写失败。
    #[error(transparent)]
    Store(#[from] StoreError),
    /// 下回来的东西不是预期的样子。
    #[error("{0}")]
    Malformed(String),
    /// 本地文件读写失败。
    #[error("读写 {path} 失败：{source}")]
    Io {
        /// 出问题的路径。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// **被按停了。** 停在两条记录之间——库里一个字都还没写，手上那份索引原样可用。
    ///
    /// 它与别的几样分开一格，是因为**这不是失败**：报成「这份原件读不动」的话，
    /// 命令行与界面都会劝用户去重下那 435 MB，而它一个字节都没坏。
    #[error(transparent)]
    Halted(#[from] Halted),
}

impl From<FetchError> for SyncError {
    /// **被叫停不折成一句「取数失败」。**
    ///
    /// 折的是**支**不是话：[`FetchError::Halted`] 进 [`SyncError::Halted`]，
    /// 别的照旧带着自己那句话进 [`SyncError::Fetch`]。手写这一条而不是用 `#[from]`，
    /// 是因为 `#[from]` 会把两样并成一格——那正是这一层要拆掉的形状。
    fn from(error: FetchError) -> Self {
        match error {
            FetchError::Halted(halted) => Self::Halted(halted),
            error => Self::Fetch(error),
        }
    }
}

/// 取数的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 取回来的原件放哪。**留着原件**：改一条平台别名之后重建索引，不必把那 435 MB
    /// 再下一遍（与 `dat::cache_dir` 同一条道理）。
    pub cache: PathBuf,
    /// 无视指纹，整份**重建索引**。
    ///
    /// **它不等于「重下」**：原件的文件名里带着这一版的日期（`dump-2026-09-01.…zip`），
    /// 同名就是同一版，手边有就直接用。改一条平台别名之后要重建索引，走的正是这条——
    /// 那时再下一遍 415 MB 纯属白花。
    ///
    /// **它也是那条别名唯一的生效途径**：不带 `--full` 的那一趟只比远端那份 dump 的
    /// 指纹，本机这两张表变没变它一个字都不知道（见模块文档「增量」那一节）。
    /// 重建之后刮削那一侧会跟着重采——折出来的那张表进了输入指纹
    /// （[`zh::PlatformFold`](crate::zh::PlatformFold)），而不是像从前那样整片跳过。
    pub full: bool,
    /// 只说这一趟会干什么，不取也不写。
    pub dry_run: bool,
}

/// 一趟取数干了什么。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Synced {
    /// 这一版 dump 叫什么。
    pub dump: String,
    /// 它的指纹。
    pub fingerprint: String,
    /// 这一版有多大。
    pub bytes: u64,
    /// **这一趟真去下载了没有。**
    ///
    /// 原件已经在手边时是 `false`（`Options::full` 的文档：文件名带着日期，
    /// 同名就是同一版）。报告里那句「取回 435 MB」少了这一格就是句假话——
    /// 而「一个字节都没下」正是重建这条路最要紧的承诺。
    pub downloaded: bool,
    /// **指纹没变，整件跳过了**。
    pub skipped: bool,
    /// 只排了计划没真干。
    pub dry_run: bool,
    /// 从 dump 里读了多少条记录。
    pub records: u64,
    /// 其中留下来的游戏条目。
    pub games: u64,
    /// 索引建完之后长什么样。
    pub stats: Stats,
}

/// 读那份原件读到哪儿了。**那几分钟里唯一说得出话的东西。**
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// 已经读过几条记录。
    pub records: u64,
    /// 其中留下来的游戏条目。
    pub games: u64,
    /// 那份条目表已经读了多少字节（**解开之后的**）。
    pub bytes: u64,
    /// 它解开一共多少字节。**这才是「还剩多少」的分母**：一共几条记录事先问不出来，
    /// 而未压缩大小在 zip 的目录里就写着（`InnerEntry::size`），一个字节都不必先解。
    pub total: u64,
}

/// 一趟取数或重建的**把手**：报进度、看有没有被叫停。
///
/// 两样都可以没有——[`Context::unattended`] 就是「谁也不看着」的那一份。
/// 界面那一侧把 `progress` 接到 [`Handle::tick`](crate::task::Handle::tick)、
/// 把 `cancel` 接到 [`Handle::cancel`](crate::task::Handle::cancel)，命令行那一侧
/// 每隔几秒打一行、接的是 Ctrl-C 那个信号。**两边按的是同一件事。**
#[derive(Default)]
pub struct Context<'a> {
    /// 被叫停就在**两条记录之间**收手，那时一个字都还没写进库。
    /// `None` 表示这一趟没人叫得停。
    pub cancel: Option<&'a CancelToken>,
    /// 每读一批报一次；**开读之前先报一次**（那一下带着分母），
    /// 于是「读原件真的开始了」这件事说得出口。`None` 表示没人看着。
    pub progress: Option<&'a mut dyn FnMut(Progress)>,
}

impl<'a> Context<'a> {
    /// 谁也不看着的一份：没人叫停，进度也没人收。
    #[must_use]
    pub fn unattended() -> Self {
        Self::default()
    }

    /// 接着一个**已经在手的中断信号**，进度没人收。
    #[must_use]
    pub fn cancelled_by(cancel: &'a CancelToken) -> Self {
        Self {
            cancel: Some(cancel),
            progress: None,
        }
    }

    fn report(&mut self, at: Progress) {
        if let Some(progress) = self.progress.as_deref_mut() {
            progress(at);
        }
    }

    fn stopped(&self) -> bool {
        self.cancel.is_some_and(CancelToken::is_cancelled)
    }
}

/// 读多少条记录报一次进度。
///
/// 每条报一次是白花几百万次调用；十万条报一次那几分钟里就又没话说了。
/// 两千条上下是零点几秒的活，正好是「看着它在动」需要的密度。
/// （**看有没有被叫停不按这个数走**：那是一次原子读，每条看一眼才停得快。）
const REPORT_EVERY: u64 = 2_000;

/// 取一次。
///
/// `library` 只用来读**本机缓存目录里那个 zip**——它不是主库，但读法一模一样，
/// 所以复用同一个只读接缝（`LibraryFs`）而不是另开一套文件读取。
///
/// **等着重建的那一份也走这条**，而且只走这一趟：先取到远端那一版，指纹一样就拿
/// 本机手上那份原件就地重建，换了新版就取新版——**无论哪一头，那 960 MB 只流一遍**。
/// 先重建一遍再来问远端的话，远端一有新版，刚解完的那一份当场被盖掉
/// （挂单 `Q15`、`Q31`）。
///
/// # Errors
/// 取数、解析或写索引失败时返回错误；被叫停时返回 [`SyncError::Halted`]。
pub fn sync(
    fetcher: &dyn Fetcher,
    library: &dyn LibraryFs,
    store: &mut Store,
    manifest: &Manifest,
    rules: &Rules,
    options: &Options,
    ctx: &mut Context<'_>,
) -> Result<Synced, SyncError> {
    let latest = fetcher.get(LATEST_URL)?;
    let text = String::from_utf8_lossy(&latest.body).into_owned();
    let release = Release::parse(&text)?;
    let mut out = Synced {
        dump: release.name.clone(),
        fingerprint: release.fingerprint(),
        bytes: release.size,
        dry_run: options.dry_run,
        ..Synced::default()
    };
    // **等着重建的那一份不许走「指纹没变就整件跳过」这条路。** 它的指纹确实没变——
    // 变的是本程序，而库里那份数据这一版读不了。跳过就等于让它永远读不了，
    // 而这条路本来是留给「本机这份就是最新的」的。
    let pending = store.rebuilding().is_some();
    if !options.full && !pending && store.fingerprint()? == Some(out.fingerprint.clone()) {
        out.skipped = true;
        out.stats = store.stats()?;
        return Ok(out);
    }
    if options.dry_run {
        out.stats = store.stats()?;
        return Ok(out);
    }
    std::fs::create_dir_all(&options.cache).map_err(|source| SyncError::Io {
        path: crate::path::display(&options.cache),
        source,
    })?;
    let file = options.cache.join(&release.name);
    // **原件已经在手边就不再下一遍**，`--full` 也不例外：文件名里带着这一版的日期，
    // 同名就是同一版。改一条平台别名重建索引时省的正是这 415 MB。
    if !file.exists() {
        // **那 435 MB 里按下停下要当场有反应。** 中断信号折成「还要不要接着搬」递进去，
        // 于是收手的代价是读完手上那一块，不是把剩下那 400 多 MB 读完（挂单 `Q150`）。
        let cancel = ctx.cancel;
        // 收手折成 `SyncError::Halted` 而不是「取数失败」，是 `From<FetchError>` 那条
        // 干的（就在这个文件上头）——那份原件一个字节都没坏，缓存里也没有半截留下
        // （`HttpFetcher::download_while` 把 `.partial` 删了），报成失败的话，
        // 命令行与界面都会劝人去重下那 435 MB。
        fetcher.download_while(&release.url, &file, &|| {
            cancel.is_none_or(|token| !token.is_cancelled())
        })?;
        out.downloaded = true;
    }
    let (entries, records, fold) = read_dump(library, &file, manifest, rules, ctx)?;
    out.records = records;
    out.games = u64::try_from(entries.len()).unwrap_or(u64::MAX);
    store.replace(&entries, &release.name, &out.fingerprint, &fold)?;
    out.stats = store.stats()?;
    Ok(out)
}

/// `aux/latest.json` 里那几样。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Release {
    name: String,
    url: String,
    digest: String,
    size: u64,
}

impl Release {
    fn parse(text: &str) -> Result<Self, SyncError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|error| SyncError::Malformed(format!("{LATEST_URL} 不是 JSON：{error}")))?;
        let field = |key: &str| -> Result<String, SyncError> {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .ok_or_else(|| SyncError::Malformed(format!("{LATEST_URL} 里没有 {key}")))
        };
        Ok(Self {
            name: field("name")?,
            url: field("browser_download_url")?,
            digest: field("digest").unwrap_or_default(),
            size: value
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
        })
    }

    /// 变没变的依据。**用官方算好的那个 sha256**——自己算要先把 435 MB 下回来，
    /// 而增量的全部意义正是不下那 435 MB。
    fn fingerprint(&self) -> String {
        format!("{}|{}", self.name, self.digest)
    }
}

/// 把 dump 里那份条目表流一遍，留下游戏条目。
///
/// 交出来的第三样是**这一趟平台折叠实际折出来的那张表**（[`PlatformFold`]）：它得跟着
/// 索引一起落库，刮削那一侧拿它当输入指纹。攒在这儿而不是事后从条目上倒推——
/// 条目身上只剩折完的那一串与原文拼成的一行（`Entry::platform_text`），
/// 哪个原文折出了哪个平台名已经看不出来了。
fn read_dump(
    library: &dyn LibraryFs,
    file: &Path,
    manifest: &Manifest,
    rules: &Rules,
    ctx: &mut Context<'_>,
) -> Result<(Vec<Entry>, u64, PlatformFold), SyncError> {
    let listing = container::list(library, file).map_err(|error| {
        SyncError::Malformed(format!("{} 读不动：{error}", crate::path::display(file)))
    })?;
    if !listing
        .contents
        .entries
        .iter()
        .any(|entry| entry.path == SUBJECTS)
    {
        return Err(SyncError::Malformed(format!(
            "{} 里没有 {SUBJECTS}——这不是一份 Bangumi Archive 的 dump",
            crate::path::display(file)
        )));
    }
    let plan = ReadPlan::new(&listing, |entry| {
        if entry.path == SUBJECTS {
            Demand::All
        } else {
            Demand::Skip
        }
    });
    let mut entries: Vec<Entry> = Vec::new();
    let mut records = 0u64;
    let mut bytes = 0u64;
    let mut total = 0u64;
    let mut fold = PlatformFold::default();
    // **被叫停不顺着回调那条路回来。** 回调只交得出 `io::Error`，那条路上的一切都会被
    // 报成「这份原件读不动」——而一次干净的停下，那份原件一个字节都没坏。
    let mut halted = false;
    container::read_entries(library, file, &listing, &plan, &mut |entry, reader| {
        total = entry.size;
        // **开读之前先报一次。** 命令行那一侧靠这头一下才知道「读原件真的开始了」，
        // 而不是等到几分钟之后才第一次说话（挂单 `Q25`）；分母也是这一下给的。
        ctx.report(Progress {
            total,
            ..Progress::default()
        });
        // **一行一行读**：解开是 960 MB，整份读进内存会当场撑爆本机剩下的那几个 GiB。
        let mut lines = BufReader::with_capacity(1 << 20, reader);
        let mut line = String::new();
        loop {
            // **在读下一条之前看一眼**：收手的地方就在两条记录之间，
            // 上一条已经整条读完，这一条一个字都还没碰。
            if ctx.stopped() {
                halted = true;
                return Ok(());
            }
            line.clear();
            let read = lines.read_line(&mut line)?;
            if read == 0 {
                break;
            }
            records += 1;
            bytes += u64::try_from(read).unwrap_or(0);
            if records.is_multiple_of(REPORT_EVERY) {
                ctx.report(Progress {
                    records,
                    games: u64::try_from(entries.len()).unwrap_or(u64::MAX),
                    bytes,
                    total,
                });
            }
            let Ok(row) = serde_json::from_str::<dump::Row>(line.trim_end()) else {
                // 一行读不动就跳过这一行。**整份 dump 不该为一行畸形的 JSON 作废**，
                // 而它确实可能出现——那是一份用户共同维护的 wiki 导出来的东西。
                continue;
            };
            if !row.is_game() {
                continue;
            }
            entries.push(entry_of(&row, manifest, rules, &mut fold));
        }
        Ok(())
    })
    .map_err(|error| SyncError::Malformed(format!("{SUBJECTS} 读不动：{error}")))?;
    if halted {
        return Err(Halted.into());
    }
    // 收尾再报一次，末尾那不足一批的几条不至于永远差着没说。
    ctx.report(Progress {
        records,
        games: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        bytes,
        total,
    });
    Ok((entries, records, fold))
}

/// 把一条记录折成索引里的一条。
///
/// 折出来的每一对都记进 `fold`——**那是这份索引与「当时那两张表」之间唯一留得下来的
/// 凭据**，刮削那一侧靠它认出「补过别名了，这些条目该重采」。`platform_of` 折不动的
/// 一个都不记（[`PlatformFold`] 的文档说了为什么够用、为什么有界）。
fn entry_of(row: &dump::Row, manifest: &Manifest, rules: &Rules, fold: &mut PlatformFold) -> Entry {
    let raw = row.platforms();
    let mut platforms: Vec<String> = Vec::new();
    for text in &raw {
        if let Some(platform) = platform_of(manifest, rules, text) {
            // **去重之前就记**：同一条条目里写了两遍的那一次也是一次实实在在的折叠，
            // 而这张表本来就按对去重。
            fold.record(text, platform);
            if !platforms.iter().any(|it| it == platform) {
                platforms.push(platform.to_string());
            }
        }
    }
    Entry {
        id: row.id,
        name: row.name.trim().to_string(),
        name_cn: row.name_cn.trim().to_string(),
        aliases: row.aliases(),
        year: row.year(),
        platforms,
        platform_text: raw.join("、"),
        // **简介一个字都不改**：换行、开头那两个全角空格、以及数据源自带的排版都是内容的
        // 一部分（规格 18）。连两头都不掐——`　　以细腻的画风…` 那两个全角空格是排版，
        // `trim` 会把它们当空白扫掉。整段只有空白时才算没有。
        summary: if row.summary.trim().is_empty() {
            String::new()
        } else {
            row.summary.clone()
        },
        genres: row.genres(),
        developers: row.developers(),
        publishers: row.publishers(),
    }
}

/// **从本机那份原件重建索引**：一个网络请求都不发，也不要用户重下那 435 MB。
///
/// 结构版本一变就走这条（[`Store::rebuilding`]）。原件就是取数时留在缓存目录里的那个
/// zip——文件名里带着这一版的日期，**同名就是同一版**（`Options::full` 的文档说的是
/// 同一件事）。重建完把原来那个指纹记回去，下一趟 `zh sync` 才认得出「本机这份就是
/// 最新的」而整件跳过。
///
/// 四种结局都不是错误：不必重建、原件不在手边（那时才轮到用户跑一趟 `zh sync`）、
/// 重建了、被按停了。
///
/// **这一趟要几分钟**（读+解 960 MB 再整份写库），所以它收一份 [`Context`]：
/// 每读一批报一次进度，每读一条看一眼有没有被叫停。被叫停时收手在两条记录之间，
/// 那时 [`Store::replace`] 一个字都还没写——那份等着重建的索引原样躺着，
/// 下一趟接着重建就是。
///
/// # Errors
/// 原件读不动或者写索引失败时返回错误。**被按停不是错误**，它是
/// [`Rebuilt::Halted`]。
pub fn rebuild(
    library: &dyn LibraryFs,
    store: &mut Store,
    manifest: &Manifest,
    rules: &Rules,
    cache: &Path,
    ctx: &mut Context<'_>,
) -> Result<Rebuilt, SyncError> {
    let Some(pending) = store.rebuilding().cloned() else {
        return Ok(Rebuilt::NotNeeded);
    };
    let file = cache.join(&pending.dump);
    if pending.dump.is_empty() || !file.exists() {
        return Ok(Rebuilt::NoOriginal {
            was: pending.was,
            dump: pending.dump,
        });
    }
    let (entries, _, fold) = match read_dump(library, &file, manifest, rules, ctx) {
        Ok(read) => read,
        // **按停下不叫「重建没成」。** 报成失败的话，命令行与界面都会顺着那句话劝用户
        // 去跑一趟 `zh sync`——而那正是这条路存在的理由：不必重下那 435 MB。
        Err(SyncError::Halted(_)) => {
            return Ok(Rebuilt::Halted {
                was: pending.was,
                dump: pending.dump,
            });
        }
        Err(error) => return Err(error),
    };
    let games = u64::try_from(entries.len()).unwrap_or(u64::MAX);
    store.replace(&entries, &pending.dump, &pending.fingerprint, &fold)?;
    Ok(Rebuilt::Done {
        was: pending.was,
        dump: pending.dump,
        games,
    })
}

/// 一趟重建的结局。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rebuilt {
    /// 结构版本对得上，什么都没做。
    NotNeeded,
    /// 该重建，但**本机没有那份原件**——只有这一种结局才轮到用户跑一趟 `zh sync`。
    NoOriginal {
        /// 旧索引的结构版本。
        was: u32,
        /// 缺的是哪一份原件；上一版索引连 dump 名都没记时是空串。
        dump: String,
    },
    /// 从本机那份原件重建好了。
    Done {
        /// 旧索引的结构版本。
        was: u32,
        /// 从哪一份原件重建的。
        dump: String,
        /// 重建出多少条游戏条目。
        games: u64,
    },
    /// **被按停了。** 停在两条记录之间，库里一个字都没写——那份索引原样等着，
    /// 下一趟接着重建。
    Halted {
        /// 旧索引的结构版本。
        was: u32,
        /// 本来要从哪一份原件重建。
        dump: String,
    },
}

/// 中文数据源写的那个平台名，在本工具里叫什么。
///
/// **先问平台清单再问剥离规则**：`GBA` / `NDS` / `PSP` 两边写法一模一样，平台清单
/// 那张 `目录` 别名表就折得动；剥离规则里那张表只补它折不动的那些
/// （`Nintendo Switch`、`PlayStation Vita`）。两张表的分工写在
/// [`filename` 的规则文件](crate::filename::Rules::BUILTIN)里。
#[must_use]
pub fn platform_of<'a>(manifest: &'a Manifest, rules: &'a Rules, text: &str) -> Option<&'a str> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(platform) = manifest.platform_for_dir(trimmed) {
        return Some(platform.name.as_str());
    }
    // 去掉空格与连字符再问一遍：`Wii U` 与 `wiiu`、`Mega Drive` 与 `megadrive`。
    let squeezed = crate::filename::fold_platform(trimmed);
    if !squeezed.is_empty()
        && let Some(platform) = manifest.platform_for_dir(&squeezed)
    {
        return Some(platform.name.as_str());
    }
    rules.platform_alias(trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_json_读得出下载地址与指纹() {
        // 这一段是 2026-09-02 实测取回来的那一份，原样。
        let text = r#"{
  "browser_download_url": "https://github.com/bangumi/Archive/releases/download/archive/dump-2026-09-01.210329Z.zip",
  "digest": "sha256:5d02c90e8317c47f17f912b614f25c7611cd08297bf428450ebf7a00a9821a39",
  "name": "dump-2026-09-01.210329Z.zip",
  "size": 435891841
}"#;
        let release = Release::parse(text).expect("读得出来");
        assert_eq!(release.name, "dump-2026-09-01.210329Z.zip");
        assert_eq!(release.size, 435_891_841);
        assert!(release.fingerprint().contains("5d02c90e"));
        // 下载地址在闸门的白名单上——这一层一个字都不必往白名单里加。
        assert!(crate::dat::guard::check(&release.url).is_ok());
        assert!(crate::dat::guard::check(LATEST_URL).is_ok());
    }

    #[test]
    fn 结构版本一变就从本机那份原件重建_不下载不报错() {
        // 规格 16：「存储结构升级时自己从本地那份原件重建，这样我不必重新下载 435 MB。」
        // 这条测试里**没有 fetcher**——重建这条路上根本没有发请求的地方。
        let dir = crate::testing::temp_dir("zh-rebuild");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let path = dir.path().join("zh.sqlite3");
        let manifest = Manifest::builtin();
        let rules = Rules::builtin();
        let library = crate::fs::RealFs;
        {
            let mut store = Store::open(&path).expect("开得起来");
            let (entries, records, fold) = read_dump(
                &library,
                &cache.join(原件名),
                &manifest,
                &rules,
                &mut Context::unattended(),
            )
            .expect("原件读得动");
            assert_eq!(records, 1, "读到几条记录");
            assert_eq!(entries.len(), 1, "留下几条游戏条目");
            store
                .replace(&entries, 原件名, 指纹, &fold)
                .expect("写得进去");
        }
        // 把版本改回上一格：这就是「拿一份旧结构版本的索引打开」。
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);

        let mut store = Store::open(&path).expect("旧版本照样打得开");
        assert!(store.load().expect("读得回来").is_empty(), "先扫干净");
        let outcome = rebuild(
            &library,
            &mut store,
            &manifest,
            &rules,
            &cache,
            &mut Context::unattended(),
        )
        .expect("重建得了");
        assert_eq!(
            outcome,
            Rebuilt::Done {
                was: 1,
                dump: 原件名.to_string(),
                games: 1,
            }
        );
        // 重建完索引又满了，而且带上了新结构才有的那几样。
        let index = store.load().expect("读得回来");
        assert_eq!(index.len(), 1);
        assert_eq!(index.entries()[0].genres, vec!["ACT"]);
        // 简介不进内存（`Store::load` 的文档），单独一条路读得出来。
        assert_eq!(
            store.summary(4).expect("读得回来").as_deref(),
            Some("　　以细腻的画风…")
        );
        // **指纹记回去了**：下一趟 `zh sync` 才认得出「本机这份就是最新的」而整件跳过。
        assert_eq!(store.fingerprint().expect("读得到").as_deref(), Some(指纹));
        assert_eq!(store.rebuilding(), None, "重建完就不再等着重建了");
    }

    #[test]
    fn 等着重建的那一份不许因为指纹没变而整件跳过() {
        // 指纹确实没变——变的是本程序，而库里那份数据这一版读不了。跳过就等于让它
        // 永远读不了，而那条路本来是留给「本机这份就是最新的」的。
        let dir = crate::testing::temp_dir("zh-sync-pending");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(&[], 原件名, 指纹, &PlatformFold::default())
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);

        let mut store = Store::open(&path).expect("旧版本照样打得开");
        assert!(store.rebuilding().is_some());
        assert_eq!(store.fingerprint().expect("读得到").as_deref(), Some(指纹));

        let fetcher = crate::dat::CannedFetcher::new().with(LATEST_URL, 一份_latest_json());
        let outcome = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache,
                full: false,
                dry_run: false,
            },
            &mut Context::unattended(),
        )
        .expect("取得动");
        assert!(!outcome.skipped, "指纹一样也不许跳过——那份索引这一版读不了");
        assert_eq!(outcome.games, 1);
        // **原件在手边就没下载**：`CannedFetcher` 根本没备那个下载地址，
        // 真去下会当场失败；`downloaded` 那一格把这件事摆到报告里。
        assert!(!outcome.downloaded, "原件在手边，一个字节都没下");
        assert_eq!(store.load().expect("读得回来").len(), 1);
        assert!(store.rebuilding().is_none());
    }

    #[test]
    fn 远端有新版时不先拿旧原件白重建一遍() {
        // 挂单 `Q15`、`Q31`：从前是先就地重建一遍再来问远端。远端一有新版，刚解完的
        // 那一份当场被下回来的新版盖掉——那几分钟（读+解 960 MB）整个白花，`--full`
        // 更是把同一份原件解两遍。**取数自己这条路只解一遍**：先取到远端那一版，
        // 再决定读哪份原件。
        let dir = crate::testing::temp_dir("zh-sync-newer");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下旧原件");
        std::fs::write(cache.join(新原件名), 另一份原件()).expect("写得下新原件");

        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(&[], 原件名, 指纹, &PlatformFold::default())
                .expect("写得进去");
        }
        // 把版本改回上一格：这就是「一份等着重建的索引」。
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);
        let mut store = Store::open(&path).expect("旧版本照样打得开");
        assert!(store.rebuilding().is_some(), "它正等着重建");

        // **数「开读之前那一下」就是数解了几遍原件**：每读一份原件报且只报一次
        // `records == 0` 的那一下（这两份固件各有一条记录，收尾那一下报的是 1）。
        let mut 解了几遍 = 0u32;
        let mut progress = |at: Progress| {
            if at.records == 0 {
                解了几遍 += 1;
            }
        };
        let fetcher = crate::dat::CannedFetcher::new()
            .with(LATEST_URL, 一份_latest_json指着(新原件名, "def"));
        let outcome = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache,
                full: false,
                dry_run: false,
            },
            &mut Context {
                cancel: None,
                progress: Some(&mut progress),
            },
        )
        .expect("取得动");

        assert_eq!(解了几遍, 1, "那份原件只解一遍");
        assert_eq!(outcome.dump, 新原件名, "读的是新那一版");
        assert!(!outcome.downloaded, "新原件也在手边，一个字节都没下");
        let index = store.load().expect("读得回来");
        assert_eq!(index.entries()[0].id, 7, "库里落的是新那一版的条目");
        assert_eq!(
            store.fingerprint().expect("读得到").as_deref(),
            Some(新指纹),
            "指纹记的也是新那一版"
        );
        assert!(store.rebuilding().is_none(), "不再等着重建了");
    }

    #[test]
    fn full_撞上等着重建的索引时原件也只解一遍() {
        // 挂单 `Q31`：从前 `zh sync --full` 撞上一份等着重建的索引，那 435 MB 原件解两遍
        // ——先地重建一遍，紧接着 `--full` 永远不走「指纹没变就跳过」，同一个文件又读
        // 一遍、`replace` 一遍。**取数这条路本身只读一遍**，`--full` 也不例外。
        let dir = crate::testing::temp_dir("zh-sync-full-pending");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(&[], 原件名, 指纹, &PlatformFold::default())
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);
        let mut store = Store::open(&path).expect("旧版本照样打得开");
        assert!(store.rebuilding().is_some(), "它正等着重建");

        let mut 解了几遍 = 0u32;
        let mut progress = |at: Progress| {
            if at.records == 0 {
                解了几遍 += 1;
            }
        };
        let fetcher = crate::dat::CannedFetcher::new().with(LATEST_URL, 一份_latest_json());
        let outcome = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache,
                full: true,
                dry_run: false,
            },
            &mut Context {
                cancel: None,
                progress: Some(&mut progress),
            },
        )
        .expect("取得动");

        assert_eq!(解了几遍, 1, "那份原件只解一遍");
        assert!(!outcome.skipped, "`--full` 本来就不走「指纹没变就跳过」");
        assert!(
            !outcome.downloaded,
            "原件在手边就不再下一遍，`--full` 也不例外"
        );
        assert_eq!(store.load().expect("读得回来").len(), 1);
        assert!(store.rebuilding().is_none(), "不再等着重建了");
    }

    #[test]
    fn 读原件时报得出读到第几条与还剩多少() {
        // 挂单 `Q25`：这一趟要读+解 960 MB，几分钟起步。这几分钟里一个字都不打的话，
        // 用户看见的是一个像死掉了的进程。
        let dir = crate::testing::temp_dir("zh-progress");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let mut 报了: Vec<Progress> = Vec::new();
        let mut progress = |at: Progress| 报了.push(at);
        let (entries, records, _) = read_dump(
            &crate::fs::RealFs,
            &cache.join(原件名),
            &Manifest::builtin(),
            &Rules::builtin(),
            &mut Context {
                cancel: None,
                progress: Some(&mut progress),
            },
        )
        .expect("原件读得动");
        assert_eq!((records, entries.len()), (1, 1));

        // **开读之前那一下**：分母有了，分子还是 0——「读原件真的开始了」这件事
        // 说得出口，而不是等几分钟之后才第一次说话。
        let 头一下 = *报了.first().expect("开读之前先报一次");
        assert_eq!(头一下.records, 0);
        assert!(
            头一下.total > 0,
            "「还剩多少」的分母从 zip 的目录里读，一个字节都不必先解"
        );
        // **收尾再报一次**：末尾那不足一批的几条不至于永远差着没说。
        let 末一下 = *报了.last().expect("收尾再报一次");
        assert_eq!((末一下.records, 末一下.games), (1, 1));
        assert!(末一下.bytes > 0 && 末一下.bytes <= 末一下.total);
        assert_eq!(末一下.total, 头一下.total, "分母一路不变");
    }

    #[test]
    fn 重建按得停_停下之后那份索引原样等着下一趟() {
        // 「能停」比「能取消」严格：停在两条记录之间，`Store::replace` 一个字都还没写。
        let dir = crate::testing::temp_dir("zh-rebuild-halt");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下原件");

        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(&[], 原件名, 指纹, &PlatformFold::default())
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);
        let mut store = Store::open(&path).expect("旧版本照样打得开");

        let cancel = CancelToken::new();
        cancel.cancel();
        let 停了 = rebuild(
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &cache,
            &mut Context::cancelled_by(&cancel),
        )
        .expect("按停下不是错误");
        assert_eq!(
            停了,
            Rebuilt::Halted {
                was: 1,
                dump: 原件名.to_string(),
            }
        );
        assert!(
            store.rebuilding().is_some(),
            "那份索引原样等着——报成失败的话，用户会被劝去重下那 435 MB"
        );
        assert_eq!(store.fingerprint().expect("读得到").as_deref(), Some(指纹));

        // **停下之后仍然重建得了**，这才叫「什么都没丢」。
        let 又一趟 = rebuild(
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &cache,
            &mut Context::unattended(),
        )
        .expect("重建得了");
        assert!(
            matches!(又一趟, Rebuilt::Done { games: 1, .. }),
            "{又一趟:?}"
        );
    }

    #[test]
    fn 取数读原件时按停_手上那份索引原样可用() {
        // 库里已经有一份读得出来的索引（旧那一版），远端换了新版。读新原件读到一半
        // 按停，库里仍然是旧那一份——换结构与写新数据在同一个事务里，中途停就是
        // 整个没发生。
        let dir = crate::testing::temp_dir("zh-sync-halt");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件()).expect("写得下旧原件");
        std::fs::write(cache.join(新原件名), 另一份原件()).expect("写得下新原件");

        let mut store = Store::open(&dir.path().join("zh.sqlite3")).expect("开得起来");
        let (entries, _, fold) = read_dump(
            &crate::fs::RealFs,
            &cache.join(原件名),
            &Manifest::builtin(),
            &Rules::builtin(),
            &mut Context::unattended(),
        )
        .expect("原件读得动");
        store
            .replace(&entries, 原件名, 指纹, &fold)
            .expect("写得进去");

        let cancel = CancelToken::new();
        cancel.cancel();
        let fetcher = crate::dat::CannedFetcher::new()
            .with(LATEST_URL, 一份_latest_json指着(新原件名, "def"));
        let error = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache,
                full: false,
                dry_run: false,
            },
            &mut Context::cancelled_by(&cancel),
        )
        .expect_err("按停了");
        // **它得落在「被按停」那一支上。** 任务台按支分「停了」与「失败」
        // （`task::Board::settle`，`sources::refetch` 把这一支折成 `Cutoff::Halted`），
        // 不看这句话说了什么——从前这儿还断过一句 `error.to_string() == Halted.to_string()`，
        // 那时判据是字符串比对，差一个字就记成失败；现在那一句断的只是个巧合。
        assert!(matches!(error, SyncError::Halted(_)), "{error:?}");

        let index = store.load().expect("读得回来");
        assert_eq!(index.entries()[0].id, 4, "手上那份索引原样可用");
        assert_eq!(
            store.fingerprint().expect("读得到").as_deref(),
            Some(指纹),
            "指纹也还是旧那一版——下一趟照样认得出该重读"
        );
    }

    #[test]
    fn 本机没有那份原件时如实说一句而不是硬报错() {
        let dir = crate::testing::temp_dir("zh-rebuild-missing");
        let path = dir.path().join("zh.sqlite3");
        {
            let mut store = Store::open(&path).expect("开得起来");
            store
                .replace(
                    &[],
                    "dump-2026-09-01.210329Z.zip",
                    "sha256:abc",
                    &PlatformFold::default(),
                )
                .expect("写得进去");
        }
        let conn = rusqlite::Connection::open(&path).expect("开得起来");
        conn.execute(
            "UPDATE meta SET value = '1' WHERE key = 'schema_version'",
            [],
        )
        .expect("改得动");
        drop(conn);
        let mut store = Store::open(&path).expect("旧版本照样打得开");
        let outcome = rebuild(
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            dir.path(),
            &mut Context::unattended(),
        )
        .expect("不是错误");
        assert_eq!(
            outcome,
            Rebuilt::NoOriginal {
                was: 1,
                dump: "dump-2026-09-01.210329Z.zip".to_string(),
            }
        );
    }

    /// 一个**边下边问**的下载替身：那份原件一块一块地给，每块之前问一次要不要收手。
    ///
    /// `CannedFetcher::download` 是一次 `fs::write`——那一下根本没有「下到一半」可言，
    /// 拿它测不出「按下停下之后等不等它下完」。真的那一个（`HttpFetcher`）走的是
    /// `dat::fetch::copy_while`，一块 1 MiB；这一个只是把同一副问法摆出来，
    /// 好验**这一层有没有把中断信号递下去**。
    struct 边下边问 {
        canned: crate::dat::CannedFetcher,
        /// 那份原件，整份下完时落到盘上的就是它。
        正文: Vec<u8>,
        /// 那 435 MB 折成几块。
        共几块: u64,
        /// 收手（或者下完）时搬了几块。
        搬了几块: std::sync::atomic::AtomicU64,
    }

    impl Fetcher for 边下边问 {
        fn head(&self, url: &str) -> Result<crate::dat::fetch::Head, FetchError> {
            self.canned.head(url)
        }

        fn get(&self, url: &str) -> Result<crate::dat::fetch::Fetched, FetchError> {
            self.canned.get(url)
        }

        fn download(&self, url: &str, to: &Path) -> Result<crate::dat::fetch::Head, FetchError> {
            self.download_while(url, to, &|| true)
        }

        fn download_while(
            &self,
            _url: &str,
            to: &Path,
            keep_going: &dyn Fn() -> bool,
        ) -> Result<crate::dat::fetch::Head, FetchError> {
            use std::sync::atomic::Ordering;
            for 第几块 in 0..self.共几块 {
                if !keep_going() {
                    self.搬了几块.store(第几块, Ordering::Relaxed);
                    // **半截的不留**：真的那一个把 `.partial` 删掉，这儿一个字节都没落。
                    return Err(Halted.into());
                }
            }
            self.搬了几块.store(self.共几块, Ordering::Relaxed);
            std::fs::write(to, &self.正文).map_err(|source| FetchError::Io {
                path: crate::path::display(to),
                source,
            })?;
            Ok(crate::dat::fetch::Head::default())
        }

        fn post(
            &self,
            url: &str,
            headers: &[(&str, &str)],
            body: &[u8],
        ) -> Result<crate::dat::fetch::Fetched, FetchError> {
            self.canned.post(url, headers, body)
        }
    }

    /// 那 435 MB 按一块 1 MiB 折成几块。
    const 那份原件几块: u64 = 435_891_841_u64.div_ceil(1 << 20);

    fn 摆一个边下边问的() -> 边下边问 {
        边下边问 {
            canned: crate::dat::CannedFetcher::new().with(LATEST_URL, 一份_latest_json()),
            正文: 一份原件(),
            共几块: 那份原件几块,
            搬了几块: std::sync::atomic::AtomicU64::new(0),
        }
    }

    #[test]
    fn 那_435_兆的下载按停之后当场收手_不等它下完() {
        // 挂单 `Q150`：从前这一层只把中断信号接到「读那份原件」那几分钟上，下载那一段
        // 根本没人问——按下停下之后要等 435 MB 全下完才轮得到它。
        //
        // **这一条验的是这一层有没有把信号递下去**（那条一块一块搬的循环由
        // `dat::fetch` 自己的测试钉）。**两头都跑**：按了停下的那一趟当场收手，
        // 没按的那一趟整份下完——只跑前一半的话，把信号接到一个恒假的东西上也会绿。
        let dir = crate::testing::temp_dir("zh-sync-按停");
        let cache = dir.path().join("cache");

        // 一、按了停下：第一块都不搬就收手，缓存里一个字节都不留。
        let fetcher = 摆一个边下边问的();
        let mut store = Store::open(&dir.path().join("zh-停.sqlite3")).expect("开得起来");
        let token = crate::scan::CancelToken::new();
        token.cancel();
        let error = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache: cache.clone(),
                full: false,
                dry_run: false,
            },
            &mut Context::cancelled_by(&token),
        )
        .expect_err("按过停下就该收手");
        // **它是「被按停了」那一档，不是「取数失败」。** 折成失败的话，命令行与界面
        // 都会劝人去重下那 435 MB，而它一个字节都没坏。
        assert!(matches!(error, SyncError::Halted(_)), "{error:?}");
        assert_eq!(
            fetcher.搬了几块.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "按了停下却还是搬了几块",
        );
        assert!(
            !cache.join(原件名).exists(),
            "收手了却在缓存里留下一份原件——下一趟会以为它已经在手边",
        );

        // 二、没按停下：同一副替身整份下完，那 416 块一块不少。
        let fetcher = 摆一个边下边问的();
        let mut store = Store::open(&dir.path().join("zh-不停.sqlite3")).expect("开得起来");
        let outcome = sync(
            &fetcher,
            &crate::fs::RealFs,
            &mut store,
            &Manifest::builtin(),
            &Rules::builtin(),
            &Options {
                cache,
                full: false,
                dry_run: false,
            },
            &mut Context::unattended(),
        )
        .expect("没人叫停就该下得完");
        assert!(outcome.downloaded, "这一趟该是真下了一份");
        assert_eq!(
            fetcher.搬了几块.load(std::sync::atomic::Ordering::Relaxed),
            那份原件几块,
            "没人叫停却没搬完",
        );
    }

    /// 缓存目录里那份原件叫什么。文件名里带着这一版的日期，**同名就是同一版**。
    const 原件名: &str = "dump-2026-09-01.210329Z.zip";

    /// 那一版的指纹，形状与 [`Release::fingerprint`] 一致。
    const 指纹: &str = "dump-2026-09-01.210329Z.zip|sha256:abc";

    /// 下一版 dump 叫什么。文件名里那个日期换了，就是换了一版。
    const 新原件名: &str = "dump-2026-09-08.210329Z.zip";

    /// 那一版的指纹。
    const 新指纹: &str = "dump-2026-09-08.210329Z.zip|sha256:def";

    /// `aux/latest.json` 说的正是本机手上这一版。
    fn 一份_latest_json() -> Vec<u8> {
        一份_latest_json指着(原件名, "abc")
    }

    /// `aux/latest.json` 说远端最新的是哪一版。
    fn 一份_latest_json指着(name: &str, digest: &str) -> Vec<u8> {
        format!(
            "{{\"browser_download_url\": \
             \"https://github.com/bangumi/Archive/releases/download/archive/{name}\",\
             \"digest\": \"sha256:{digest}\", \"name\": \"{name}\", \"size\": 1}}"
        )
        .into_bytes()
    }

    /// 另一版 dump：里头那条记录的编号不一样，认得出这一趟读的是哪一份。
    fn 另一份原件() -> Vec<u8> {
        let line = r#"{"id":7,"type":4,"name":"メタルスラッグX","name_cn":"合金弹头X","infobox":"{{Infobox Game\r\n|平台= NDS\r\n|游戏类型= ACT\r\n}}","platform":4001,"date":"2009-01-01","meta_tags":["ACT","NDS","游戏"]}"#;
        crate::testing::container::zip_container(&[
            crate::testing::container::ZipEntrySpec::stored(
                SUBJECTS,
                format!("{line}\n").into_bytes(),
            ),
        ])
    }

    /// 一份最小的 dump：一个 zip，里头一份 `subject.jsonlines`，一条游戏记录。
    fn 一份原件() -> Vec<u8> {
        let line = r#"{"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7","infobox":"{{Infobox Game\r\n|别名={\r\n[Metal Slug 7]\r\n}\r\n|平台= NDS\r\n|游戏类型= ACT\r\n|开发= SNK\r\n|发行= 世嘉\r\n}}","platform":4001,"summary":"　　以细腻的画风…","date":"2008-07-17","meta_tags":["ACT","NDS","游戏"]}"#;
        crate::testing::container::zip_container(&[
            crate::testing::container::ZipEntrySpec::stored(
                SUBJECTS,
                format!(
                    "{line}
"
                )
                .into_bytes(),
            ),
        ])
    }

    #[test]
    fn 平台名先问平台清单再问剥离规则() {
        let manifest = Manifest::builtin();
        let rules = Rules::builtin();
        // 平台清单那张目录别名表折得动的。
        assert_eq!(platform_of(&manifest, &rules, "GBA"), Some("GBA"));
        assert_eq!(platform_of(&manifest, &rules, "NDS"), Some("NDS"));
        // 去掉空格之后折得动的。
        assert_eq!(platform_of(&manifest, &rules, "Wii U"), Some("WIIU"));
        // 只有剥离规则那张表折得动的。
        assert_eq!(
            platform_of(&manifest, &rules, "Nintendo Switch"),
            Some("SWITCH")
        );
        assert_eq!(platform_of(&manifest, &rules, "PS Vita"), Some("PSV"));
        // 两张表都不认的一律留空——**认不出就留空**，硬折一个平台出来会让交叉校验说谎。
        assert_eq!(platform_of(&manifest, &rules, "iOS"), None);
        assert_eq!(platform_of(&manifest, &rules, ""), None);
    }

    /// 一份最小的 dump，平台那一格写什么由调用方定。
    fn 一份原件写着(平台: &str) -> Vec<u8> {
        let line = format!(
            r#"{{"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7","infobox":"{{{{Infobox Game\r\n|平台= {平台}\r\n|游戏类型= ACT\r\n}}}}","platform":4001,"date":"2008-07-17","meta_tags":["ACT","游戏"]}}"#
        );
        crate::testing::container::zip_container(&[
            crate::testing::container::ZipEntrySpec::stored(
                SUBJECTS,
                format!("{line}\n").into_bytes(),
            ),
        ])
    }

    #[test]
    fn 建索引时折出来的那张表跟着索引一起落库() {
        // 折叠发生在**建索引那一刻**，用的是那一刻的平台清单与别名表。所以「这一趟
        // 折出来了什么」得记进库——本机那两张表事后再变，也改不了库里这一份是怎么折的。
        let dir = crate::testing::temp_dir("zh-fold-meta");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件写着("NDS")).expect("写得下原件");

        let mut store = Store::open(&dir.path().join("zh.sqlite3")).expect("开得起来");
        let (entries, _, fold) = read_dump(
            &crate::fs::RealFs,
            &cache.join(原件名),
            &Manifest::builtin(),
            &Rules::builtin(),
            &mut Context::unattended(),
        )
        .expect("原件读得动");
        assert_eq!(fold.line(), "共 1 对\nNDS=NDS", "折出来的那一对记下了");
        store
            .replace(&entries, 原件名, 指纹, &fold)
            .expect("写得进去");

        // 落进 `meta`，而且**索引读回来时带在身上**——刮削那一侧拿它当输入指纹。
        assert_eq!(
            store.meta(crate::zh::store::PLATFORM_FOLD).expect("读得到"),
            Some("共 1 对\nNDS=NDS".to_string())
        );
        assert_eq!(
            store.load().expect("读得回来").platform_fold(),
            "共 1 对\nNDS=NDS"
        );
    }

    #[test]
    fn 补一条平台别名重建之后折出来的那张表跟着变() {
        // 用户的原话。`--full` 是那条别名唯一的生效途径（`Options::full` 的文档），
        // 而重建之后刮削那一侧认得出「这份索引不是原来那份」，靠的就是这张表变了。
        let dir = crate::testing::temp_dir("zh-fold-alias");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        // 两张表都不认的一个平台名。
        std::fs::write(cache.join(原件名), 一份原件写着("任天堂红白机")).expect("写得下原件");
        let manifest = Manifest::builtin();
        let 读一遍 = |rules: &Rules| {
            read_dump(
                &crate::fs::RealFs,
                &cache.join(原件名),
                &manifest,
                rules,
                &mut Context::unattended(),
            )
            .expect("读得动")
        };

        let (折不动的条目, _, 折不动) = 读一遍(&Rules::builtin());
        assert!(折不动的条目[0].platforms.is_empty(), "折不动");
        assert!(折不动.is_empty());
        assert_eq!(折不动.line(), "共 0 对");

        let 规则文件 = dir.path().join("rules.toml");
        std::fs::write(
            &规则文件,
            "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"任天堂红白机\"\n\"是\" = \"FC\"\n",
        )
        .expect("写得下规则");
        let (折得动的条目, _, 折得动) = 读一遍(&Rules::load(&规则文件).expect("读得进来"));
        assert_eq!(折得动的条目[0].platforms, vec!["FC".to_string()]);
        assert_eq!(折得动.line(), "共 1 对\n任天堂红白机=FC");
        assert_ne!(折不动.line(), 折得动.line(), "两趟折出来的表不一样");
    }

    #[test]
    fn 与这份_dump_无关的那些别名不进折出来的那张表() {
        // **多盖会误伤。** 平台清单里加一个这份 dump 里根本没人写的平台、或者补一条
        // 谁都没用上的别名，折出来的东西一个字都没变，就不该引发全片重采——
        // 盖的是**折叠真发生了什么**，不是那两张表现在长什么样。
        let dir = crate::testing::temp_dir("zh-fold-unrelated");
        let cache = dir.path().join("cache");
        std::fs::create_dir_all(&cache).expect("建得出缓存目录");
        std::fs::write(cache.join(原件名), 一份原件写着("NDS")).expect("写得下原件");
        let manifest = Manifest::builtin();
        let 读一遍 = |rules: &Rules| {
            read_dump(
                &crate::fs::RealFs,
                &cache.join(原件名),
                &manifest,
                rules,
                &mut Context::unattended(),
            )
            .expect("读得动")
            .2
        };

        let 规则文件 = dir.path().join("rules.toml");
        std::fs::write(
            &规则文件,
            "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"世嘉土星\"\n\"是\" = \"SS\"\n",
        )
        .expect("写得下规则");
        assert_eq!(
            读一遍(&Rules::builtin()).line(),
            读一遍(&Rules::load(&规则文件).expect("读得进来")).line(),
            "这份 dump 里没人写「世嘉土星」，补它不改变这一趟折出来的东西"
        );
    }
}
