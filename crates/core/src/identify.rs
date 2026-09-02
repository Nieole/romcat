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
//! ## 第二命中层：光盘序列号（票 09）
//!
//! CRC-32 那一层对光盘世代结构性地不够用——**压缩镜像的 CRC-32 算在压缩后的字节上**、
//! **汉化版改过字节**、**目录树转储压根没有一个整文件可以算哈希**。而这三种东西
//! 都把身份**明文**写在最前面：PS1 / PS2 的 `SYSTEM.CNF`、PSP / PS3 / PSV 的
//! `PARAM.SFO`、GC / Wii 的光盘头。于是有了[光盘那一层](disc)与[序列号那一层](serial)：
//! **识别一个 8 GB 的 ISO 不需要读 8 GB。**
//!
//! 它排在 CRC 之后跑，不是因为不如它可信（票据定的是「序列号命中等同于精确哈希命中」），
//! 而是因为 CRC 那一层免费：撞上了就不必再为那份内容读几百 KB（`worth_probing`）。
//!
//! ## ⭐ NKit 前置于任何 CRC 匹配
//!
//! Dolphin 的原话：NKit 处理过的镜像**的 CRC32 可能和好转储的相同，即使两个文件并不
//! 完全一样**。所以 GC / Wii 的镜像在**接受**一条命中之前先验**光盘逻辑偏移** `0x200`
//! 处的 `NKIT`，验出来就降一档置信度、不许自动通过——序列号那一层也一样，一份 NKit
//! 镜像的序列号照样是对的，但它不是一份好转储，得先转回 ISO。
//!
//! **逻辑**偏移三个字是要害：`.nkit.gcz` 与 WBFS 里那一处不在文件的 `0x200` 上，
//! 得先按壳子打开（[`disc`]）。票 07 只按文件偏移验，压缩镜像一律漏网。
//!
//! **验不了也不许自动通过。** 判据取自**内容自己**——撞上的那条记录说它是 GC / Wii 的
//! 光盘，或者光盘头就摆在那儿——不是取自目录名，目录只是强先验（ADR-0011）。盘不在位、
//! 容器解不开、`--no-read-library`：这几种情形下验不出来，那条命中就只能是中置信。
//!
//! ## 四种结论，跳过与无判据都不混进未命中
//!
//! [`State`] 分四档：**命中 / 未命中 / 无判据 / 跳过**。把后两档并进未命中，命中率就
//! 失真了——「DAT 里没有这个东西」与「这东西根本不该撞 DAT」是两件事（[`scope`]），
//! 「拿不到判据」（容器穿不透、压缩镜像、目录树转储）又是第三件。

pub mod disc;
pub mod fingerprint;
pub mod header;
pub mod iso9660;
pub mod naming;
pub mod report;
pub mod scope;
pub mod serial;
pub mod sfo;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::catalog::identify::{
    Candidate, Confidence, ContentHash, DiscFactRow, EntryFact, Identification,
};
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
use crate::verdict::{self, Decision, Verdict};

use fingerprint::{Fingerprint, Headerless};
use header::DumpHeader;
use report::IdentifyReport;
use serial::Evidence;

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
    /// 有几个变体的结论直接来自**沉淀库**——**裁决过的东西不必再撞一遍 DAT**。
    pub from_verdicts: u64,
    /// 光盘那一层探了几份内容。
    pub probed: u64,
    /// 其中读出了内部标识的。
    pub with_id: u64,
    /// 光盘那一层这一趟没读到几份。**读不到不是结论，不落库**（ADR-0021）。
    pub missed: u64,
    /// **序列号层**产出的候选条数。
    pub serial_candidates: u64,
    /// **靠序列号层才认出来**的变体数——第一命中层在它们身上一条自动通过的候选都没有。
    pub serial_only: u64,
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
    /// 光盘那一层探出来的事实：壳子、NKit、内部标识（票 09）。
    disc: Option<disc::Facts>,
}

impl ContentUnit {
    /// 验过 NKit 没有，结论是什么。**`None` 是「没验过」，与 `Some(false)` 是两回事**
    /// ——混起来会让一份没验过的 GC 镜像自动通过（Dolphin：NKit 的 CRC32 可能与好转储相同）。
    ///
    /// 两个来源：光盘那一层按**逻辑**偏移 `0x200` 验的（压缩镜像也验得了），以及裸文件
    /// 整份读那一趟顺手验的。前者优先——后者只在字节恰好就是逻辑光盘时才对。
    fn nkit(&self) -> Option<bool> {
        self.disc
            .as_ref()
            .and_then(|facts| facts.nkit)
            .or_else(|| self.print.and_then(|print| print.nkit))
    }

