//! **识别**的第一命中层：**CRC-32 加未压缩大小**撞 DAT，产出带**置信度**与**依据**的
//! **候选**，并在中立库里立起**作品**与**发行版**。
//!
//! ## 为什么第一层是 CRC-32 而不是 SHA-1
//!
//! zip 与 7z 的元数据里**只有 CRC-32**，而且零解压可得（票 03）；DAT 的每条 `<rom>` 都带
//! `size` + `crc`。库里 **91.1% 的容量在透明容器里**，所以第一层用 CRC-32 加未压缩大小
//! 就能不解压地认出绝大部分内容（ADR-0002 的再修订）。SHA-1 留给后面几层。
//!
//! ## 两套哈希，两种成本
//!
//! **No-Intro 的 headerless 集按去头哈希，TOSEC 与 GoodNES 按含头原样**——只算一套就会
//! 丢掉一整边，含头那边丢掉的正是 TOSEC 全部的汉化条目。两套的成本却完全不同：
//!
//! - **裸文件**本来就要整份读一遍，那就一次读取把两套一起算出来（[`fingerprint`]）。
//! - **容器内部条目**的含头那套白拿（元数据里就有），去头那套要解压。于是先用**未压缩
//!   大小**筛一道：外挂头都是「固定长度的头加上整齐的内容」，尺寸对不上就一定没有头，
//!   两套落在同一串字节上——含头那次已经把去头那档 DAT 一并撞过了，不必解压。
//!   尺寸说得通的才解出来算第二套（[`header::size_suggests_header`]）。
//!
//! 于是**两套都撞**，而且撞出来的记录里有一部分是「去头口径的 DAT 被含头那套哈希撞上」
//! ——那些文件本来就没有外挂头，两套是同一串字节。实测数字在 `docs/library-facts.md`。
//!
//! ## NKit 前置于任何 CRC 匹配
//!
//! Dolphin 的原话：NKit 处理过的镜像**的 CRC32 可能和好转储的相同，即使两个文件并不
//! 完全一样**。所以 GC / Wii 的镜像在**接受**一条命中之前先验 `0x200` 处的 `NKIT`
//! （[`header::is_nkit`]），验出来就降一档置信度、不许自动通过。转回 ISO 再识别是票 09 的活。
//!
//! **验不了也不许自动通过。** 判据取自**撞上的那条记录说它是 GC / Wii 的光盘**，
//! 不是取自目录名——目录只是强先验（ADR-0011），放错地方的镜像照样存在。盘不在位、
//! 容器解不开、`--no-read-library`：这几种情形下验不出来，那条命中就只能是中置信。
//!
//! ## 四种结论，跳过与无判据都不混进未命中
//!
//! [`State`] 分四档：**命中 / 未命中 / 无判据 / 跳过**。把后两档并进未命中，命中率就
//! 失真了——「DAT 里没有这个东西」与「这东西根本不该撞 DAT」是两件事（[`scope`]），
//! 「拿不到判据」（容器穿不透、压缩镜像、目录树转储）又是第三件。

pub mod fingerprint;
pub mod header;
pub mod naming;
pub mod report;
pub mod scope;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::catalog::identify::{Candidate, Confidence, ContentHash, EntryFact, Identification};
use crate::catalog::{Catalog, CatalogError, Provenance, State, VariantRow};
use crate::classify::{self, Category};
use crate::container::{self, ContainerKind, Demand, ReadPlan};
use crate::dat::chinese::ChineseMark;
use crate::dat::{Convention, DatRepo, Hit, RepoError};
use crate::fs::LibraryFs;
use crate::path::{extension_lower, file_name_of_key};
use crate::report::thousands;
use crate::scan::CancelToken;
use crate::shape::Role;

use fingerprint::{Fingerprint, Headerless};
use header::DumpHeader;
use report::IdentifyReport;

/// 识别跑不下去的原因。
///
/// **读不动一个文件不在这里**——那是一条如实记录的结论（「无判据」），不是致命错误。
/// 能到这一层的只有中立库与 DAT 库本身读写不了。
#[derive(Debug, thiserror::Error)]
pub enum IdentifyError {
    /// 中立库读写失败。
    #[error(transparent)]
    Catalog(#[from] CatalogError),
    /// DAT 库读写失败。
    #[error(transparent)]
    Dat(#[from] RepoError),
}

/// 识别的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 主库根。只有需要回盘读字节时才用得上。
    pub root: PathBuf,
    /// 允许回盘读吗。关掉之后**一个字节都不读主库**：容器里零解压可得的 CRC-32
    /// 照撞，裸文件与去头那套则报「无判据」。
    pub read_library: bool,
    /// 单份内容读到多大就不读了；`None` 是不设上限。
    pub max_read_bytes: Option<u64>,
    /// 每算完多少个变体就把这一批结论写进中立库。
    pub write_batch: usize,
}

impl Options {
    /// 对着某个主库根的默认选项。
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            read_library: true,
            max_read_bytes: None,
            write_batch: 2_000,
        }
    }
}

