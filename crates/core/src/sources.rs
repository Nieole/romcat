//! **数据源**在本机那几份镜像现在是什么状况。
//!
//! 三个源——DAT 仓库、**中文离线源**、Switch 数据库——各是一份本地镜像，取一次用很久
//! （README 的「三份数据分得很清」）。这个模块只回答一句话：**这一份取回来了没有、
//! 有多少条、什么时候取的**。
//!
//! ## 为什么这一层在核心库里
//!
//! 它是三份库各自的读法拼起来的一句结论，而不是画法（ADR-0005）。新用户最容易卡的
//! 「扫完了怎么没认出来」，答案十有八九是某个源那一行是空的——那句话该由核心说出来，
//! 命令行与界面各说一遍就会有两个版本。
//!
//! ## 读不动不等于空
//!
//! 某一份库打不开时那一行的 `state` 是 [`SourceState::Broken`]，**不是「还没取回」**
//! （ADR-0021 那条纪律）：前者要人去看一眼，后者按一下「取回」就好。

use std::path::Path;
use std::time::Duration;

use crate::dat::{DatRepo, HttpFetcher, Registry};
use crate::fs::RealFs;
use crate::platform::Manifest;
use crate::task::Handle;
use crate::workspace;
use crate::zh;

/// 一份镜像现在处在哪一档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceState {
    /// 取回来了，有这么多条。
    Ready {
        /// 条数。
        records: u64,
        /// 上次取回的时刻（UNIX 纪元起的秒）；这份库没记时刻时是 `None`。
        fetched_at: Option<i64>,
    },
    /// **还没取回。** 这一档要在界面上被明确标出来。
    Missing,
    /// 库在那儿但读不动。
    Broken {
        /// 读不动的那句话。
        why: String,
    },
}

/// 一份**数据源**镜像的状况。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceStatus {
    /// 这个源叫什么。
    pub name: &'static str,
    /// 眼下这一档。
    pub state: SourceState,
    /// 这一份都覆盖了些什么，给人看的一句。
    pub coverage: String,
    /// **没取回的话会怎样**。一句话说清代价，那正是「扫完了怎么没认出来」的答案。
    pub cost: &'static str,
}

impl SourceStatus {
    /// 取回来了吗。
    #[must_use]
    pub fn ready(&self) -> bool {
        matches!(self.state, SourceState::Ready { .. })
    }

    /// 有多少条；没取回或者读不动时是 0。
    #[must_use]
    pub fn records(&self) -> u64 {
        match &self.state {
            SourceState::Ready { records, .. } => *records,
            _ => 0,
        }
    }

    /// 上次取回的时刻。
    #[must_use]
    pub fn fetched_at(&self) -> Option<i64> {
        match &self.state {
            SourceState::Ready { fetched_at, .. } => *fetched_at,
            _ => None,
        }
    }
}

/// 一个源用哪个名字点名重取。命令行与界面共用这一套。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// DAT 仓库（`romcat dat sync`）。
    Dat,
    /// 中文离线源（`romcat zh sync`）。
    Chinese,
    /// Switch 数据库（`romcat switch sync`）。
    Switch,
}

impl Source {
    /// 三个源，界面上的固定次序。
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Dat, Self::Chinese, Self::Switch]
    }

    /// 这个源叫什么。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dat => "DAT 仓库",
            Self::Chinese => "中文离线源",
            Self::Switch => "Switch 数据库",
        }
    }

    /// 没取回的话会怎样。
    #[must_use]
    pub fn cost(self) -> &'static str {
        match self {
            Self::Dat => "取回前撞不上任何官方发行版，识别只剩文件名那条路",
            Self::Chinese => "取回前中文名、简介、类型、开发商、发行商一个都补不上",
            Self::Switch => "取回前 Switch 平台认不出任何东西",
        }
    }

    /// 查一查这一份镜像现在什么状况。**不联网。**
    #[must_use]
    pub fn survey(self, workspace: &Path) -> SourceStatus {
        let (state, coverage) = match self {
            Self::Dat => survey_dat(workspace),
            Self::Chinese => survey_chinese(workspace),
            Self::Switch => survey_switch(workspace),
        };
        SourceStatus {
            name: self.label(),
            state,
            coverage,
            cost: self.cost(),
        }
    }
}

/// 三个源各查一遍。**不联网**，也不碰主库。
#[must_use]
pub fn survey(workspace: &Path) -> Vec<SourceStatus> {
    Source::all()
        .into_iter()
        .map(|source| source.survey(workspace))
        .collect()
}