    fn is_nkit(&self) -> bool {
        self.nkit() == Some(true)
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
    verdicts: &verdict::Index,
    options: &Options,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(&Progress),
) -> Result<Outcome, IdentifyError> {
    // 先把上一轮的结论清干净：候选、结论，以及作品与发行版里由它们造出来的行。
    // **裁决那些行也一起清**——它们是沉淀库的投影，下面照沉淀库重建一遍就回来了
    // （`Catalog::clear_identifications` 的文档说的就是这件事）。
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
        let record = identify_variant(
            library, catalog, repo, verdicts, options, variant, &mut state,
        )?;
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
        from_verdicts: state.from_verdicts,
        probed: state.probed,
        with_id: state.with_id,
        missed: state.missed,
        serial_candidates: state.serial_candidates,
        serial_only: state.serial_only,
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
    /// 把结论落成作品与发行版那几行的家伙。识别与**裁决**共用同一个。
    projector: Projector,
    /// 从中立库直接取回来、没再读一遍盘的哈希数。
    reused: u64,
    /// 结论直接来自沉淀库的变体数。
    from_verdicts: u64,
    /// 光盘那一层探了几份内容。
    probed: u64,
    /// 其中读出了标识的。
    with_id: u64,
    /// 这一趟没读到的份数（盘不在位、容器解不开、元数据读不到）。**它们不落库。**
    missed: u64,
    /// 序列号层产出的候选条数。
    serial_candidates: u64,
    /// 靠序列号层才认出来的变体数（第一命中层一条自动通过的候选都没有）。
    serial_only: u64,
    /// 这一轮探出来的光盘事实，攒够一批写一次。
    facts: Vec<DiscFactRow>,
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
    catalog.put_disc_facts(&state.facts)?;
    state.facts.clear();
    catalog.write_identifications(batch)?;
    batch.clear();
    Ok(())
}

fn identify_variant(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    repo: &DatRepo,
    verdicts: &verdict::Index,
    options: &Options,
    variant: &VariantRow,
    state: &mut Run,
) -> Result<Identification, IdentifyError> {
    let members = catalog.variant_members(&variant.key)?;
    let (mut units, visible) = collect(catalog, &members)?;
    // 算过的哈希先取回来。**这一步不读盘**，只是把中立库里存着的那套判据装回 units
    // （挂账 D14）。它排在撞库之前，是为了让沉淀库拿判据查得着——裁决过的东西
    // 一个字节都不必再读。
    let cached = restore_cached(catalog, &mut units, state)?;

    // 零、**沉淀库先说话**：裁决过的内容直接精确命中，不再进队列（ADR-0008）。
    let found = find_verdict(verdicts, variant, &units);
    let mut unknown = false;
    if let Some(found) = &found {
        state.from_verdicts += 1;
        match state.projector.project(
            catalog,
            variant,
            found.verdict,
            &found.member,
            &found.inner,
        )? {
            Some(record) => return Ok(record),
            // 「都不对而且认不出」不短路：它照常走完下面的流程，只在结论上盖一句
            // [`VERDICT_UNKNOWN_REASON`]，队列据此不再问它。
            None => unknown = true,
        }
    }

    if let Some(skip) = scope::decide(variant, &visible) {
        return Ok(Identification {
            variant_key: variant.key.clone(),
            state: State::Skipped,
            reason: Some(skip.recorded()),
            units: 0,
            nkit: 0,
            read_bytes: 0,
            work_id: None,
            release_id: None,
            candidates: Vec::new(),
        });
    }

    // 一、该回盘的回盘：裸文件要整份读一遍才有判据；可能带外挂头的，解出来把去头
    // 那套也算上。容器里那套含头的 CRC-32 是零解压白拿的，这一步碰都不碰它们。
    //
    // 但**这个平台在 DAT 库里一条记录都没有**时，一个字节都不读：读出来的哈希
    // 无处可撞。容器里那套零解压的 CRC-32 照撞不误——它是白拿的，而且撞的是
    // 全库的记录，说不定这个 `switch/` 目录下躺着的其实是别的平台的东西。
    let read_bytes = if has_ammo(variant, state) {
        fill_in(library, options, &mut units, &cached, state)?
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

    // 二、撞 CRC。含头那套一律撞一次——容器里的它零解压就有，裸文件的它刚算出来。
    // **它排在光盘那一层之前**，不是因为更可信，而是因为它免费：撞上了就不必再为
    // 那份内容读几百 KB 去找序列号（`worth_probing`）。
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

    // 三、⭐ **光盘那一层**：只读几百字节把内部标识取出来，顺便按**逻辑**偏移 0x200
    // 验 NKit。它在**接受**任何 CRC 命中之前跑完——NKit 处理过的镜像的 CRC32 可能与
    // 好转储相同（Dolphin），验完才敢说那句「命中」。
    let found = probe_discs(
        library, catalog, options, variant, &members, &mut units, state,
    )?;
    let read_bytes = read_bytes + found.read_bytes;

    // 四、拿标识撞 DAT 的序列号索引。
    let mut serial_candidates = serial::candidates(repo, &found.evidence)?;
    state.serial_candidates += u64::try_from(serial_candidates.len()).unwrap_or(0);

    let mut record = assemble(
        catalog,
        variant,
        &units,
        &found.evidence,
        &mut serial_candidates,
        read_bytes,
        state,
    )?;
    if unknown {
        record.reason = Some(VERDICT_UNKNOWN_REASON.to_string());
    }
    Ok(record)
}

/// 把一个变体拆成几份要撞 DAT 的内容，顺带收集它里面能看见的名字（给 [`scope`] 用）。
fn collect(
    catalog: &Catalog,
    members: &[(String, Role)],
) -> Result<(Vec<ContentUnit>, scope::Visible), CatalogError> {
    let mut units = Vec::new();
    let mut visible = scope::Visible::new();
    let contents = &mut visible.names;
    for (key, role) in members {
        if ContainerKind::for_path(Path::new(key)).is_none() {
            contents.push(key.clone());
        }
        // **内部资源不产生候选**（`CONTEXT.md`）：目录树转储里的音频与封包数据没有
        // 独立身份。**附属内容**同理——它不能独立运行，DAT 里没有它。
        if !matches!(role, Role::Main | Role::Companion) {
            continue;
        }
        if let Some(kind) = ContainerKind::for_path(Path::new(key)) {
            let files = catalog.container_files(key)?;
            // **只收容器里装着什么，不收容器自己**：补丁的判据是「里面没有可运行的
            // 内容」，把容器自己算进去的话每个 zip 都自称可运行，那条判据就废了。
            for (inner, _, _) in &files {
                contents.push(inner.clone());
            }
            if files.is_empty() {
                // **三种情况说三句不同的话**（`CONTEXT.md` 的「穿不透」条）：穿不透是
                // 试过了读不出来，空容器是读出来了里面没东西，还没读过是压根没试。
                // 三者的下一步完全不同——去看看这文件、不用管、再扫一趟。
                //
                // 对 [`scope::Visible::saw_inside`] 而言这三者**也不是一回事**：只有「读出来了、
                // 里面没东西」才算看进去过；另外两种交出来的空名单是「没看见」。
                let reason = match catalog.container_status(key)? {
                    Some(Some(detail)) => {
                        visible.saw_inside = false;
                        format!("容器穿不透：{detail}")
                    }
                    Some(None) => "容器里没有内容".to_string(),
                    None if kind.is_penetrable() => {
                        visible.saw_inside = false;
                        "这一趟扫描没有读容器（--no-containers）".to_string()
                    }
                    None => {
                        visible.saw_inside = false;
                        format!(
                            "{} 穿不透，而这一趟没为它付全量解压的代价（`romcat scan --zst`）",
                            kind.label()
                        )
                    }
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
                        disc: None,
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
            Category::CompressedImage => units.push(blocked_unit(
                key,
                "",
                "压缩镜像：CRC-32 算在压缩后的字节上，撞不了 DAT，内部标识也没读出来".to_string(),
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
                    disc: None,
                }),
                // 目录树转储：整个变体里没有一份「整文件」可以算哈希，锚是里面那份
                // `param.sfo`（[光盘那一层](disc)去读）。这句话是那一份也没读到时的落点。
                EntryFact::Dir => units.push(blocked_unit(
                    key,
                    "",
                    "目录树转储：没有可以算哈希的整份内容，也没读到 param.sfo".to_string(),
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
            // `.rar` 在归类里也是**透明容器**，但穿透层故意还认不出它（票 04）。
            // 说清楚是「还穿不透」而不是「没有内容」——两句话指向完全不同的下一步。
            Category::TransparentContainer => {
                visible.saw_inside = false;
                units.push(blocked_unit(
                    key,
                    "",
                    match extension_lower(Path::new(file_name_of_key(key))).as_deref() {
                        Some("rar") => "rar 容器这一层还穿不透（票 04）".to_string(),
                        _ => "这个容器格式还穿不透".to_string(),
                    },
                    true,
                ));
            }
            Category::MediaOrMetadata => {}
        }
    }
    Ok((units, visible))
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
        disc: None,
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

/// 把中立库里算过的哈希装回 units。**一个字节都不读盘。**
///
/// 抽出来单开一步，是因为它有**两个**消费者：回盘那一步（算过的不必再算，挂账 D14），
/// 以及**沉淀库**那一步（裁决按内容哈希钉，取不到判据就查不着）。塞在回盘里面的话，
/// 沉淀库就只能在读完盘之后才查得起来——而那正是它要省下的那笔钱。
fn restore_cached(
    catalog: &Catalog,
    units: &mut [ContentUnit],
    state: &mut Run,
) -> Result<BTreeMap<String, BTreeMap<String, ContentHash>>, CatalogError> {
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
    Ok(cached)
}

/// 该回盘的回盘。返回这个变体这一趟读了多少字节。
fn fill_in(
    library: &dyn LibraryFs,
    options: &Options,
    units: &mut [ContentUnit],
    cached: &BTreeMap<String, BTreeMap<String, ContentHash>>,
    state: &mut Run,
) -> Result<u64, IdentifyError> {
    let mut read_bytes = 0;
    // 按成员分组，一个容器最多开一次。
    let mut wanted: BTreeMap<String, Vec<(usize, Demand)>> = BTreeMap::new();
    let mut capped: Vec<(usize, String)> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if unit.blocked.is_some() {
            continue;
        }
        let Some(demand) = demand_of(unit) else {
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
///
/// **NKit 不在这里了**（票 09）：它挪进了[光盘那一层](disc)，判据从「文件偏移 0x200」
/// 改成「**逻辑**偏移 0x200」——那才是 Dolphin 说的那一处，压缩镜像也验得了。
fn demand_of(unit: &ContentUnit) -> Option<Demand> {
    let Some(print) = unit.print else {
        // 一个字节都还没看过——裸文件就是这一档。
        return Some(Demand::All);
    };
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

/// 为了够到 solid 块里排在后面的一条，最多肯先解开多少字节扔掉。
///
/// 这个数是**光盘那一层的成本闸**。它存在的理由是这一层的全部意义：只读几百字节。
/// 一份 7z 里的 `.iso` 排在几 GB 的东西后面时，为它付几分钟的解压等于把这条道理
/// 反过来做——而那一条真机上存在，第一次跑就撞上了。
///
/// 64 MiB 按 LZMA 约 50 MB/s 算是一秒多，摊在两千来个容器上是分钟量级；踢掉的那些
/// 报告里逐条点名，看得见、也补得回来（改大这个数重跑即可）。
const MAX_SOLID_DRAIN: u64 = 64 << 20;

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

/// 候选表里，**沉淀库**那条候选的「数据源」叫什么。
///
/// 它和 `No-Intro`、`TOSEC` 平级地出现在报告的数据源那一栏里——**裁决攒出来的
/// 覆盖率和 DAT 给的覆盖率要一眼看得出各占多少**，那正是这份数据存在的理由（ADR-0008）。
pub const VERDICT_SOURCE: &str = "沉淀库";

/// 候选表里，沉淀库那条候选的「哪一份 DAT」叫什么。
pub const VERDICT_DAT: &str = "本机裁决";

/// 「都不对，而且认不出」那一档裁决落进 `identification.reason` 的那句话。
///
/// **写与读必须共用它**（与 `scope::Skip::recorded` 同理）：队列靠这一句把「人看过了、
/// 认不出」与「还没人看过」分开，而那两件事混在一起就会让人被反复问同一个问题。
/// 它落在 `reason` 而不是另立一个状态，是因为**结论本身没有变**——它照旧是未命中或
/// 无判据，变的只是「为什么还停在这儿」，而那正是这一列装的东西。
pub const VERDICT_UNKNOWN_REASON: &str = "裁决：都不对，而且认不出是什么";

/// 一个变体拿得到的**内容判据**：第一命中层那一套（含头的 CRC-32 加大小）。
///
/// **裁决**拿它当锚：钉在内容上的裁决换台机器、改个名字照样认得出，钉在路径上的不行
/// （`verdict::Anchor`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentPrint {
    /// 是变体的哪个成员：容器的键，或者裸文件自己的键。
    pub member: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// 未压缩大小。
    pub size: u64,
    /// 含头（原样）的 CRC-32。
    pub crc32: u32,
}

/// 这个变体拿得到内容判据吗；拿不到就是 `None`。
///
/// **一个字节都不读主库**：判据要么零解压地躺在容器构成里（票 03），要么是上一趟识别
/// 算过之后存在中立库里的（挂账 D14）。取不到就如实说没有——**待确认队列**据此告诉
/// 用户「这一条的裁决只钉得住本机的路径」。
///
/// # Errors
/// 读中立库失败时返回错误。
pub fn content_print(
    catalog: &Catalog,
    variant: &VariantRow,
) -> Result<Option<ContentPrint>, CatalogError> {
    let members = catalog.variant_members(&variant.key)?;
    let (mut units, _visible) = collect(catalog, &members)?;
    let mut state = Run::default();
    restore_cached(catalog, &mut units, &mut state)?;
    Ok(representative(variant, &units).and_then(|unit| {
        unit.print.map(|print| ContentPrint {
            member: unit.member.clone(),
            inner: unit.inner.clone(),
            size: print.size,
            crc32: print.crc32,
        })
    }))
}

/// 一个变体拿哪一份内容代表自己。
///
/// **主文件那一份优先，同为主文件的取大的**。一个变体可以是好几份内容（`cue` 加几条
/// `bin`、一个包里装着 ROM 和说明书），拿说明书的哈希去当这个变体的锚，换台机器就再也
/// 对不上了。顺序还要**定死**：同一份库跑两次，锚必须是同一份。
fn representative<'a>(variant: &VariantRow, units: &'a [ContentUnit]) -> Option<&'a ContentUnit> {
    ordered(variant, units).first().map(|index| &units[*index])
}

/// 按「谁最能代表这个变体」把 units 排个序，返回下标。带不动判据的一律不进。
fn ordered(variant: &VariantRow, units: &[ContentUnit]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..units.len())
        .filter(|index| units[*index].print.is_some())
        .collect();
    order.sort_by(|a, b| {
        let (left, right) = (&units[*a], &units[*b]);
        u8::from(left.member != variant.main_key)
            .cmp(&u8::from(right.member != variant.main_key))
            .then_with(|| right.size.cmp(&left.size))
            .then_with(|| left.member.cmp(&right.member))
            .then_with(|| left.inner.cmp(&right.inner))
    });
    order
}

/// 沉淀库对这个变体说过话没有，说的是钉在哪一份内容上的。
struct Found<'a> {
    verdict: &'a Verdict,
    member: String,
    inner: String,
}

/// 查沉淀库。**内容锚优先于路径锚**：前者说的是「这串字节是什么」，后者只是本机的退路。
///
/// [`Decision::Unknown`] 也查得出来，但它**不短路**——「都不对，我也认不出」是一条
/// 记下来别再问第二遍的裁决，不是一个结论。把它变成「命中」或者「跳过」都会让命中率
/// 说谎，所以它照常走完下面的流程，只在结论上盖一句
/// [`VERDICT_UNKNOWN_REASON`]，队列据此不再问它。
fn find_verdict<'a>(
    verdicts: &'a verdict::Index,
    variant: &VariantRow,
    units: &[ContentUnit],
) -> Option<Found<'a>> {
    if verdicts.is_empty() {
        return None;
    }
    for index in ordered(variant, units) {
        let unit = &units[index];
        if let Some(print) = unit.print
            && let Some(found) = verdicts.by_content(print.crc32, print.size)
        {
            return Some(Found {
                verdict: found,
                member: unit.member.clone(),
                inner: unit.inner.clone(),
            });
        }
    }
    verdicts.by_path(&variant.key).map(|found| Found {
        verdict: found,
        member: variant.main_key.clone(),
        inner: String::new(),
    })
}

/// 要探的一份内容。
///
/// 六样东西成群结队地一起走，所以捏成一个类型而不是一个六元组——`member` / `inner` /
/// `name` 三个都是 `String`，元组里写错顺序编译器不会说话。
struct Wanted {
    /// 对应 [`ContentUnit`] 里的哪一条。**目录树转储里那份 `param.sfo` 是 `None`**：
    /// 它的角色是**内部资源**，压根不产生候选，也就不在 units 里。
    unit: Option<usize>,
    /// 是变体的哪个成员。
    member: String,
    /// 容器内部路径；裸文件是空串。
    inner: String,
    /// 判壳子用的名字。
    name: String,
    /// 这份内容多大。
    size: u64,
    /// 在容器里，还是躺在盘上。
    in_container: bool,
}

/// 探一份内容的结果。
///
/// 两档分开，是因为它们的**保质期**完全不同：
///
/// - [`Self::Read`] 是关于这份内容的**结论**，只要文件没变就一直成立 → 落进
///   `content_disc`，第二趟直接取回。
/// - [`Self::Missed`] 是「**这一趟**没读到」——盘不在位、容器解不开、
///   或者那 4,085 个在 macOS 上连 `stat` 都失败的文件（ADR-0021 的第三态）。
///   **它绝不落库**：缓存一次读失败等于让它永久生效，而那批文件在 Windows 上是正常的
///   （ADR-0018 定的工作方式正是两台机器轮流碰同一块盘）。
enum Probe {
    /// 读到了，这是结论。
    Read(disc::Facts),
    /// 这一趟没读到，理由在这儿。
    Missed(String),
}

/// 光盘那一层探完之后攒下来的东西。
struct Probed {
    /// 收集到的标识，按可信程度排好。
    evidence: Vec<Evidence>,
    /// 这一趟为它读了多少字节。
    read_bytes: u64,
}

/// ⭐ **只读几百字节就认出来**：把变体里每一份光盘形态的内容探一遍。
///
/// 它同时办两件事，而两件都必须发生在**接受**任何命中之前：
///
/// 1. **NKit**。判据是**光盘逻辑偏移** `0x200` 处的 `NKIT`（Dolphin
///    `VolumeDisc::IsNKit()`）。逻辑偏移这三个字是要害——`.nkit.gcz` 与 WBFS 里那一处
///    不在文件的 `0x200` 上，得先按壳子打开才读得到（[`disc`]）。
/// 2. **内部标识**：PS1 / PS2 的 `SYSTEM.CNF`、PSP / PS3 / PSV 的 `PARAM.SFO`、
///    GC / Wii 的光盘头。
///
/// **算过的不再算**：探出来的事实落在中立库的 `content_disc` 里，按文件的三元组作废，
/// 与算过的哈希是同一条路（挂账 D14）。第二趟识别在这一层上同样是零字节。
fn probe_discs(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &Options,
    variant: &VariantRow,
    members: &[(String, Role)],
    units: &mut [ContentUnit],
    state: &mut Run,
) -> Result<Probed, IdentifyError> {
    let mut probed = Probed {
        evidence: Vec::new(),
        read_bytes: 0,
    };
    // 一、要探哪几份。先是 units 里那些长得像光盘形态的；再是变体成员里那些
    // **裸的 `param.sfo`**——目录树转储里那一份的角色是**内部资源**，压根不在 units 里
    // （`CONTEXT.md`：内部资源不产生候选），可它正是 PSV 那 1,416 个变体的锚。
    let mut wanted: Vec<Wanted> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        // **`blocked` 不是跳过的理由**：压缩镜像与目录树转储在第一命中层就是带着
        // 「拿不到判据」这句话过来的，而它们正是这一层要救的东西。真正读不动的
        // （元数据读不到、rar 穿不透）自然过不了下面那道 `by_name`。
        if disc::by_name(&unit.name).is_none() || !worth_probing(unit) {
            continue;
        }
        wanted.push(Wanted {
            unit: Some(index),
            member: unit.member.clone(),
            inner: unit.inner.clone(),
            name: unit.name.clone(),
            size: unit.size,
            in_container: unit.in_container,
        });
    }
    for (key, _role) in members {
        if !file_name_of_key(key).eq_ignore_ascii_case("param.sfo") {
            continue;
        }
        if wanted
            .iter()
            .any(|it| it.member == *key && it.inner.is_empty())
        {
            continue;
        }
        let EntryFact::File(size) = catalog.entry_fact(key)? else {
            continue;
        };
        wanted.push(Wanted {
            unit: None,
            member: key.clone(),
            inner: String::new(),
            name: key.clone(),
            size,
            in_container: false,
        });
    }

    // 二、算过的先取回来（不读盘），剩下的才回盘。
    let mut cached: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for it in &wanted {
        if !cached.contains_key(&it.member) {
            cached.insert(it.member.clone(), catalog.disc_facts(&it.member)?);
        }
    }
    let mut todo: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut facts: Vec<Option<Probe>> = Vec::with_capacity(wanted.len());
    for (at, it) in wanted.iter().enumerate() {
        let stored = cached
            .get(&it.member)
            .and_then(|rows| rows.get(&it.inner))
            .and_then(|text| serde_json::from_str::<disc::Facts>(text).ok())
            .map(Probe::Read);
        if stored.is_none() && options.read_library {
            todo.entry(it.member.clone()).or_default().push(at);
        }
        facts.push(stored);
    }
    for (member, indexes) in todo {
        let path = library_path(&options.root, &member);
        let read = if wanted[indexes[0]].in_container {
            read_disc_from_container(library, &path, &wanted, &indexes, &mut facts)
        } else {
            indexes
                .iter()
                .map(|at| read_disc_bare(library, &path, &wanted[*at], &mut facts[*at]))
                .sum()
        };
        probed.read_bytes += read;
        // 这一层读的字节也进总账：**「第二趟读了多少」是这条增量兑现与否的唯一凭据**
        // （挂账 D14），少记一处就等于自己给自己发了张漂亮的成绩单。
        state.progress.read_bytes += read;
        state.progress.read_files += u64::try_from(indexes.len()).unwrap_or(0);
    }

    // 三、装回 units（NKit 靠它），落库，收集标识。
    for (at, it) in wanted.iter().enumerate() {
        let found = match facts[at].take() {
            None => continue,
            // 这一趟没读到：**不落库**（缓存一次读失败等于让它永久生效，ADR-0021），
            // 也**不覆盖 `blocked`**——第一命中层写在那儿的那句话（「元数据读不到」
            // 之类）才是这条该报的理由。只有它本来什么都没说时才补上这一句，
            // 否则报告里这条会一个字都没有。
            Some(Probe::Missed(why)) => {
                state.missed += 1;
                if let Some(index) = it.unit
                    && units[index].print.is_none()
                    && units[index].blocked.is_none()
                {
                    units[index].blocked = Some(why);
                }
                continue;
            }
            Some(Probe::Read(found)) => found,
        };
        state.probed += 1;
        if !found.ids.is_empty() {
            state.with_id += 1;
        }
        if cached
            .get(&it.member)
            .and_then(|rows| rows.get(&it.inner))
            .is_none()
            && let Ok(text) = serde_json::to_string(&found)
        {
            state.facts.push(DiscFactRow {
                key: it.member.clone(),
                inner: it.inner.clone(),
                facts: text,
            });
        }
        // **GC / Wii 的光盘才在乎 NKit**，而判据取自内容自己（读出来的是一条光盘 ID），
        // 不取自目录（ADR-0011）。别的盘验不验都不影响它干不干净。
        let is_disc = found.ids.iter().any(|id| id.kind == disc::IdKind::DiscId);
        let nkit_clean = found.nkit != Some(true) && (!is_disc || found.nkit.is_some());
        for id in &found.ids {
            probed.evidence.push(Evidence {
                member: it.member.clone(),
                inner: it.inner.clone(),
                id: id.clone(),
                from_content: true,
                nkit_clean,
            });
        }
        if let Some(index) = it.unit {
            // 读出标识了，第一命中层那句「压缩镜像 / 目录树转储这一层认不了」就过时了
            // ——留着它，报告会把一个已经认出来的变体记成「无判据」。
            if !found.ids.is_empty() {
                units[index].blocked = None;
            } else if let Some(note) = &found.note
                // **只在那条本来就没判据时换掉它**：容器里那条有零解压的 CRC-32，
                // 给它盖一句「认不了」会让报告拿它当无判据的理由。
                && units[index].print.is_none()
            {
                units[index].blocked = Some(note.clone());
            }
            units[index].disc = Some(found);
        }
    }
    // 四、**名字里那个 TitleID 是一条独立的依据**（票 09）。它永远不自动通过——
    // 目录只是强先验（ADR-0011），这一条连内容都没看。
    if let Some(id) = serial::title_id_in_name(file_name_of_key(&variant.key))
        && !probed.evidence.iter().any(|seen| seen.id.key == id.key)
    {
        probed.evidence.push(Evidence {
            member: variant.main_key.clone(),
            inner: String::new(),
            id,
            from_content: false,
            nkit_clean: true,
        });
    }
    Ok(probed)
}

/// 这份内容值不值得为它读那几百 KB。
///
/// **已经精确命中的不必再探**——序列号是第二命中层，第一层办成了的事不必重办。
///
/// **唯一的例外是 NKit，而且这条例外不看 CRC 撞上了什么。** 票据原话：「NKit 检测在
/// 任何 CRC 匹配之前执行」。所以判据取自[内容自己的形态](disc::may_hold_nkit)——
/// 可能是一张 GC / Wii 光盘的，撞上了也照验。拿「撞上的那条记录说它是 GC / Wii」当判据
/// 就把顺序倒过来了：CRC 的结论反过来决定要不要验 NKit，而 NKit 存在的理由正是
/// **CRC 会骗人**。
fn worth_probing(unit: &ContentUnit) -> bool {
    !unit.hits.iter().any(|(hit, _)| hit.is_exact()) || disc::may_hold_nkit(&unit.name)
}

/// 探一个裸文件：只读前若干字节。
fn read_disc_bare(
    library: &dyn LibraryFs,
    path: &Path,
    wanted: &Wanted,
    slot: &mut Option<Probe>,
) -> u64 {
    let Some(shell) = disc::by_name(&wanted.name) else {
        return 0;
    };
    let limit = disc::probe_len(shell);
    let head = match library.read_head(path, limit) {
        Ok(head) => head,
        Err(error) => {
            *slot = Some(Probe::Missed(format!("读不动：{error}")));
            return 0;
        }
    };
    let read = head.len() as u64;
    *slot = Some(Probe::Read(disc::probe(&wanted.name, &head, wanted.size)));
    read
}

/// 探容器里的几条：一趟打开，每条只要前若干字节（[`Demand::Prefix`]）。
fn read_disc_from_container(
    library: &dyn LibraryFs,
    path: &Path,
    wanted: &[Wanted],
    indexes: &[usize],
    facts: &mut [Option<Probe>],
) -> u64 {
    let listing = match container::list(library, path) {
        Ok(listing) => listing,
        Err(error) => {
            for at in indexes {
                facts[*at] = Some(Probe::Missed(format!("容器读不动：{error}")));
            }
            return 0;
        }
    };
    // ⭐ **solid block 的代价要在开工前算清楚**（ADR-0014 1.6）。7z 的一个块里几个条目是
    // 连着压的，解压器**跳不过**中间的字节——想读排在后面那一条，前面那些就得先解出来
    // 扔掉。真机上撞见过一次：一份 7z 里那个 `.iso` 排在几 GB 的东西后面，
    // 「只读几百字节」当场变成解几个 GB，一个变体卡住整趟识别。
    //
    // 判据零解压可得（`block` 与每条的未压缩大小都在容器头里），所以这里**在下计划之前**
    // 就把太贵的那些踢出去，并如实说为什么——而不是等它跑几分钟。
    let mut before: BTreeMap<usize, u64> = BTreeMap::new();
    {
        let mut running: BTreeMap<usize, u64> = BTreeMap::new();
        for (index, entry) in listing.contents.entries.iter().enumerate() {
            let Some(block) = entry.block else { continue };
            let sum = running.entry(block).or_insert(0);
            before.insert(index, *sum);
            *sum = sum.saturating_add(entry.size);
        }
    }
    let position: BTreeMap<&str, usize> = listing
        .contents
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.path.as_str(), index))
        .collect();
    let mut asked: BTreeMap<&str, u64> = BTreeMap::new();
    for at in indexes {
        let it = &wanted[*at];
        let Some(shell) = disc::by_name(&it.name) else {
            continue;
        };
        let ahead = position
            .get(it.inner.as_str())
            .and_then(|index| before.get(index))
            .copied()
            .unwrap_or(0);
        if ahead > MAX_SOLID_DRAIN {
            // 这是一条**结论**不是一次读失败：判据（块与每条的未压缩大小）零解压可得，
            // 只要容器没变就一直成立，所以它照样落库，第二趟不必再把容器头解析一遍。
            facts[*at] = Some(Probe::Read(disc::Facts {
                note: Some(format!(
                    "solid 块太深：这一条前面还压着 {}，要解开才够得到它，而这一层只想读几百字节",
                    crate::report::human_bytes(ahead)
                )),
                ..disc::Facts::default()
            }));
            continue;
        }
        asked.insert(it.inner.as_str(), disc::probe_len(shell) as u64);
    }
    if asked.is_empty() {
        return 0;
    }
    let plan = ReadPlan::new(&listing, |entry| match asked.get(entry.path.as_str()) {
        Some(limit) => Demand::Prefix(*limit),
        None => Demand::Skip,
    });
    let mut got: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let outcome = container::read_entries(library, path, &listing, &plan, &mut |entry, reader| {
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        got.insert(entry.path.clone(), buffer);
        Ok(())
    });
    if let Err(error) = outcome {
        for at in indexes {
            if facts[*at].is_none() {
                facts[*at] = Some(Probe::Missed(format!("解不开：{error}")));
            }
        }
    }
    let mut read = 0;
    for at in indexes {
        let it = &wanted[*at];
        let Some(bytes) = got.get(&it.inner) else {
            continue;
        };
        read += bytes.len() as u64;
        facts[*at] = Some(Probe::Read(disc::probe(&it.name, bytes, it.size)));
    }
    read
}