/// 跑到哪儿了。识别可能要几十分钟，得说得出进度。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Progress {
    /// 算完了几个变体。
    pub done: u64,
    /// 一共几个。
    pub total: u64,
    /// 命中几个。
    pub matched: u64,
    /// 回盘读了多少字节。
    pub read_bytes: u64,
    /// 回盘读了几份内容。
    pub read_files: u64,
}

/// 一趟识别的产物。
#[derive(Debug, Clone)]
pub struct Outcome {
    /// 命中率报告。
    pub report: IdentifyReport,
    /// 这一趟是不是被中断了。
    pub interrupted: bool,
    /// 回盘读了多少字节。**第二趟应当接近 0**——算过的哈希留在中立库里（挂账 D14）。
    pub read_bytes: u64,
    /// 回盘读了几份内容。
    pub read_files: u64,
    /// 有几份内容的哈希是从中立库里直接取回来的，没有再读一遍盘。
    pub reused_hashes: u64,
}

/// 一份拿去撞 DAT 的内容：容器里的一个内部文件，或者一个裸文件。
///
/// 名字不叫 `Unit`：`dat::repo::Unit` 说的是「一次取数的最小单位」，同一个 crate 里
/// 两个 `Unit` 只会让人读错。
#[derive(Debug, Clone)]
struct ContentUnit {
    /// 是变体的哪个成员。
    member: String,
    /// 容器内部路径；裸文件是空串。
    inner: String,
    /// 判外挂头与 NKit 用的名字。
    name: String,
    /// 含头（原样）的大小。容器里零解压可得；裸文件从中立库的条目表取。
    size: u64,
    /// 判据；`None` 表示还没有。
    print: Option<Fingerprint>,
    /// 拿不到判据的理由。
    blocked: Option<String>,
    /// 撞上的记录，以及撞上时用的是哪套哈希。
    hits: Vec<(Hit, Convention)>,
    /// 这份内容在容器里，还是躺在盘上。
    in_container: bool,
}

impl ContentUnit {
    fn is_nkit(&self) -> bool {
        self.print.and_then(|print| print.nkit).unwrap_or(false)
    }
}