fn survey_dat(workspace: &Path) -> (SourceState, String) {
    let path = workspace::dat_repo_path(workspace);
    if !path.exists() {
        return (SourceState::Missing, String::new());
    }
    let repo = match DatRepo::open(&path) {
        Ok(repo) => repo,
        Err(error) => return (broken(&error.to_string()), String::new()),
    };
    match (repo.coverage(), repo.dat_count()) {
        (Ok((0, _)), _) => (SourceState::Missing, String::new()),
        (Ok((records, fetched_at)), Ok(dats)) => (
            SourceState::Ready {
                records,
                fetched_at,
            },
            format!("{dats} 份 DAT"),
        ),
        (Err(error), _) | (_, Err(error)) => (broken(&error.to_string()), String::new()),
    }
}

fn survey_chinese(workspace: &Path) -> (SourceState, String) {
    let path = workspace::zh_store_path(workspace);
    if !path.exists() {
        return (SourceState::Missing, String::new());
    }
    let store = match zh::store::Store::open(&path) {
        Ok(store) => store,
        Err(error) => return (broken(&error.to_string()), String::new()),
    };
    let stats = match store.stats() {
        Ok(stats) => stats,
        Err(error) => return (broken(&error.to_string()), String::new()),
    };
    if stats.subjects == 0 {
        return (SourceState::Missing, String::new());
    }
    // 「带中文简介的占几成」正是这个源在这一轮里最值钱的那一栏，一眼看得见。
    #[expect(clippy::cast_precision_loss, reason = "十万量级，转 f64 精确")]
    let share = stats.with_summary as f64 * 100.0 / stats.subjects as f64;
    (
        SourceState::Ready {
            records: stats.subjects,
            fetched_at: Some(stats.built_at),
        },
        format!("{share:.1}% 带中文简介"),
    )
}

fn survey_switch(workspace: &Path) -> (SourceState, String) {
    let path = workspace::titledb_store_path(workspace);
    if !path.exists() {
        return (SourceState::Missing, String::new());
    }
    let store = match crate::titledb::store::Store::open(&path) {
        Ok(store) => store,
        Err(error) => return (broken(&error.to_string()), String::new()),
    };
    let stats = match store.stats() {
        Ok(stats) => stats,
        Err(error) => return (broken(&error.to_string()), String::new()),
    };
    if stats.ncas == 0 {
        return (SourceState::Missing, String::new());
    }
    (
        SourceState::Ready {
            records: stats.ncas,
            fetched_at: store.fetched_at().ok().flatten(),
        },
        format!("{} 个 TitleID", stats.titles),
    )
}

fn broken(why: &str) -> SourceState {
    SourceState::Broken {
        why: why.to_string(),
    }
}

/// 取一个数据源时**两次请求之间至少隔多久**。
///
/// 与命令行那三条子命令的默认值同一个数（`--throttle-ms`）。在线源赌的是账号与 IP
/// （ADR-0007），界面上那个按钮与命令行按的必须是同一条限流。
pub const THROTTLE: Duration = Duration::from_millis(500);