/// 把撞出来的东西折成候选、结论，以及作品与发行版。
fn assemble(
    catalog: &mut Catalog,
    variant: &VariantRow,
    units: &[ContentUnit],
    evidence: &[Evidence],
    from_serial: &mut Vec<(Candidate, Option<String>)>,
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
    // **序列号层**的候选并进来，父条目名一起带着——ADR-0010 拿 `cloneof` 把同一部
    // **作品**下的几个发行版归堆，序列号撞出来的那条与哈希撞出来的那条在这件事上没有区别。
    let first_layer_accepted = scored.iter().any(|(candidate, _)| candidate.accepted);
    let serial_accepted = from_serial.iter().any(|(candidate, _)| candidate.accepted);
    if serial_accepted && !first_layer_accepted {
        state.serial_only += 1;
    }
    scored.append(from_serial);
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
        let work = state
            .projector
            .work(catalog, &parsed.work, Provenance::Identified)?;
        work_id = Some(work);
        // **汉化版条目是变体不是发行版**（ADR-0012）：TOSEC 的 `[tr zh]` 条目说的是
        // 「有人把某个发行版汉化了」，它自己不是一次官方发行。认得出是哪部作品，
        // 认不出它基于哪一条发行版——那就只挂作品，发行版留空，等裁决补。
        if candidate.chinese != Some(ChineseMark::FanTranslated) {
            let id = state
                .projector
                .dat_release(catalog, work, candidate, &parsed)?;
            release_id = Some(id);
            scored[best].0.release_id = Some(id);
        }
    }
    let candidates: Vec<Candidate> = scored.into_iter().map(|(candidate, _)| candidate).collect();

    let blocked: Vec<&str> = units
        .iter()
        .filter_map(|unit| unit.blocked.as_deref())
        .collect();
    // **判据不只是哈希**：光盘那一层读出来的标识同样是判据。不算进来的话，一份读出了
    // 序列号却撞不上 DAT 的镜像会被记成「无判据」——而那是在说谎，判据拿到了，
    // 只是 DAT 里没有这一条。
    //
    // 按 `(成员, 内部路径)` 去重：同一份内容既算过哈希、又读出了标识时只算一份。
    // 而**目录树转储里那份 `param.sfo` 压根不在 `units` 里**（它的角色是内部资源），
    // 所以只数 units 会把整个 PSV 平台记成没有判据。
    let mut with_evidence: BTreeSet<(&str, &str)> = units
        .iter()
        .filter(|unit| {
            unit.print.is_some()
                || unit
                    .disc
                    .as_ref()
                    .is_some_and(|facts| !facts.ids.is_empty())
        })
        .map(|unit| (unit.member.as_str(), unit.inner.as_str()))
        .collect();
    for found in evidence.iter().filter(|found| found.from_content) {
        with_evidence.insert((found.member.as_str(), found.inner.as_str()));
    }
    let usable = with_evidence.len();
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
    let unverified = unit.nkit().is_none() && matches!(hit.platform.as_str(), "NGC" | "WII");
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