/// 跑一趟识别。
///
/// **不联网、不写任何 ROM 文件**：这一层只读中立库、DAT 库，以及（要算裸文件的哈希时）
/// 主库的字节——主库的接触全走 [`LibraryFs`] 这个只读接缝（ADR-0004）。
///
/// # Errors
/// 中立库或 DAT 库读写失败时返回错误。读不动某个文件不算错误，那是一条「无判据」。
pub fn run(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    repo: &DatRepo,
    options: &Options,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Outcome, IdentifyError> {
    // 先把识别自己上一轮造的东西清干净：候选、结论，以及它建出来的作品与发行版。
    // **裁决定下来的一行都不碰**——清完再读变体，于是「作品有、发行版没有」这条
    // 判据看到的只会是人说的话（见 `scope::decide`）。
    catalog.clear_identifications()?;

    let variants = catalog.variants()?;
    let mut state = Run {
        progress: Progress {
            total: u64::try_from(variants.len()).unwrap_or(u64::MAX),
            ..Progress::default()
        },
        // DAT 库里有哪几个平台。**没有弹药的平台不值得为它读盘**——真机上
        // `switch/` 那 91 个变体是 154 GiB 的 `.xci`，而 DAT 库里 Switch 一条记录都没有
        // （那是票 27 的活），读完它们只是把 154 GiB 换成一堆撞不上的数。
        ammo: repo.platforms()?,
        ..Run::default()
    };
    let mut batch: Vec<Identification> = Vec::new();
    let mut interrupted = false;

    for variant in &variants {
        if cancel.is_cancelled() {
            interrupted = true;
            break;
        }
        let record = identify_variant(library, catalog, repo, options, variant, &mut state)?;
        if record.state == State::Matched {
            state.progress.matched += 1;
        }
        batch.push(record);
        state.progress.done += 1;
        if batch.len() >= options.write_batch.max(1) {
            flush(catalog, &mut batch, &mut state)?;
            progress(&state.progress);
        }
    }
    flush(catalog, &mut batch, &mut state)?;
    progress(&state.progress);

    Ok(Outcome {
        report: IdentifyReport::build(catalog, repo)?,
        interrupted,
        read_bytes: state.progress.read_bytes,
        read_files: state.progress.read_files,
        reused_hashes: state.reused,
    })
}

/// 一趟识别路上攒着的东西。
#[derive(Default)]
struct Run {
    progress: Progress,
    /// DAT 库覆盖到的平台。
    ammo: BTreeSet<String>,
    /// 这一轮算出来的哈希，攒够一批写一次。
    hashes: Vec<ContentHash>,
    /// 作品名 → id。同一部作品的几个发行版共用一行。
    works: BTreeMap<String, i64>,
    /// 「源 + DAT + 条目名」→ id。同一条 DAT 条目在几个变体上共用一行发行版。
    releases: BTreeMap<String, i64>,
    /// 从中立库直接取回来、没再读一遍盘的哈希数。
    reused: u64,
}

/// DAT 库里有没有这个平台的记录。平台认不出来时当作**有**——那时无从判断，
/// 而少读一次的代价是一条永远认不出来的变体。
fn has_ammo(variant: &VariantRow, state: &Run) -> bool {
    variant
        .platform
        .as_deref()
        .is_none_or(|platform| state.ammo.contains(platform))
}

fn flush(
    catalog: &mut Catalog,
    batch: &mut Vec<Identification>,
    state: &mut Run,
) -> Result<(), IdentifyError> {
    catalog.put_content_hashes(&state.hashes)?;
    state.hashes.clear();
    catalog.write_identifications(batch)?;
    batch.clear();
    Ok(())
}

fn identify_variant(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    repo: &DatRepo,
    options: &Options,
    variant: &VariantRow,
    state: &mut Run,
) -> Result<Identification, IdentifyError> {
    let members = catalog.variant_members(&variant.key)?;
    let (mut units, contents) = collect(catalog, &members)?;

    if let Some(skip) = scope::decide(variant, &contents) {
        return Ok(Identification {
            variant_key: variant.key.clone(),
            state: State::Skipped,
            reason: Some(format!("{}：{}", skip.kind(), skip.detail())),
            units: 0,
            nkit: 0,
            read_bytes: 0,
            work_id: None,
            release_id: None,
            candidates: Vec::new(),
        });
    }

    // 一、该回盘的回盘：裸文件要整份读一遍才有判据；GC / Wii 的镜像在**接受**命中
    // 之前要先验 NKit（NKit **前置于任何 CRC 匹配**，所以读盘在撞库之前）；可能带
    // 外挂头的，解出来把去头那套也算上。容器里那套含头的 CRC-32 是零解压白拿的，
    // 这一步碰都不碰它们。
    //
    // 但**这个平台在 DAT 库里一条记录都没有**时，一个字节都不读：读出来的哈希
    // 无处可撞。容器里那套零解压的 CRC-32 照撞不误——它是白拿的，而且撞的是
    // 全库的记录，说不定这个 `switch/` 目录下躺着的其实是别的平台的东西。
    let read_bytes = if has_ammo(variant, state) {
        fill_in(library, catalog, options, variant, &mut units, state)?
    } else {
        let platform = variant.platform.as_deref().unwrap_or("这个");
        for unit in &mut units {
            if unit.print.is_none() && unit.blocked.is_none() {
                unit.blocked = Some(format!(
                    "DAT 库里 {platform} 平台一条记录都没有，读它算哈希也无处可撞"
                ));
            }
        }
        0
    };

    // 二、撞。含头那套一律撞一次——容器里的它零解压就有，裸文件的它刚算出来。
    for unit in &mut units {
        if let Some(print) = unit.print {
            unit.hits = lookup(repo, print.crc32, print.size, Convention::AsIs)?;
        }
        // 去头那套**照撞不误**，哪怕含头那次已经撞上了：一份带外挂头的卡带在 TOSEC 里
        // 按含头收、在 No-Intro 的 headerless 集里按去头收，两边各是一条候选。
        // 两套落在同一串字节上时不重复撞——那只会撞出两条一模一样的候选。
        if let Some(print) = unit.print
            && let Some((size, crc32)) = print.headerless_pair()
            && (size, crc32) != (print.size, print.crc32)
        {
            let mut bare = lookup(repo, crc32, size, Convention::Headerless)?;
            unit.hits.append(&mut bare);
        }
    }

    Ok(assemble(catalog, variant, &units, read_bytes, state)?)
}

/// 把一个变体拆成几份要撞 DAT 的内容，顺带收集它里面能看见的名字（给 [`scope`] 用）。
fn collect(
    catalog: &Catalog,
    members: &[(String, Role)],
) -> Result<(Vec<ContentUnit>, Vec<String>), CatalogError> {
    let mut units = Vec::new();
    let mut contents = Vec::new();
    for (key, role) in members {
        if ContainerKind::for_path(Path::new(key)).is_none() {
            contents.push(key.clone());
        }
        // **内部资源不产生候选**（`CONTEXT.md`）：目录树转储里的音频与封包数据没有
        // 独立身份。**附属内容**同理——它不能独立运行，DAT 里没有它。
        if !matches!(role, Role::Main | Role::Companion) {
            continue;
        }
        if ContainerKind::for_path(Path::new(key)).is_some() {
            let files = catalog.container_files(key)?;
            // **只收容器里装着什么，不收容器自己**：补丁的判据是「里面没有可运行的
            // 内容」，把容器自己算进去的话每个 zip 都自称可运行，那条判据就废了。
            for (inner, _, _) in &files {
                contents.push(inner.clone());
            }
            if files.is_empty() {
                let reason = match catalog.container_status(key)? {
                    Some(Some(detail)) => format!("容器穿不透：{detail}"),
                    Some(None) => "容器里没有内容".to_string(),
                    None => "这一趟扫描没有穿透容器".to_string(),
                };
                units.push(blocked_unit(key, "", reason, true));
                continue;
            }
            for (inner, size, crc32) in files {
                if !worth_matching(&inner) {
                    continue;
                }
                match crc32 {
                    Some(crc32) => units.push(ContentUnit {
                        member: key.clone(),
                        name: inner.clone(),
                        inner,
                        size,
                        print: Some(Fingerprint::as_is(size, crc32)),
                        blocked: None,
                        hits: Vec::new(),
                        in_container: true,
                    }),
                    // 7z 的 `kCRC` 是可选块。没有 CRC 的条目进不了第一命中层。
                    None => units.push(blocked_unit(
                        key,
                        &inner,
                        "容器没记这一条的 CRC-32".to_string(),
                        true,
                    )),
                }
            }
            continue;
        }
        let category = classify::classify(Path::new(file_name_of_key(key))).category;
        match category {
            // **压缩镜像不是包装，它就是变体本身的形态**（ADR-0014）。它的 CRC-32 是
            // 压缩之后那串字节的，DAT 记的是原始转储的——撞不上，也不该假装撞得上。
            // 认它要读内部的光盘序列号，那是票 09。
            Category::CompressedImage => units.push(blocked_unit(
                key,
                "",
                "压缩镜像：CRC-32 算在压缩后的字节上，撞不了 DAT（票 09 读光盘序列号）".to_string(),
                false,
            )),
            Category::BareFile | Category::Unclassified => match catalog.entry_fact(key)? {
                EntryFact::File(size) => units.push(ContentUnit {
                    member: key.clone(),
                    inner: String::new(),
                    name: key.clone(),
                    size,
                    print: None,
                    blocked: None,
                    hits: Vec::new(),
                    in_container: false,
                }),
                // 目录树转储：`param.sfo` 与内部序列号是票 09 的活。
                EntryFact::Dir => units.push(blocked_unit(
                    key,
                    "",
                    "目录树转储：这一层认不了（票 09 读 param.sfo 与光盘序列号）".to_string(),
                    false,
                )),
                EntryFact::Unreadable => units.push(blocked_unit(
                    key,
                    "",
                    "元数据读不到（ADR-0021 的第三态）".to_string(),
                    false,
                )),
                EntryFact::Missing | EntryFact::Other => {}
            },
            // `.rar`（票 04）与 `.zst`（票 26）在归类里也是**透明容器**，但穿透层
            // 故意还认不出它们。说清楚是「还穿不透」而不是「没有内容」——两句话
            // 指向完全不同的下一步。
            Category::TransparentContainer => units.push(blocked_unit(
                key,
                "",
                match extension_lower(Path::new(file_name_of_key(key))).as_deref() {
                    Some("rar") => "rar 容器这一层还穿不透（票 04）".to_string(),
                    Some("zst") => "zst 容器这一层还穿不透（票 26）".to_string(),
                    _ => "这个容器格式还穿不透".to_string(),
                },
                true,
            )),
            Category::MediaOrMetadata => {}
        }
    }
    Ok((units, contents))
}

fn blocked_unit(member: &str, inner: &str, reason: String, in_container: bool) -> ContentUnit {
    ContentUnit {
        member: member.to_string(),
        inner: inner.to_string(),
        name: if inner.is_empty() {
            member.to_string()
        } else {
            inner.to_string()
        },
        size: 0,
        print: None,
        blocked: Some(reason),
        hits: Vec::new(),
        in_container,
    }
}

/// 容器里这一条值不值得拿去撞。
///
/// 封面、说明书、金手指文本不是内容——它们撞不上任何 DAT，白白多几十万次查询。
/// 拿不准的（**未归类**）照撞：库里 2,297 个文件的扩展名连探针都没有，其中真有 ROM。
fn worth_matching(inner: &str) -> bool {
    let classification = classify::classify(Path::new(file_name_of_key(inner)));
    !matches!(classification.category, Category::MediaOrMetadata)
        && classification.suspect.is_none()
}

/// 该回盘的回盘。返回这个变体这一趟读了多少字节。
fn fill_in(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &Options,
    variant: &VariantRow,
    units: &mut [ContentUnit],
    state: &mut Run,
) -> Result<u64, IdentifyError> {
    // 先看中立库里有没有算过的。算过的哈希留着，第二趟就不必再读一遍盘（挂账 D14）。
    let mut cached: BTreeMap<String, BTreeMap<String, ContentHash>> = BTreeMap::new();
    for unit in units.iter() {
        if !cached.contains_key(&unit.member) {
            cached.insert(unit.member.clone(), catalog.content_hashes(&unit.member)?);
        }
    }
    for unit in units.iter_mut() {
        if let Some(row) = cached
            .get(&unit.member)
            .and_then(|rows| rows.get(&unit.inner))
        {
            unit.print = Some(restore(row));
            state.reused += 1;
        }
    }

    let mut read_bytes = 0;
    let platform = variant.platform.as_deref();
    // 按成员分组，一个容器最多开一次。
    let mut wanted: BTreeMap<String, Vec<(usize, Demand)>> = BTreeMap::new();
    let mut capped: Vec<(usize, String)> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if unit.blocked.is_some() {
            continue;
        }
        let Some(demand) = demand_of(unit, platform) else {
            continue;
        };
        // 上限只挡整份读，不挡那 0x204 字节的 NKit 探测——那一趟的代价与文件多大无关。
        if matches!(demand, Demand::All)
            && let Some(reason) = over_limit(unit, options)
        {
            capped.push((index, reason));
            continue;
        }
        wanted
            .entry(unit.member.clone())
            .or_default()
            .push((index, demand));
    }
    for (index, reason) in capped {
        if units[index].print.is_none() {
            units[index].blocked = Some(reason);
        }
    }
    if wanted.is_empty() {
        return Ok(0);
    }
    if !options.read_library {
        for (_, indexes) in wanted {
            for (index, _) in indexes {
                if units[index].print.is_none() {
                    units[index].blocked = Some(
                        "没读主库（--no-read-library）：裸文件的哈希只能从字节里来".to_string(),
                    );
                }
            }
        }
        return Ok(0);
    }

    for (member, indexes) in wanted {
        let path = library_path(&options.root, &member);
        if units[indexes[0].0].in_container {
            read_bytes += read_from_container(library, &path, units, &indexes, state);
        } else {
            let (index, demand) = indexes[0];
            read_bytes += read_bare(library, &path, &mut units[index], demand, state);
        }
    }

    // 算出来的存下来。
    for unit in units.iter() {
        if let Some(print) = unit.print
            && !cached
                .get(&unit.member)
                .and_then(|rows| rows.get(&unit.inner))
                .is_some_and(|row| restore(row) == print)
        {
            state.hashes.push(store(unit, print));
        }
    }
    Ok(read_bytes)
}