/// **取回一个数据源。联网。**
///
/// 这一层在核心库里而不在界面里，是因为「取哪一份、用哪套清单、限多快的流」是领域判断
/// （ADR-0005）。界面那个「重取」按钮与命令行的 `dat sync` / `zh sync` / `switch sync`
/// 走同一条路，于是两边取回来的东西一模一样。
///
/// 清单、平台清单与剥离规则**先看工作目录里有没有，没有才用内置的那一份**——
/// 与命令行的取法同一条（`ManifestArgs::load` 那几个）。
///
/// `task` 报进度，「停下」只接得动一段：`dat::sync::run` / `titledb::sync::sync`
/// 两个入口不收中断信号，`zh::sync::sync` 收（[`zh::sync::Context`]），但它接得住的
/// 只是**读那份原件**那几分钟——下载那 435 MB 照样停不掉。所以这一趟**下载开跑之后
/// 停不下来**：排着队还没轮到的那一趟停得掉（那一步在 [`Handle::step`] 上），
/// 正在下载的停不掉。剩下那几段记在挂单 `Q62` 上。这里如实说，不摆一个按了没反应的
/// 承诺出来。
///
/// # Errors
/// 取数、解析或写库失败时返回一句给人看的话。
pub fn refetch(source: Source, workspace: &Path, task: &Handle) -> Result<SourceStatus, String> {
    task.steps(1);
    task.step(&format!("取 {}", source.label()))
        .map_err(|halted| halted.to_string())?;
    let fetcher = HttpFetcher::with_throttle(THROTTLE);
    match source {
        Source::Dat => {
            let registry = load_registry(workspace)?;
            let mut repo = DatRepo::open(&workspace::dat_repo_path(workspace))
                .map_err(|error| format!("DAT 库打不开：{error}"))?;
            let options = crate::dat::sync::SyncOptions::new(workspace);
            crate::dat::sync::run(&fetcher, &mut repo, &registry, &options)
                .map_err(|error| format!("取 DAT 失败：{error}"))?;
        }
        Source::Chinese => {
            let manifest = manifest(workspace)?;
            let rules = rules(workspace)?;
            let mut store = zh::store::Store::open(&workspace::zh_store_path(workspace))
                .map_err(|error| format!("中文离线源打不开：{error}"))?;
            let options = zh::sync::Options {
                cache: workspace::zh_cache_dir(workspace),
                full: false,
                dry_run: false,
            };
            zh::sync::sync(
                &fetcher,
                &RealFs::new(),
                &mut store,
                &manifest,
                &rules,
                &options,
                &mut zh::sync::Context {
                    cancel: Some(task.cancel()),
                    progress: Some(&mut |at: zh::sync::Progress| task.tick(at.bytes, at.total)),
                },
            )
            // **按停下不折成一句「失败」。** 任务台按「那句话正是 `Halted` 交出来的那一句」
            // 把「停了」与「失败」分开记（`task::Board::settle`）——前头加一句
            // 「取中文离线源失败：」，界面上那一趟就成了失败，用户会以为自己按坏了什么。
            .map_err(|error| match error {
                zh::sync::SyncError::Halted(halted) => halted.to_string(),
                error => format!("取中文离线源失败：{error}"),
            })?;
        }
        Source::Switch => {
            let mut store =
                crate::titledb::store::Store::open(&workspace::titledb_store_path(workspace))
                    .map_err(|error| format!("Switch 数据库打不开：{error}"))?;
            let options = crate::titledb::sync::Options {
                cache: workspace::titledb_cache_dir(workspace),
                full: false,
                dry_run: false,
                regions: true,
            };
            crate::titledb::sync::sync(&fetcher, &mut store, &options)
                .map_err(|error| format!("取 Switch 数据库失败：{error}"))?;
        }
    }
    Ok(source.survey(workspace))
}

/// 数据源清单：工作目录里那份优先，没有才用内置的。
fn load_registry(workspace: &Path) -> Result<Registry, String> {
    let candidate = workspace.join("sources.toml");
    if !candidate.exists() {
        return Ok(Registry::builtin());
    }
    Registry::load(&candidate).map_err(|error| error.to_string())
}

/// **平台清单**：工作目录里那份优先，没有才用内置的。
///
/// 它是公开的，因为**这条查法不能有第二份**：命令行按 `--manifest`、没给就查工作目录，
/// 界面上没有那个开关、只查工作目录（`romcat_gui::scrape`）。两处各写一遍的话，
/// 用户改过的那份平台清单会在一条路上生效、另一条路上不生效——而它决定平台名怎么折，
/// 折出来的名字进的是变体那一行。
///
/// # Errors
/// 那份文件读不出来或者写坏了时返回一句给人看的话。
pub fn manifest(workspace: &Path) -> Result<Manifest, String> {
    let candidate = workspace.join("platforms.toml");
    if !candidate.exists() {
        return Ok(Manifest::builtin());
    }
    Manifest::load(&candidate).map_err(|error| error.to_string())
}

/// **剥离规则**：同上，理由也同上。
///
/// 它尤其要紧：剥离规则决定文件名剥出来的**正题**长什么样，而正题正是拿去撞
/// [中文离线源](crate::scrape::zh)的那一串字。两条路各用一份规则，同一个变体在命令行
/// 与界面上会撞到不同的条目——而那是写进库里的结论，不是显示上的差别。
///
/// # Errors
/// 那份文件读不出来或者写坏了时返回一句给人看的话。
pub fn rules(workspace: &Path) -> Result<crate::filename::Rules, String> {
    let candidate = workspace.join("name-rules.toml");
    if !candidate.exists() {
        return Ok(crate::filename::Rules::builtin());
    }
    crate::filename::Rules::load(&candidate).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::temp_dir;

    #[test]
    fn 一个源都没取过时三行都明说还没取回() {
        let workspace = temp_dir("数据源");
        let rows = survey(workspace.path());
        assert_eq!(rows.len(), 3);
        for row in &rows {
            assert_eq!(row.state, SourceState::Missing, "{} 该是还没取回", row.name);
            assert!(!row.ready());
            assert_eq!(row.records(), 0);
            assert!(!row.cost.is_empty(), "{} 得说清没取回的代价", row.name);
        }
    }
}
