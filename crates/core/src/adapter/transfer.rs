//! **导入与导出这两趟**：读盘、存快照、落库、写盘、检测外部改动。
//!
//! ## 导入
//!
//! 三件事，顺序不能反：
//!
//! 1. **先跑一趟往返实测**（[`assert_capability`]）。档位是断言不是声明，而断言要在
//!    **维护者手上这份真文件**上做——测试里编的样本过了不算数。
//! 2. **存快照**。原文逐字节进中立库（ADR-0003 修订段）。这是往返的全部依据，
//!    也是导出时还原「中立模型表达不了的部分」的基线。
//! 3. **落库**。手工维护的值进 `scrape_value`，源写作适配器的名字。它在
//!    `priorities.toml` 里排在 `裁决` 之后、全部数据源之前——**人手写的东西不该被
//!    DAT 覆盖**，那正是这张票的首要交付。
//!
//! 落不了库的条目（`file:` 指向的东西库里没有）**不是失败**：那一段的原文一字不差地
//! 留在快照里，只是暂时挂不到变体上。报告点名，不吞掉。
//!
//! ## 导出
//!
//! **不是覆盖写。** ADR-0001 的修订段把字段级三方合并降级成了改动检测，但「检测到
//! 外部改动就停下来」这一条没降：
//!
//! - 落点上有文件、而且与我们上次对齐的样子**不一致** → 有人在外面动过，**不写**。
//! - 落点上有文件、但我们**从没见过它** → 更要停——那可能就是维护者的原件。
//! - 两种都可以用 `--force` 压过去，但那是用户明说的，不是默认。
//!
//! ## 基线怎么找
//!
//! 落点目录里**导入过的快照**，若它里面的合集正是这一个，那份快照就是基线，
//! 落点也改用它原来的文件名。于是「把自己的 `metadata.pegasus.txt` 导进来，
//! 再导出到同一个目录」就是一次真正的往返：库里有的字段按库里的写，
//! 库里没有的（注释、未知键、排版）一字不动。
//!
//! 路径基准因此必须一致：`file:` 是**相对元数据文件所在目录**解析的，而中立库的键是
//! 相对主库根的（ADR-0020）。所以导出目录在语义上就是主库根的替身——把导出来的这几份
//! 文件放到主库根下，路径直接就对。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::catalog::scrape::{Harvested, HarvestedValue};
use crate::catalog::{Catalog, CatalogError, Roots, SnapshotOrigin};
use crate::path;
use crate::report::thousands;
use crate::scrape::priority::Priorities;
use crate::scrape::{AnchorKind, Field};
use crate::task::{Cutoff, Halted, Handle};

use super::converge::{self, Converged, NotAnEntry, Preference, VARIANT_KEY};
use super::report::{Conflict, EXAMPLES, ExportReport, ExportedFile, ImportReport, ImportedFile};
use super::{Adapter, AdapterError, Body, Document, Entry, Lossy, Parsed, assert_capability};