/// 这份内容要回盘读吗，读多少。
fn demand_of(unit: &ContentUnit, platform: Option<&str>) -> Option<Demand> {
    let Some(print) = unit.print else {
        // 一个字节都还没看过——裸文件就是这一档。
        return Some(Demand::All);
    };
    // NKit **前置于任何 CRC 匹配**：验不了就不许自动通过。读 0x204 字节，与文件多大无关。
    if print.nkit.is_none() && header::may_be_nkit(platform, &unit.name) {
        return Some(Demand::Prefix(fingerprint::PROBE_LEN as u64));
    }
    // 这个扩展名可能带外挂头，**而且尺寸也说得通**：解出来把去头那套也算上。
    //
    // 不看「含头那次撞上没有」——撞上了照样要算：一份带 iNES 头的卡带在 TOSEC 里按
    // 含头收、在 No-Intro 的 headerless 集里按去头收，**两边各是一条候选**，只算一套
    // 就少一条。尺寸排掉的那些是另一回事：那里根本不可能有外挂头，两套落在同一串
    // 字节上，含头那次已经把去头那档 DAT 一并撞过了。
    if print.headerless == Headerless::Unknown
        && header::may_have_header(&unit.name)
        && header::size_suggests_header(&unit.name, unit.size)
    {
        // 容器里那一条是整份读进内存的，因此还有一道体积闸：带外挂头的卡带最大也就
        // 几 MB，超过 `MAX_INLINE_READ` 的一定不是它。
        if unit.in_container && unit.size > MAX_INLINE_READ {
            return None;
        }
        return Some(Demand::All);
    }
    None
}