/// 把一条结论落成中立库里的**作品**与**发行版**那几行。
///
/// **识别与裁决共用它，而且必须共用**：两条路各写一遍「找出或建出一行作品」，
/// 同一部作品迟早会攒出两行——而导出时的**收敛**按作品走，两行就是两个前端条目。
///
/// 它也是 [`triage`](crate::triage) 那一侧的同一件东西：裁决一落下就立刻在中立库里
/// 看得见（不必等下一趟识别），而下一趟识别照沉淀库重放一遍，结果与这次一模一样。
#[derive(Debug, Default)]
pub struct Projector {
    /// 作品名 → id。同一部作品的几个发行版共用一行。
    works: BTreeMap<String, i64>,
    /// 发行版的去重键 → id。
    releases: BTreeMap<String, i64>,
    /// 有裁决点过名的作品。
    verdict_works: BTreeSet<String>,
}

impl Projector {
    /// 一个空的。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 找出（必要时建出）一个**作品**行。识别撞出来的与裁决定下来的**共用这一张表**。
    ///
    /// 共用是必须的：一部作品两行的话，导出时的**收敛**会把它拆成两个前端条目，而作品名
    /// 正是刮削的锚点（`catalog::scrape`）。于是来路这样定——**只要有一条裁决点过它的名，
    /// 这一行就算裁决的**，先来后到不影响最终的样子。
    fn work(
        &mut self,
        catalog: &mut Catalog,
        name: &str,
        origin: Provenance,
    ) -> Result<i64, CatalogError> {
        let id = match self.works.get(name).copied() {
            Some(id) => id,
            None => match catalog.work_named(name)? {
                Some(id) => {
                    self.works.insert(name.to_string(), id);
                    id
                }
                None => {
                    let id = catalog.add_work(name, origin)?;
                    self.works.insert(name.to_string(), id);
                    if origin == Provenance::Verdict {
                        self.verdict_works.insert(name.to_string());
                    }
                    return Ok(id);
                }
            },
        };
        if origin == Provenance::Verdict && self.verdict_works.insert(name.to_string()) {
            catalog.set_work_origin(id, origin)?;
        }
        Ok(id)
    }