/// 导入或导出跑不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    /// 文件读不出来 / 写不进去。
    #[error("{path} {what}：{source}")]
    Io {
        /// 哪份文件。
        path: String,
        /// 读还是写。
        what: &'static str,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// 适配器读写失败。
    #[error(transparent)]
    Adapter(#[from] AdapterError),
}

/// 把一批前端元数据文件导进中立库。
///
/// # Errors
/// 读文件或读写中立库失败时返回错误。**某一份文件不是这个格式不算失败**——
/// 那一份跳过、报告点名，整趟接着跑。
pub fn import(
    catalog: &mut Catalog,
    adapter: &dyn Adapter,
    files: &[PathBuf],
    root: Option<&Path>,
) -> Result<ImportReport, TransferError> {
    // 主库那一组根**只用来把 `file:` 折成变体的键，不写回中立库**。写回去是有代价的：
    // 用户给错一次 `--root`，下一趟扫描就会撞上「这个根名底下换了另一块盘」那道守卫
    // （`scan::guard_same_root`）。导入这一趟没有任何理由去动那一行。
    let mut roots = Roots::load(catalog)?;
    if let Some(given) = root {
        // `--root` 只在**这份库只有一个根**的时候换得动位置：一组根里哪个是准的，
        // 一条路径说不出来。多于一个根时它被忽略——库里记着的位置本来就更可信。
        if let Some(only) = roots.only().map(str::to_string) {
            roots.set(&only, normalize(given));
        }
    }
    // 一次读齐。逐个条目查一遍等于把 9,226 行的作品表读上几千遍。
    let works = catalog.work_names()?;
    let mut report = ImportReport {
        catalog: catalog.location().to_string(),
        format: adapter.name().to_string(),
        ceiling: adapter.ceiling().label().to_string(),
        tier: adapter.ceiling().label().to_string(),
        // **不看这一趟导了什么。** 结构性损失是格式自己的边界，一条都没撞上也照样成立
        // ——照着文件里的内容去数，说出来的就成了「这一趟丢了 N 条」，那是另一件事。
        structural_losses: adapter.structural_losses().to_vec(),
        ..ImportReport::default()
    };
    let mut worst = adapter.ceiling();

    for file in files {
        let bytes = std::fs::read(file).map_err(|source| TransferError::Io {
            path: path::display(file),
            what: "读不出来",
            source,
        })?;
        let assertion = assert_capability(adapter, &bytes)?;
        let parsed = adapter.read(&bytes)?;
        worst = worst.min(assertion.asserted);

        let absolute = normalize(file);
        let stored = path::nfc(&path::display(&absolute)).into_owned();
        let fingerprint = crate::catalog::frontend::hash_of(&bytes);
        catalog.put_snapshot(adapter.name(), &stored, &bytes, SnapshotOrigin::Imported)?;

        let mut account = ImportedFile {
            path: stored.clone(),
            bytes: bytes.len() as u64,
            lines: parsed.preserved.lines,
            comments: parsed.preserved.comments,
            unknown_keys: parsed.preserved.unknown_keys,
            extension_keys: parsed.preserved.extension_keys,
            user_state: parsed.preserved.user_state,
            roundtrip: assertion.identical,
            tier: assertion.asserted.label().to_string(),
            difference: assertion.difference.as_ref().map(|difference| {
                format!(
                    "第 {} 行写回去对不上：原文 `{}`，写出来 `{}`",
                    difference.line, difference.expected, difference.found
                )
            }),
            lossy_total: parsed.lossy.len() as u64,
            lossy_by_kind: Lossy::all()
                .into_iter()
                .map(|kind| {
                    (
                        kind.label().to_string(),
                        parsed.lossy.iter().filter(|note| note.kind == kind).count() as u64,
                    )
                })
                .collect(),
            // **由重到轻各取几条。** 一份真库导出来的元数据里「补足」那一档能有上千条
            // （每个只知道年份的 `release` 都算一条），按行号取前十条的话，
            // 「整条丢弃」那几条——真正需要人看一眼的——会被它们整个挤掉。
            lossy: Lossy::all()
                .into_iter()
                .flat_map(|kind| {
                    parsed
                        .lossy
                        .iter()
                        .filter(move |note| note.kind == kind)
                        .take(EXAMPLES / 3 + 1)
                })
                .take(EXAMPLES)
                .map(|note| {
                    format!(
                        "第 {} 行 `{}: {}`——{}",
                        note.line, note.key, note.value, note.detail
                    )
                })
                .collect(),
            ..ImportedFile::default()
        };
        // **条目里那条相对路径以哪儿为基准，归适配器答**（[`Adapter::rom_bases`]）：
        // Pegasus 的 `file:` 是相对元数据文件所在目录的，ES gamelist 躺在
        // `gamelists/<平台目录>/` 下而 ROM 在 `<主库根>/<平台目录>/` 下。
        let mut batch = Vec::new();
        for entry in &parsed.doc.entries {
            match &entry.body {
                Body::Collection(_) => account.collections += 1,
                Body::Game(game) => {
                    account.games += 1;
                    let variant = resolve(catalog, &works, &roots, adapter, &absolute, game)?;
                    match variant {
                        Some((key, work)) => {
                            account.resolved += 1;
                            batch.extend(landed(
                                adapter.name(),
                                &fingerprint,
                                &key,
                                work.as_deref(),
                                game,
                            ));
                        }
                        None => {
                            account.unresolved += 1;
                            if report.unresolved_examples.len() < EXAMPLES {
                                report.unresolved_examples.push(format!(
                                    "「{}」：{}",
                                    game.title,
                                    game.files.join("、")
                                ));
                            }
                        }
                    }
                }
            }
        }
        report.values += batch
            .iter()
            .map(|harvested: &Harvested| harvested.values.len() as u64)
            .sum::<u64>();
        catalog.put_scraped(&batch)?;
        report.files.push(account);
    }

    report.tier = worst.label().to_string();
    Ok(report)
}

/// 导出**接在任务台上**那一趟交不出报告的原因。
///
/// **单独一个枚举，不往 [`TransferError`] 上加一支**：那一条是**导入与导出共用**的，
/// 而导入根本没有把手、停不下来——给它加一个交不出来的支，等于让每个 `match` 它的
/// 地方都去处理一种不会发生的事（与 [`FoldTitlesError`](crate::title::FoldTitlesError)
/// 同一条理由）。
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// 导不出来：读写中立库、写文件或适配器写不出来。
    #[error(transparent)]
    Transfer(#[from] TransferError),
    /// **被按停了，而且一份文件都还没写。** 盘上与中立库都一个字节没动。
    ///
    /// **写过之后停下的那一趟不走这一支**：它交出的是一份报告加一句
    /// [`Handle::halfway`]，因为它**留下了东西**（见 [`export_task`]）。
    ///
    /// **这一句想怎么写就怎么写。** 任务台分「停了」与「失败」看的是 [`Cutoff`]
    /// 落在哪一支（底下那个 `From` 折的），不是这句话说了什么。
    #[error("导出按停了：还没开始往盘上写，一份元数据文件都没动。")]
    Halted(#[from] Halted),
}

impl From<CatalogError> for ExportError {
    fn from(error: CatalogError) -> Self {
        Self::Transfer(error.into())
    }
}

impl From<AdapterError> for ExportError {
    fn from(error: AdapterError) -> Self {
        Self::Transfer(error.into())
    }
}

impl From<ExportError> for Cutoff {
    /// **被按停不折成一句「失败」。**
    ///
    /// 折的是**支**不是话：`Halted` 那一支进 [`Cutoff::Halted`]（它一个字都不带），
    /// 别的照旧带着自己那句话进 [`Cutoff::Failed`]。界面上那一趟于是记成「停了」，
    /// 而不是让人去找哪儿坏了。
    fn from(error: ExportError) -> Self {
        match error {
            ExportError::Halted(_) => Self::Halted,
            error => Self::Failed(error.to_string()),
        }
    }
}

/// 一个条目的 `file:` 指到库里的哪个变体，以及它属于哪个作品。
///
/// **一个根一个根地试**：主库是一组根，同一份元数据文件里的 `file:` 可能指着任何一个。
/// 先命中的那个赢——根之间不许套在一起（`catalog::roots::add_root`），所以至多命中一个。
fn resolve(
    catalog: &Catalog,
    works: &BTreeMap<i64, String>,
    roots: &Roots,
    adapter: &dyn Adapter,
    absolute: &Path,
    game: &super::Game,
) -> Result<Option<(String, Option<String>)>, CatalogError> {
    for (name, root) in roots.iter() {
        // Pegasus 的 `file:` 是相对元数据文件所在目录的，ES gamelist 躺在
        // `gamelists/<平台目录>/` 下而 ROM 在 `<那个根>/<平台目录>/` 下。
        let bases = adapter.rom_bases(absolute, Some(root));
        if let Some(found) = resolve_in(catalog, works, name, root, &bases, game)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn resolve_in(
    catalog: &Catalog,
    works: &BTreeMap<i64, String>,
    root_name: &str,
    root: &Path,
    bases: &[PathBuf],
    game: &super::Game,
) -> Result<Option<(String, Option<String>)>, CatalogError> {
    for file in &game.files {
        for base in bases {
            let key = path::library_key(root_name, root, &normalize(&base.join(file)));
            // 先按变体自己的键找，找不到再看它是不是某个变体的**成员**——多碟条目里
            // 写的常常是其中一张碟，而那张碟只是变体的一个成员。
            let variant = match catalog.variant(&key)? {
                Some(variant) => Some(variant),
                None => match catalog.variant_of(&key)? {
                    Some((variant_key, _)) => catalog.variant(&variant_key)?,
                    None => None,
                },
            };
            let Some(variant) = variant else { continue };
            let work = variant.work_id.and_then(|id| works.get(&id).cloned());
            return Ok(Some((variant.key, work)));
        }
    }
    Ok(None)
}

/// 一个条目落进中立库的那几条值。
///
/// **标题挂在变体上**：票 15 的 [`title::fold`](crate::title::fold) 正是从变体级的
/// 标题值折出标题集合的，挂到作品上它看不见。其余字段挂在作品上——简介、年份、
/// 发行商跨平台跨地区都成立（`CONTEXT.md` 的「作品」词条）；作品还没认出来时
/// 一并挂在变体上，否则那几个值无处安放。
///
/// **开发商、发行商与类型是集合字段：有几条落几条。** 维护者用续行写了两家开发商
/// （Pegasus 官方文档里 `files:` 那种写法），只落头一家的话，导出那一趟会拿库里
/// 那一条去比他手里那两条，判成「变了」再把整段续行重写成一行——第二家当场蒸发，
/// 而 `scrape_value` 的主键里带着值本身，本来就装得下几条（`catalog::scrape` 的
/// 表注释）。一条都不许少，这正是「一次往返不蒸发心血」（ADR-0001）。
fn landed(
    source: &str,
    fingerprint: &str,
    variant_key: &str,
    work: Option<&str>,
    game: &super::Game,
) -> Vec<Harvested> {
    let evidence = "维护者手工维护的前端元数据".to_string();
    let mut variant_values = Vec::new();
    if !game.title.trim().is_empty() {
        variant_values.push(HarvestedValue {
            field: Field::Title.label().to_string(),
            value: game.title.clone(),
            evidence: evidence.clone(),
        });
    }
    let mut work_values = Vec::new();
    let mut push = |field: Field, value: Option<String>| {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            work_values.push(HarvestedValue {
                field: field.label().to_string(),
                value,
                evidence: evidence.clone(),
            });
        }
    };
    // 集合字段：**一个值一条**，与中文离线源那一侧（`scrape::zh::collect_facts`）同一个形状。
    for (field, values) in [
        (Field::Developer, &game.developers),
        (Field::Publisher, &game.publishers),
        (Field::Genre, &game.genres),
    ] {
        for value in values {
            push(field, Some(value.clone()));
        }
    }
    push(Field::Description, game.description.clone());
    push(
        Field::Year,
        game.release.map(|release| release.year.to_string()),
    );

    // **输入指纹**：这一份文件的内容哈希加这一段挂在哪个变体上。文件改一个字节它就变，
    // 于是下一趟重新落库；文件没变就整条跳过（`scrape_probe` 的用法）。
    //
    // **不拿 `format!("{game:?}")` 当指纹**：`Debug` 的输出不是稳定契约——换个编译器、
    // 给 `Game` 加个字段，全库的指纹一起变，28,529 条缓存当场全失效；而且它本身
    // 就是一整个结构体摊成的长字符串，逐条存进 `scrape_probe` 是白花的空间。
    let input = format!("{fingerprint}#{variant_key}");
    let mut out = Vec::new();
    match work {
        // 认出了作品：标题挂变体、其余挂作品，两个**不同的锚点**，各写各的。
        Some(work) => {
            if !variant_values.is_empty() {
                out.push(Harvested {
                    anchor: AnchorKind::Variant.label().to_string(),
                    subject: variant_key.to_string(),
                    source: source.to_string(),
                    input: input.clone(),
                    values: variant_values,
                    media: Vec::new(),
                });
            }
            if !work_values.is_empty() {
                out.push(Harvested {
                    anchor: AnchorKind::Work.label().to_string(),
                    subject: work.to_string(),
                    source: source.to_string(),
                    input,
                    values: work_values,
                    media: Vec::new(),
                });
            }
        }
        // **还没认出作品：两拨值挂在同一个锚点上，必须并成一条。**
        //
        // `put_scraped` 是按「锚点 + 主体 + 源」整组替换的：同一组写两次，第二次会把
        // 第一次删掉。分成两条推进去，标题（第一条）当场被简介开发商那一条抹掉——
        // 真库上 16.3% 的变体还没认出作品，那就是每六条手工维护的标题丢掉一条，
        // 而「无损导入维护者多年手工维护的成果」正是这条路的全部意义（ADR-0001）。
        None => {
            variant_values.extend(work_values);
            if !variant_values.is_empty() {
                out.push(Harvested {
                    anchor: AnchorKind::Variant.label().to_string(),
                    subject: variant_key.to_string(),
                    source: source.to_string(),
                    input,
                    values: variant_values,
                    media: Vec::new(),
                });
            }
        }
    }
    out
}

/// 导出这一趟怎么跑。
#[derive(Debug, Clone)]
pub struct ExportOptions {
    /// 写到哪个目录。
    pub out: PathBuf,
    /// 只排计划、不写盘。
    pub dry_run: bool,
    /// 外面有人动过也照写。**默认关**。
    pub force: bool,
}

/// 把中立库导出成前端元数据。**没人按停下的那条路。**
///
/// 它就是 [`export_task`] 配一个**没人拿着的把手**：[`Handle::new`] 建出来的取消位
/// 永远关着，也没有第二处够得着它，于是那一趟停不下来。**两条路只有一份实现**
/// ——分成两份的话，「跑完之后记下这一趟的时刻」这种事迟早只落在其中一条上。
///
/// # Errors
/// 读写中立库、写文件或适配器写不出来时返回错误。**这条路交不出
/// [`Halted`](ExportError::Halted) 那一支。**
pub fn export(
    catalog: &mut Catalog,
    adapter: &dyn Adapter,
    priorities: &Priorities,
    options: &ExportOptions,
) -> Result<ExportReport, ExportError> {
    export_task(catalog, adapter, priorities, options, &Handle::new())
}

/// 这一趟一共几步。**改了 [`export_task`] 里那几句 `task.step` 就得改这个数**，
/// 不然进度条会走过头。调用方自己还有装配步骤时，把它加进自己声明的总数里
/// （`romcat_gui::stages` 那一处「读优先级表」就是这么算的）。
pub const TASK_STEPS: u32 = 3;

/// 把中立库导出成前端元数据——**接在任务台上**的那一趟。
///
/// ## 导出**有**「停在半路」这一档
///
/// 这一条与[折标题](crate::title::run_task)正相反，而差别是真的：重折的写是
/// 「清掉再写回」，停在中间等于把整份集合丢掉，所以那一趟要么写完、要么一个字节
/// 都没写；**导出是一份文件一份文件地写**，而每一份写完就当场把
/// [**底本**](Catalog::put_snapshot)也存进中立库。停在第三份上，前两份**真的躺在盘上
/// 了**，而且下一趟拿它们当基线接着比——那正是[停在半路](Handle::halfway)那一档
/// 说的「没走完却留下了东西」。
///
/// 于是这一趟有两种停法，**按留下了什么分**：
///
/// - **一份都还没写就停下** → [`ExportError::Halted`]，任务台记「停了，什么都没留下」。
///   收敛整个库那一步是这一趟最长的一段（见 [`Stage::Export`](crate::stage::Stage::Export)
///   上的实测），按停多半落在这儿。
/// - **写过至少一份之后停下** → 报一句 [`Handle::halfway`] 并**照旧返回 `Ok`**
///   （`Handle::halfway` 的文档逐字写着这一条：报了这句就得返回 `Ok`，
///   不然那句话被整条丢掉，历史反过来说「什么都没留下」）。报告里逐份列着写了哪几份。
///
/// **只排计划那一趟（`dry_run`）没有半路可停**：它一个字节都不写盘，停在哪儿都是
/// 「什么都没留下」。
///
/// ## 时刻戳打在最后，而且只打给走完了的那一趟
///
/// [`Catalog::mark_exported`] 在这一趟的**末尾**、跳过了 `dry_run` 与被按停的那两种
/// 之后才落。库屏工序段上导出那一行说的正是它——那一支的「还差多少」算不出来
/// （`stage::Stage::Export` 写着为什么与实测代价），退回显示上次跑的时刻。
/// 只排了计划、或者只写了一半就说「上次跑是刚刚」，那一行就在骗人
/// （规格第 31 条「至少不骗我」）。
///
/// # Errors
/// 读写中立库、写文件或适配器写不出来时返回 [`ExportError::Transfer`]；
/// 一份文件都没写就被叫停时返回 [`ExportError::Halted`]，那时盘上与中立库都一个字节
/// 没动。
#[allow(clippy::too_many_lines)]
pub fn export_task(
    catalog: &mut Catalog,
    adapter: &dyn Adapter,
    priorities: &Priorities,
    options: &ExportOptions,
    task: &Handle,
) -> Result<ExportReport, ExportError> {
    task.step("把整个库收敛成条目")?;
    let converged = converge::run(catalog, priorities, adapter)?;
    let out_dir = normalize(&options.out);

    // 落点目录里对齐过的快照，两条路各认各的：
    //
    // 1. **按合集名认**——维护者自己那份 `metadata.pegasus.txt` 叫什么名字是他的事，
    //    我们生成的叫 `FC.metadata.pegasus.txt`。认出来之后落点也改用他原来的名字，
    //    于是「把自己的文件导进来、再导出到同一个目录」就是一次真正的往返。
    // 2. **按落点路径认**——ES gamelist 的文档里没有合集段（平台是由文件摆在哪个
    //    目录下说的），第一条路对它一句话都说不出来。而它的落点本来就是唯一的
    //    （`gamelists/<平台目录>/gamelist.xml`），路径本身就是身份。
    //
    // ⚠️ **路径从 `read_dir` 来，不从快照那一列来。** 快照的键是 NFC 形式，它是身份；
    // 而 macOS 上 NTFS 交出来的名字是 NFD（ADR-0020：1.99% 的路径两种形式不同）。
    // 拿键去 `fs::write` 会在旁边新建一个 NFC 名字的文件，维护者的原件还躺在原处
    // 一个字没改——从此两份各走各的，而且外部改动检测再也认不出那份原件。
    // ADR-0020 的「读盘用系统给的原始路径，入库与比较用 NFC 形式」在这里是硬约束。
    task.step("读上次写出去的底本")?;
    let stored_snapshots = catalog.snapshots(adapter.name())?;
    let mut baselines: BTreeMap<String, (PathBuf, Parsed)> = BTreeMap::new();
    let mut by_path: BTreeMap<String, (PathBuf, Parsed)> = BTreeMap::new();
    for row in &stored_snapshots {
        // **只认落在导出目录里的那些。** 别处的快照与这一趟无关，拿它当基线等于
        // 把文件写到 `--out` 之外去。
        let stored = PathBuf::from(&row.path);
        if !path::is_inside(&out_dir, &stored) {
            continue;
        }
        let Some(real) = real_path(&stored) else {
            continue;
        };
        let Ok(parsed) = adapter.read(&row.bytes) else {
            continue;
        };
        by_path.insert(row.path.clone(), (real.clone(), parsed.clone()));
        for entry in &parsed.doc.entries {
            if let Some(collection) = entry.collection() {
                baselines.insert(collection.name.clone(), (real, parsed));
                break;
            }
        }
    }

    let mut report = ExportReport {
        catalog: catalog.location().to_string(),
        format: adapter.name().to_string(),
        tier: adapter.ceiling().label().to_string(),
        // 与导入那一侧同一份、同一句（见 `report` 的模块文档）。
        structural_losses: adapter.structural_losses().to_vec(),
        out: path::display(&out_dir),
        dry_run: options.dry_run,
        ..ExportReport::default()
    };
    fill_counts(&mut report, &converged);
    let mut worst = adapter.ceiling();

    // **最后一个能停的地方在每一份文件之前。** 过了它这一份就写到底：一份文件写一半
    // 留在盘上，前端读到的是一份残缺的元数据，而外部改动检测下一趟会把它认成
    // 「有人在外面动过」。
    task.step("逐份写出去")?;
    let 共几份 = converged.files.len() as u64;
    let mut 写了几份 = 0_u64;
    for (走到第几份, file) in converged.files.iter().enumerate() {
        task.tick(走到第几份 as u64, 共几份);
        // 一份都还没写就停下的那一趟什么都没留下，交回 `Halted`（任务台记「停了」）；
        // 写过之后停下的那一趟**留下了东西**，走底下那句 `halfway` 并照旧返回 `Ok`。
        if task.check().is_err() {
            if 写了几份 == 0 {
                return Err(Halted.into());
            }
            report.interrupted = true;
            break;
        }
        let default = out_dir.join(&file.file_name);
        let (target, baseline) = match baselines.get(&file.collection) {
            Some((real, parsed)) => (real.clone(), Some(parsed)),
            None => {
                let key = path::nfc(&path::display(&default)).into_owned();
                match by_path.get(&key) {
                    Some((real, parsed)) => (real.clone(), Some(parsed)),
                    None => (default, None),
                }
            }
        };
        let stored = path::nfc(&path::display(&target)).into_owned();

        // **不静默覆盖**：盘上那份与上次对齐的样子对不上就停下来（ADR-0001 修订段）。
        //
        // `--force` 压得过这一停，但**压不过「说出来」**：照写了哪几份、丢掉的是什么，
        // 照样逐条列进报告。「不静默」说的是不许悄悄发生，不是不许发生。
        if let Some(why) = external_change(catalog, adapter.name(), &target, &stored)? {
            if options.force {
                report.forced.push(Conflict {
                    path: stored.clone(),
                    why,
                });
            } else {
                report.conflicts.push(Conflict { path: stored, why });
                continue;
            }
        }

        let doc = rebase(&file.doc, baseline);
        let bytes = adapter.write(&doc, baseline)?;
        // **自查一遍**：写出来的这份自己读回去还能逐字节写回去吗。写得出但读不回来的
        // 格式不配叫无损往返，而这一条不能靠祈祷。
        let roundtrip = assert_capability(adapter, &bytes)?;
        worst = worst.min(roundtrip.asserted);

        // 「基线里有几段这次一段都没认领」**归适配器答**：一个条目占几段是格式自己的事
        // （见 [`Adapter::kept_verbatim`]）。
        let kept = baseline.map_or(0, |baseline| adapter.kept_verbatim(&doc, baseline));

        if !options.dry_run {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|source| TransferError::Io {
                    path: path::display(parent),
                    what: "建不出目录",
                    source,
                })?;
            }
            std::fs::write(&target, &bytes).map_err(|source| TransferError::Io {
                path: stored.clone(),
                what: "写不进去",
                source,
            })?;
            catalog.put_snapshot(adapter.name(), &stored, &bytes, SnapshotOrigin::Exported)?;
            写了几份 += 1;
        }

        report.files.push(ExportedFile {
            path: stored,
            collection: file.collection.clone(),
            entries: file
                .doc
                .entries
                .iter()
                .filter(|entry| entry.game().is_some())
                .count() as u64,
            bytes: bytes.len() as u64,
            written: !options.dry_run,
            baseline: baseline.map(|_| "导入时存下的原文".to_string()),
            kept_verbatim: kept,
            roundtrip: roundtrip.identical,
        });
    }

    report.tier = worst.label().to_string();
    if report.interrupted {
        // **留下了什么由这一层说**：任务台不知道这一趟写没写过东西（`Handle::halfway`
        // 的文档）。这句话要说清「下一趟接着来」是什么意思——导出**有**接得上的东西：
        // 写过的那几份连底本一起进了中立库，下一趟以它们为基线，只有变过的才重写。
        task.halfway(format!(
            "按停时写出去 {} 份元数据文件（共 {} 份），底本一起进了中立库；\
             再按一次会接着把剩下的写完，已经写过的那几份原样对得上就不重写。",
            thousands(写了几份),
            thousands(共几份),
        ));
    } else if 写了几份 > 0 {
        // **记下这一趟导出的时刻**：库屏工序段上导出那一行说的正是它。
        //
        // **判据是「真往盘上写过东西」**，四档收场里三档因此都不打戳，而且是同一条理由
        // ——它们一个字节都没写：只排计划那一趟（`dry_run`）、被按停那一趟（上面那一支），
        // 以及**每一份都撞上外面有人动过、一份都没写成**的那一趟。最后这一档最容易漏：
        // 它走的是正常出口、报告也没有 `interrupted`，可盘上什么都没多。那时说
        // 「上次跑是刚刚」，工序段那一行就在骗人（规格第 31 条「至少不骗我」）。
        catalog.mark_exported()?;
    }
    Ok(report)
}

fn fill_counts(report: &mut ExportReport, converged: &Converged) {
    report.variants = converged.variants;
    report.entries = converged.entries;
    report.work_entries = converged.work_entries;
    report.loose_entries = converged.loose_entries;
    report.exported_variants = converged.exported_variants;
    report.converged_entries = converged.converged_entries;
    report.extra_content_members = converged.extra_content_members;
    report.excluded = NotAnEntry::all()
        .into_iter()
        .map(|why| {
            (
                why.label().to_string(),
                converged.excluded.get(why.label()).copied().unwrap_or(0),
            )
        })
        .collect();
    report.excluded_examples = converged
        .excluded_examples
        .iter()
        .map(|(why, key)| ((*why).to_string(), key.clone()))
        .collect();
    report.preferred = Preference::all()
        .into_iter()
        .map(|why| {
            (
                why.label().to_string(),
                converged.preferred.get(why.label()).copied().unwrap_or(0),
            )
        })
        .collect();
}

/// 落点上那份文件与我们上次对齐的样子还一致吗；一致（或者根本没有那份文件）是 `None`。
fn external_change(
    catalog: &Catalog,
    format: &str,
    target: &Path,
    stored: &str,
) -> Result<Option<String>, TransferError> {
    if !target.exists() {
        return Ok(None);
    }
    let disk = std::fs::read(target).map_err(|source| TransferError::Io {
        path: stored.to_string(),
        what: "读不出来",
        source,
    })?;
    let hash = crate::catalog::frontend::hash_of(&disk);
    // 问的是「盘上这串字节是我们**对齐过的某一份**吗」——导入的原件与导出的产物都算。
    if catalog.snapshot_matches(format, stored, &hash)? {
        return Ok(None);
    }
    match catalog.snapshot(format, stored)? {
        Some(snapshot) => Ok(Some(format!(
            "落点上那份是 {} 字节，与上次{}时存下的对不上——有人在工具外面改过它。\
             覆盖等于把那次手改静默吞掉。",
            disk.len(),
            snapshot.origin.label()
        ))),
        None => Ok(Some(format!(
            "落点上已经有一份 {} 字节的文件，而工具从没见过它——它可能就是维护者的原件。\
             先 `romcat import` 把它读进来（原文会一字不差地留下），再导出。",
            disk.len()
        ))),
    }
}

/// 把新生成的段对回基线里的段。
///
/// 三层判据，由硬到软，**整层走完再降一层**：
///
/// 1. **`x-romcat-variant` 相同**——那是我们自己导出时写进去的**首选变体的键**，
///    机器读得动，不会认错；
/// 2. **有一条 `file:` 相同**——路径是这几个格式里最稳的身份（调研 D.4 第 2 条：
///    主键用路径，不要用标题）。**带不带平台那一段都算**，见函数体里的注释；
/// 3. **标题一字不差**——最后的兜底。
///
/// **「整层走完再降一层」不是排版，是正确性。** 标题在这个库里大量重名：真机上
/// 一趟 `导出 → 导入 → 再导出` 里，光 FC 一个平台就有近千个条目的标题与别的条目撞名。
/// 若按「逐个候选依次试三条判据、谁先真算谁」，一个本该按变体键对上的段会先被某个
/// **同名的、而且已经被别人占掉的**段截胡，于是它对不上基线、被当成新段接在文件末尾
/// ——文件一趟比一趟长，而基线里那一段再也不会被更新。
///
/// 对不上的基线段**原样留着**（[`super::pegasus`] 的渲染负责），不删。
fn rebase(doc: &Document, baseline: Option<&Parsed>) -> Document {
    let Some(baseline) = baseline else {
        return doc.clone();
    };
    // 先把基线索引起来。逐个段扫一遍全表是 O(n²)：真机上 FC 一个平台 7,739 个段，
    // 那是六千万次比较，而且每次还要问一遍「这个段被占了没有」。
    let mut by_variant: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut by_file: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut by_title: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    let mut by_collection: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (nth, entry) in baseline.doc.entries.iter().enumerate() {
        match &entry.body {
            Body::Collection(collection) => {
                by_collection
                    .entry(collection.name.as_str())
                    .or_default()
                    .push(nth);
            }
            Body::Game(game) => {
                if let Some(values) = game.extra.get(VARIANT_KEY)
                    && let Some(key) = values.first()
                {
                    by_variant.entry(key.as_str()).or_default().push(nth);
                }
                for file in &game.files {
                    by_file.entry(file.as_str()).or_default().push(nth);
                }
                if !game.title.is_empty() {
                    by_title.entry(game.title.as_str()).or_default().push(nth);
                }
            }
        }
    }

    // 这份文档要剥的是哪一段。**同一个文件在两个格式里可能写成两种路径**：中立库的键
    // 带着**平台目录**那一段（`FC/魂斗罗.zip`），而 ES gamelist 的 `<path>` 是相对
    // 那个目录的（`魂斗罗.zip`）。对回基线时两种都试，否则同一个文件会被当成两条，
    // 基线那一段永远对不上、一趟比一趟长。
    //
    // **平台目录与合集名都要试**：真库 22 个平台里有 12 个两者对不上（`WII` 的目录叫
    // `Wii`），只试合集名的话，那 12 个平台这条判据一次都不命中——于是 `origin` 退到
    // 「标题一字不差」那一档，而标题在这个库里大量重名（见上面那段注释）。
    let prefixes: Vec<String> = doc
        .entries
        .iter()
        .find_map(|entry| entry.collection())
        .map(|collection| {
            let mut out = vec![format!("{}/", collection.name)];
            if let Some(directory) = &collection.directory
                && directory != &collection.name
            {
                out.push(format!("{directory}/"));
            }
            out
        })
        .unwrap_or_default();

    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut entries = Vec::with_capacity(doc.entries.len());
    for entry in &doc.entries {
        let mut lanes: Vec<&Vec<usize>> = Vec::new();
        match &entry.body {
            Body::Collection(collection) => {
                lanes.extend(by_collection.get(collection.name.as_str()));
            }
            Body::Game(game) => {
                if let Some(key) = game
                    .extra
                    .get(VARIANT_KEY)
                    .and_then(|values| values.first())
                {
                    lanes.extend(by_variant.get(key.as_str()));
                }
                for file in &game.files {
                    lanes.extend(by_file.get(file.as_str()));
                    for prefix in &prefixes {
                        if let Some(rest) = file.strip_prefix(prefix.as_str()) {
                            lanes.extend(by_file.get(rest));
                        }
                    }
                }
                if !game.title.is_empty() {
                    lanes.extend(by_title.get(game.title.as_str()));
                }
            }
        }
        let origin = lanes
            .into_iter()
            .flatten()
            .copied()
            .find(|nth| !used.contains(nth));
        if let Some(nth) = origin {
            used.insert(nth);
        }
        entries.push(Entry {
            origin,
            body: entry.body.clone(),
        });
    }
    Document { entries }
}

/// 快照那一列记的是 NFC 形式的键，盘上那份的名字未必是同一种形式——把它找回来。
///
/// ⚠️ 这一步不能省（ADR-0020：真机上 1.99% 的路径两种形式不同）。拿 NFC 键直接去
/// `fs::write`，macOS 上会在维护者的原件**旁边**新建一个同名不同形式的文件，
/// 原件一个字没改地躺着——从此两份各走各的，外部改动检测再也认不出那一份。
///
/// 盘上没有这份文件时是 `None`：那说明这个落点还没有原件，这一趟从头生成。
fn real_path(stored: &Path) -> Option<PathBuf> {
    let parent = stored.parent()?;
    let want = path::nfc(&path::display(stored)).into_owned();
    std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| path::nfc(&path::display(path)) == want)
}