/// 体积超过上限，这一份就不读了。
///
/// 返回**为什么不读**而不是一个光秃秃的「没读」：报告要说得出「是这一层拿不到判据」
/// 与「是我按上限没去拿」的区别，后者是一条**策略**，说不清就成了默默压低未命中数。
fn over_limit(unit: &ContentUnit, options: &Options) -> Option<String> {
    let limit = options.max_read_bytes?;
    (unit.size > limit).then(|| {
        format!(
            "按 --max-read-mib 定的上限没读：这一份 {}，上限 {}",
            crate::report::human_bytes(unit.size),
            crate::report::human_bytes(limit)
        )
    })
}

/// 容器里的一条整份读进内存的上限。带外挂头的卡带最大的也就几 MB（SFC 的 6 MB 卡是
/// 上限那一档），64 MiB 是宽出一个数量级的保险。
const MAX_INLINE_READ: u64 = 64 << 20;

fn read_bare(
    library: &dyn LibraryFs,
    path: &Path,
    unit: &mut ContentUnit,
    demand: Demand,
    state: &mut Run,
) -> u64 {
    let mut handle = match library.open(path) {
        Ok(handle) => handle,
        Err(error) => {
            unit.blocked = Some(format!("读不动：{error}"));
            return 0;
        }
    };
    let print = match demand {
        Demand::Prefix(limit) => probe_prefix(unit, &mut handle, limit),
        _ => match fingerprint::of_reader(&unit.name, unit.size, &mut handle) {
            Ok(print) => print,
            Err(error) => {
                unit.blocked = Some(format!("读不动：{error}"));
                return 0;
            }
        },
    };
    unit.print = Some(print);
    state.progress.read_files += 1;
    let read = match demand {
        Demand::Prefix(limit) => limit.min(unit.size),
        _ => print.size,
    };
    state.progress.read_bytes += read;
    read
}