    /// 识别撞出来的那条 DAT 条目对应的发行版。一条 DAT 条目就是一条发行版。
    fn dat_release(
        &mut self,
        catalog: &mut Catalog,
        work: i64,
        candidate: &Candidate,
        parsed: &naming::Parsed,
    ) -> Result<i64, CatalogError> {
        // **数字世代不必特殊对待**（ADR-0019）：那里港服与美服共用同一个 TitleID，
        // 本来就是 DAT 里的同一条条目，于是自然只有一条发行版，中文落在 `languages` 上。
        let key = format!("{}|{}|{}", candidate.source, candidate.dat, candidate.game);
        if let Some(id) = self.releases.get(&key) {
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
        self.releases.insert(key, id);
        Ok(id)
    }

    /// 裁决说出口的那次发行。
    fn verdict_release(
        &mut self,
        catalog: &mut Catalog,
        work: i64,
        facts: &verdict::Facts,
        platform: Option<&str>,
    ) -> Result<i64, CatalogError> {
        // 键取裁决说出口的那几样。同一次发行被裁决过几次（几个变体基于它），
        // 只该有一行发行版——多出来的行会让导出时的**收敛**把一个条目拆成好几个。
        let key = format!(
            "裁决|{}|{}|{}|{}|{}",
            facts.work,
            platform.unwrap_or(""),
            facts.region.as_deref().unwrap_or(""),
            facts.serial.as_deref().unwrap_or(""),
            facts.languages.as_deref().unwrap_or(""),
        );
        if let Some(id) = self.releases.get(&key) {
            return Ok(*id);
        }
        // 跨调用的去重靠库自己：`romcat triage decide` 一次一批，两批之间这张表是空的。
        let id = match catalog.release_like(
            work,
            platform,
            facts.region.as_deref(),
            facts.serial.as_deref(),
            facts.languages.as_deref(),
        )? {
            Some(id) => id,
            None => catalog.add_release(
                work,
                platform,
                facts.region.as_deref(),
                facts.serial.as_deref(),
                facts.languages.as_deref(),
                Provenance::Verdict,
            )?,
        };
        self.releases.insert(key, id);
        Ok(id)
    }

    /// 把一条**裁决**落成这个变体的结论；`None` 表示这条裁决不产生结论。
    ///
    /// 产出的候选是**高置信、自动通过**的——它不是猜出来的，是人看过之后定下来的
    /// （ADR-0002 那三档里最上面那一档）。**依据**照样写全：锚是哪一种、钉在哪串字节上、
    /// 汉化组与版本是什么，事后一样复核得了。
    ///
    /// **「都不对而且认不出」那一档返回 `None`**，而不是一份空结论：它只是记下来别再问
    /// 第二遍，结论本身没有变（照旧是未命中或无判据），连候选都该原样留着。返回一份
    /// 空结论写回去的话，那个变体识别撞出来的候选会被连带清掉——而它们是事实，
    /// 与人认不认得出无关。调用方拿到 `None` 该走
    /// [`Catalog::set_identification_reason`](crate::catalog::Catalog::set_identification_reason)。
    ///
    /// # Errors
    /// 写中立库失败时返回错误。
    pub fn project(
        &mut self,
        catalog: &mut Catalog,
        variant: &VariantRow,
        found: &Verdict,
        member: &str,
        inner: &str,
    ) -> Result<Option<Identification>, CatalogError> {
        let record = |work_id, release_id, state_of, reason, candidates| Identification {
            variant_key: variant.key.clone(),
            state: state_of,
            reason,
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id,
            release_id,
            candidates,
        };
        match &found.decision {
            Decision::Release(facts) => {
                let work = self.work(catalog, &facts.work, Provenance::Verdict)?;
                let platform = facts
                    .platform
                    .clone()
                    .or_else(|| variant.platform.clone())
                    .unwrap_or_default();
                let named = Some(platform.as_str()).filter(|text| !text.is_empty());
                let release = self.verdict_release(catalog, work, facts, named)?;
                let candidate = Candidate {
                    member_key: member.to_string(),
                    inner: inner.to_string(),
                    confidence: Confidence::High,
                    accepted: true,
                    source: VERDICT_SOURCE.to_string(),
                    dat: VERDICT_DAT.to_string(),
                    platform,
                    game: facts.work.clone(),
                    rom: file_name_of_key(if inner.is_empty() { member } else { inner })
                        .to_string(),
                    hashed_as: Convention::AsIs,
                    dat_convention: Convention::AsIs,
                    evidence: found.evidence(),
                    chinese: facts.chinese,
                    serial: facts.serial.clone(),
                    release_id: Some(release),
                };
                Ok(Some(record(
                    Some(work),
                    Some(release),
                    State::Matched,
                    None,
                    vec![candidate],
                )))
            }
            // **明说的**「没有发行版」——不再靠「作品有、发行版空」推断（原挂账 D48）。
            Decision::NoRelease { work } => {
                let work_id = match work {
                    Some(name) => Some(self.work(catalog, name, Provenance::Verdict)?),
                    None => None,
                };
                let skip = scope::Skip::NoRelease("裁决记着它没有发行版".to_string());
                Ok(Some(record(
                    work_id,
                    None,
                    State::Skipped,
                    Some(skip.recorded()),
                    Vec::new(),
                )))
            }
            Decision::Unknown => Ok(None),
        }
    }
}