/// 把一个路径化成**可与中立库里那个主库根比较**的绝对形态。
///
/// 两道，缺一不可：
///
/// 1. **先把 `.` 与 `..` 就地约掉。** 不能直接交给 [`path::catalog_key`]：它按
///    [`Component::Normal`] 拼键，`..` 会被悄悄丢掉，于是 `FC/../GBA/x.zip` 折出来是
///    `FC/GBA/x.zip`——一个不存在的键。
/// 2. **再化开符号链接**（[`path::normalize_existing`]）。macOS 上 `/var` 是指向
///    `/private/var` 的链接，而扫描存进中立库的主库根是化开过的。不化开，
///    `strip_prefix` 当场对不上，`file:` 里每一条都会被报成「对不上库里的变体」。
fn normalize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    path::normalize_existing(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::Capability;

    #[test]
    fn 路径里的点点就地约掉() {
        // 这两级都不存在，`normalize_existing` 于是原样接回去——要证的正是 `..`
        // 在交给 `catalog_key` 之前就被约掉了，而不是被它悄悄丢掉。
        let out = normalize(Path::new("/没有这个目录/FC/../GBA/x.zip"));
        assert_eq!(out, PathBuf::from("/没有这个目录/GBA/x.zip"));
    }

    #[test]
    fn 平台目录与合集名对不上时也对得回基线() {
        // 真库 22 个平台里有 12 个两者对不上（`WII` 的目录叫 `Wii`）。基线那一侧写的是
        // 相对平台目录的路径，新文档这一侧是中立库的键——只试合集名的话，那 12 个平台
        // 「有一条 file 相同」这条判据一次都不命中，`origin` 退到「标题一字不差」，
        // 而标题在这个库里大量重名。
        let 基线 = Parsed {
            doc: Document {
                entries: vec![Entry {
                    origin: Some(0),
                    body: Body::Game(super::super::Game {
                        title: "甲".to_string(),
                        files: vec!["塞尔达.rar".to_string()],
                        ..super::super::Game::default()
                    }),
                }],
            },
            source: Vec::new(),
            preserved: super::super::Preserved::default(),
            lossy: Vec::new(),
        };
        let doc = Document {
            entries: vec![
                Entry::new(Body::Collection(super::super::Collection {
                    name: "WII".to_string(),
                    directory: Some("Wii".to_string()),
                    ..super::super::Collection::default()
                })),
                Entry::new(Body::Game(super::super::Game {
                    // 标题与基线那一段**不一样**：能对上就只可能是靠路径。
                    title: "塞尔达传说".to_string(),
                    files: vec!["Wii/塞尔达.rar".to_string()],
                    ..super::super::Game::default()
                })),
            ],
        };
        let rebased = rebase(&doc, Some(&基线));
        assert_eq!(rebased.entries[1].origin, Some(0), "该靠路径对回基线那一段");
    }

    #[test]
    fn 档位取全部文件里最低的那一档() {
        // `min` 靠的是 `Capability` 的序：只读 < 只写 < 双向 < 无损往返。
        assert_eq!(
            Capability::LosslessRoundTrip.min(Capability::Bidirectional),
            Capability::Bidirectional
        );
    }
}