/// 只读前若干字节：这一趟只回答「是不是 NKit」，**绝不拿它当哈希**——前缀的 CRC-32
/// 不是这份内容的 CRC-32，混进去就是一条永远撞不上的判据。
fn probe_prefix(unit: &ContentUnit, reader: &mut dyn Read, limit: u64) -> Fingerprint {
    let mut head = vec![0u8; usize::try_from(limit).unwrap_or(fingerprint::PROBE_LEN)];
    let mut filled = 0;
    while filled < head.len() {
        match reader.read(&mut head[filled..]) {
            Ok(0) | Err(_) => break,
            Ok(read) => filled += read,
        }
    }
    head.truncate(filled);
    let mut print = unit
        .print
        .unwrap_or_else(|| Fingerprint::as_is(unit.size, 0));
    print.nkit = Some(header::is_nkit(&unit.name, &head));
    print
}

fn read_from_container(
    library: &dyn LibraryFs,
    path: &Path,
    units: &mut [ContentUnit],
    indexes: &[(usize, Demand)],
    state: &mut Run,
) -> u64 {
    let listing = match container::list(library, path) {
        Ok(listing) => listing,
        Err(error) => {
            for (index, _) in indexes {
                units[*index].blocked = Some(format!("容器读不动：{error}"));
            }
            return 0;
        }
    };
    let wanted: BTreeMap<&str, Demand> = indexes
        .iter()
        .map(|(index, demand)| (units[*index].inner.as_str(), *demand))
        .collect();
    let plan = ReadPlan::new(&listing, |entry| {
        wanted
            .get(entry.path.as_str())
            .copied()
            .unwrap_or(Demand::Skip)
    });
    let mut got: BTreeMap<String, (Vec<u8>, bool)> = BTreeMap::new();
    let outcome = container::read_entries(library, path, &listing, &plan, &mut |entry, reader| {
        let full = matches!(wanted.get(entry.path.as_str()), Some(Demand::All));
        let mut buffer = Vec::new();
        reader
            .take(entry.size.min(1 << 30))
            .read_to_end(&mut buffer)?;
        got.insert(entry.path.clone(), (buffer, full));
        Ok(())
    });
    if let Err(error) = outcome {
        for (index, _) in indexes {
            if units[*index].print.is_none() {
                units[*index].blocked = Some(format!("解不开：{error}"));
            }
        }
    }
    let mut read = 0;
    for (index, _) in indexes {
        let unit = &mut units[*index];
        let Some((bytes, full)) = got.get(&unit.inner) else {
            continue;
        };
        read += bytes.len() as u64;
        state.progress.read_files += 1;
        state.progress.read_bytes += bytes.len() as u64;
        if *full {
            unit.print = Some(Fingerprint::of_bytes(&unit.name, bytes));
        } else {
            unit.print = Some(probe_prefix(
                unit,
                &mut bytes.as_slice(),
                bytes.len() as u64,
            ));
        }
    }
    read
}

/// 主库里那个文件在哪。键是**相对主库根**的路径，分隔符是 `/`（ADR-0020）。
fn library_path(root: &Path, key: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for part in key.split('/') {
        path.push(part);
    }
    path
}

/// 撞一次，并记住**撞上时用的是哪套哈希**。参数顺序跟 [`DatRepo::lookup`] 一致，
/// 免得在调用处把 CRC-32 与大小写反——两个都是整数，写反了编译器不会说话。
fn lookup(
    repo: &DatRepo,
    crc32: u32,
    size: u64,
    hashed_as: Convention,
) -> Result<Vec<(Hit, Convention)>, RepoError> {
    repo.lookup(crc32, size)
        .map(|hits| hits.into_iter().map(|hit| (hit, hashed_as)).collect())
}

fn restore(row: &ContentHash) -> Fingerprint {
    let headerless = match (
        row.looked,
        row.header.as_deref(),
        row.bare_size,
        row.bare_crc32,
    ) {
        (false, ..) => Headerless::Unknown,
        (true, Some(label), Some(size), Some(crc32)) => Headerless::Present {
            header: [
                DumpHeader::INes,
                DumpHeader::Fds,
                DumpHeader::Lynx,
                DumpHeader::Atari7800,
                DumpHeader::SnesCopier,
            ]
            .into_iter()
            .find(|header| header.label() == label)
            .unwrap_or(DumpHeader::INes),
            size,
            crc32,
        },
        (true, ..) => Headerless::Absent,
    };
    Fingerprint {
        size: row.size,
        crc32: row.crc32,
        headerless,
        nkit: row.nkit,
    }
}

fn store(unit: &ContentUnit, print: Fingerprint) -> ContentHash {
    ContentHash {
        key: unit.member.clone(),
        inner: unit.inner.clone(),
        size: print.size,
        crc32: print.crc32,
        looked: print.headerless != Headerless::Unknown,
        header: print.header().map(|header| header.label().to_string()),
        bare_size: match print.headerless {
            Headerless::Present { size, .. } => Some(size),
            _ => None,
        },
        bare_crc32: match print.headerless {
            Headerless::Present { crc32, .. } => Some(crc32),
            _ => None,
        },
        nkit: print.nkit,
    }
}

/// 把撞出来的东西折成候选、结论，以及作品与发行版。
fn assemble(
    catalog: &mut Catalog,
    variant: &VariantRow,
    units: &[ContentUnit],
    read_bytes: u64,
    state: &mut Run,
) -> Result<Identification, CatalogError> {
    // 候选与它的**父条目名**成对走：`cloneof` 是 No-Intro 的 parent/clone 关系，
    // ADR-0010 拿它映射「同一部**作品**下的多个**发行版**」。排序会打乱顺序，
    // 所以不能靠下标去另一张表里找它。
    let mut scored: Vec<(Candidate, Option<String>)> = Vec::new();
    let mut nkit = 0;
    for unit in units {
        if unit.is_nkit() {
            nkit += 1;
        }
        for (hit, hashed_as) in &unit.hits {
            scored.push((candidate_of(unit, hit, *hashed_as), hit.cloneof.clone()));
        }
    }
    // 排序：先按候选自己的可信程度，再按数据源的先后，最后按名字定死顺序——
    // 同一份中立库跑两次，候选的次序必须一样。
    scored.sort_by(|a, b| {
        rank(variant, &a.0)
            .cmp(&rank(variant, &b.0))
            .then_with(|| a.0.game.cmp(&b.0.game))
            .then_with(|| a.0.dat.cmp(&b.0.dat))
    });

    let mut work_id = None;
    let mut release_id = None;
    if let Some(best) = scored.iter().position(|(c, _)| c.accepted) {
        let (candidate, cloneof) = &scored[best];
        let parsed = naming::parse(&candidate.game, cloneof.as_deref());
        let work = ensure_work(catalog, state, &parsed.work)?;
        work_id = Some(work);
        // **汉化版条目是变体不是发行版**（ADR-0012）：TOSEC 的 `[tr zh]` 条目说的是
        // 「有人把某个发行版汉化了」，它自己不是一次官方发行。认得出是哪部作品，
        // 认不出它基于哪一条发行版——那就只挂作品，发行版留空，等裁决补。
        if candidate.chinese != Some(ChineseMark::FanTranslated) {
            let id = ensure_release(catalog, state, work, candidate, &parsed)?;
            release_id = Some(id);
            scored[best].0.release_id = Some(id);
        }
    }
    let candidates: Vec<Candidate> = scored.into_iter().map(|(candidate, _)| candidate).collect();

    let blocked: Vec<&str> = units
        .iter()
        .filter_map(|unit| unit.blocked.as_deref())
        .collect();
    let usable = units.iter().filter(|unit| unit.print.is_some()).count();
    let state_of = if !candidates.is_empty() {
        State::Matched
    } else if usable > 0 {
        State::Unmatched
    } else {
        State::NoEvidence
    };
    let reason = match state_of {
        State::NoEvidence => Some(if blocked.is_empty() {
            "这个变体里没有可以撞 DAT 的内容".to_string()
        } else {
            blocked[0].to_string()
        }),
        _ => None,
    };

    Ok(Identification {
        variant_key: variant.key.clone(),
        state: state_of,
        reason,
        units: u64::try_from(usable).unwrap_or(u64::MAX),
        nkit,
        read_bytes,
        work_id,
        release_id,
        candidates,
    })
}

/// 候选之间怎么排。数字小的排前面。
///
/// 源的先后**不是** `sources.toml` 那份清单的抄件，而是一句关于**元数据质量**的判断：
/// No-Intro 与 Redump 的条目名、序列号与 parent/clone 关系最整齐，TOSEC 的名字里
/// 塞满了发行年份与小组名，MAME 是逐芯片的。清单里的顺序说的是「先取哪一个」，
/// 与「哪个的名字更可信」无关，两者没有理由绑在一起。认不出的源排最后。
fn rank(variant: &VariantRow, candidate: &Candidate) -> (u8, u8, u8) {
    let platform_matches =
        u8::from(variant.platform.as_deref() != Some(candidate.platform.as_str()));
    let source = match candidate.source.as_str() {
        "No-Intro" => 0,
        "Redump" => 1,
        "TOSEC" => 2,
        "MAME" => 3,
        "GoodNES" => 4,
        _ => 5,
    };
    (
        match candidate.confidence {
            Confidence::High => 0,
            Confidence::Medium => 1,
            Confidence::Low => 2,
        },
        platform_matches,
        source,
    )
}

fn candidate_of(unit: &ContentUnit, hit: &Hit, hashed_as: Convention) -> Candidate {
    let nkit = unit.is_nkit();
    // **NKit 没验过的 GC / Wii 镜像一律不许自动通过。**
    //
    // 「NKit 检测前置于任何 CRC 匹配」这条不能只靠目录去落实——目录只是强先验
    // （ADR-0011），放错地方的镜像照样存在。这里的判据换成**内容自己给的**：
    // 撞上的那条记录说它是 GC / Wii 的光盘，那这份内容就必须验过 NKit 才敢自动认账。
    // 验不了（盘不在位、容器解不开、`--no-read-library`）就降一档，等裁决。
    let unverified = unit.print.is_none_or(|print| print.nkit.is_none())
        && matches!(hit.platform.as_str(), "NGC" | "WII");
    // **逐芯片的命中不自动通过。** MAME 的 software list 把一张卡拆成 `prg` / `chr`
    // 若干 dataarea，一条 `rom` 是**一颗芯片**的内容。单芯片卡上它恰好等于去头哈希，
    // 多芯片卡上「对上了一颗芯片」离「这个文件就是那次发行」还差着别的芯片
    // （`dat::Convention::PerChip` 的文档说的就是这件事）。所以它降一档：
    // 通过但标记，等裁决（ADR-0002 的中置信那一档）。
    let per_chip = hit.convention == Convention::PerChip;
    let exact = hit.is_exact() && !nkit && !per_chip && !unverified;
    let mut evidence = format!(
        "{} 的《{}》里条目「{}」的文件「{}」，按{}哈希匹配 CRC-32 {:08X}",
        hit.source,
        hit.dat,
        hit.game,
        hit.rom,
        hashed_as.label(),
        unit.print.map_or(0, |print| match hashed_as {
            Convention::Headerless => print.headerless_pair().map_or(print.crc32, |pair| pair.1),
            _ => print.crc32,
        })
    );
    match hit.size {
        Some(size) => evidence.push_str(&format!(" 加大小 {}", thousands(size))),
        None => evidence.push_str("；这份 DAT 没记大小，只凭 CRC-32 撞上"),
    }
    // 剥了什么头只在**去头**那条候选上说——含头那条根本没剥。
    if hashed_as == Convention::Headerless
        && let Some(header) = unit.print.and_then(|print| print.header())
    {
        evidence.push_str(&format!("（剥掉了 {}）", header.label()));
    }
    if per_chip {
        evidence.push_str(
            "；这是**逐芯片**的记录（一条 `rom` 是一颗芯片，不是一个文件），\
             对上一颗芯片不等于对上整次发行，不自动通过",
        );
    }
    if nkit {
        evidence.push_str(
            "；**这份镜像是 NKit 处理过的**，Dolphin 明说它的 CRC32 可能与好转储相同而内容不同，\
             不许自动通过（票 09 转回 ISO 再识别）",
        );
    } else if unverified {
        evidence.push_str(
            "；这条记录说它是 GC / Wii 的光盘，而这份内容**没验过 NKit**——\
             NKit 处理过的镜像 CRC32 可能与好转储相同（Dolphin），验不了就不敢自动通过",
        );
    }
    if let Some(status) = &hit.status
        && matches!(status.as_str(), "baddump" | "nodump")
    {
        evidence.push_str(&format!("；DAT 说这条是 {status}"));
    }
    Candidate {
        member_key: unit.member.clone(),
        inner: unit.inner.clone(),
        confidence: if exact {
            Confidence::High
        } else {
            Confidence::Medium
        },
        accepted: exact,
        source: hit.source.clone(),
        dat: hit.dat.clone(),
        platform: hit.platform.clone(),
        game: hit.game.clone(),
        rom: hit.rom.clone(),
        hashed_as,
        dat_convention: hit.convention,
        evidence,
        chinese: hit.chinese,
        serial: hit.serial.clone(),
        release_id: None,
    }
}

fn ensure_work(catalog: &mut Catalog, state: &mut Run, name: &str) -> Result<i64, CatalogError> {
    if let Some(id) = state.works.get(name) {
        return Ok(*id);
    }
    let id = catalog.add_work(name, Provenance::Identified)?;
    state.works.insert(name.to_string(), id);
    Ok(id)
}

fn ensure_release(
    catalog: &mut Catalog,
    state: &mut Run,
    work: i64,
    candidate: &Candidate,
    parsed: &naming::Parsed,
) -> Result<i64, CatalogError> {
    // 一条 DAT 条目就是一条发行版。**数字世代不必特殊对待**（ADR-0019）：那里港服与
    // 美服共用同一个 TitleID，本来就是 DAT 里的同一条条目，于是自然只有一条发行版，
    // 中文落在 `languages` 那一列上。
    let key = format!("{}|{}|{}", candidate.source, candidate.dat, candidate.game);
    if let Some(id) = state.releases.get(&key) {
        return Ok(*id);
    }
    let id = catalog.add_release(
        work,
        Some(&candidate.platform),
        parsed.region.as_deref(),
        candidate.serial.as_deref(),
        parsed.languages.as_deref(),
        Provenance::Identified,
    )?;
    state.releases.insert(key, id);
    Ok(id)
}
