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
//! ## 第三命中层：卡带内部头（票 10）
//!
//! CRC 那一层对卡带世代的**汉化版**同样结构性地不够用，而且缺口正落在中文玩家存量
//! 最大的两个平台上：票 07 实测 GBA 5.8%、NDS 4.4%、GBC 0%，抽样确认那批未命中全是
//! 汉化版。**汉化补丁通常不改内部头**（调研 C.2、D.3.1），于是 GBA 头里的 game code、
//! NDS 头里的 gamecode 照样说得出「这是哪个游戏」——缺的只是「这是谁汉化的第几版」，
//! 那归**裁决**（ADR-0008）。
//!
//! 因此[卡带那一层](cart)的候选**永远不自动通过**：它只说得到**发行版**这一层。
//! 这与光盘序列号那一层不同——一张汉化过的盘与原版盘是两份不同的转储、各自有哈希，
//! 序列号对上就是同一次发行；而一张汉化过的卡与原版卡**共用同一个游戏码**。
//!
//! ## 倒数第二层：文件名规则加中文离线模糊匹配（票 11）
//!
//! 上面三层的判据都来自**内容自己的字节**。数据库覆盖不到的那批变体，字节说不出话了，
//! 手上只剩一个文件名——而这个库里的文件名多半是中文，中文在 No-Intro / Redump /
//! TOSEC 里一个字都没有。于是[这一层](fuzzy)把文件名剥成正题，去撞一份**中文离线
//! 数据源**（[`zh`](crate::zh)），用**平台与年份**做交叉校验。
//!
//! 它**永远不自动通过**，理由比卡带那一层还硬：**它一个字节都没看**。结论一律进
//! 待确认队列（ADR-0002）。
//!
//! ## 还有一条 SHA-1 的窄路
//!
//! DAT 库里有一批记录连 `crc32` 与 `size` 两列都是空的（GoodNES 实测 30,244 条，
//! 其中 748 条中文汉化），第一命中层够不到它们。SHA-1 要整份读一遍，所以它**只对
//! 真有这种弹药的平台、而且第一层没办成时**才付（`has_sha1_ammo`）——ADR-0008 的修订段
//! 算过这笔账，对光盘世代不值，对卡带世代反过来。
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

pub mod cart;
pub mod disc;
pub mod fingerprint;
pub mod fuzzy;
pub mod header;
pub mod ident;
pub mod iso9660;
pub mod model;
pub mod naming;
pub mod report;
pub mod scope;
pub mod serial;
pub mod sfo;
pub mod switch;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use unicode_normalization::{IsNormalized, is_nfd_quick};

use crate::catalog::identify::{
    Candidate, CartFactRow, Confidence, ContentHash, DiscFactRow, EntryFact, Identification,
    ModelAnswerRow, SwitchFactRow,
};
use crate::catalog::{Catalog, CatalogError, Provenance, Roots, State, VariantRow};
use crate::classify::{self, Category};
use crate::container::{self, ContainerKind, Demand, ReadPlan, volume};
use crate::dat::chinese::ChineseMark;
use crate::dat::{Convention, DatRepo, Hit, Matched, RepoError};
use crate::fs::{DirCache, LibraryFs};
use crate::path::file_name_of_key;
use crate::report::thousands;
use crate::scan::CancelToken;
use crate::shape::Role;
use crate::titledb;
use crate::verdict::{self, Decision, Verdict};

use fingerprint::{Fingerprint, Headerless, Want};
use fuzzy::Naming;
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
    /// **TitleID 索引**读写失败（票 27）。
    #[error(transparent)]
    TitleDb(#[from] titledb::store::StoreError),
}

/// 这一趟能用的**弹药**：撞哈希的 DAT 库、裁决攒出来的沉淀库、撞名字的中文离线索引。
///
/// 四样捆在一起传而不是散成四个参数，是因为它们**永远同进同出**：每一层识别都要问过
/// 它们才敢说话。第四样（票 12 的模型推断）加进来时，这条文档预言的那件事真的发生了
/// ——而因为它们本来就捆着，调用处一处都没改。
#[derive(Clone, Copy)]
pub struct Ammo<'a> {
    /// DAT 库：世上有哪些发行版。
    pub repo: &'a DatRepo,
    /// **沉淀库**：人裁决过什么。它先说话——裁决过的东西不必再撞一遍 DAT（ADR-0008）。
    pub verdicts: &'a verdict::Index,
    /// 文件名那一层认得的东西与调得动的参数（票 11）。
    pub naming: &'a Naming<'a>,
    /// **模型推断那一层**的缓存、价钱、上限与（要真问时的）网络句柄（票 12）。
    pub guessing: &'a model::Guessing<'a>,
    /// **第三方 TitleID 数据库**：Switch 那一层拿它把 ContentId 反查成
    /// (TitleID, 版本)（票 27）。
    ///
    /// **它是 `Option` 而别的三样不是**，因为这一层没有它照样跑得动：容器的明文
    /// 文件名表免密钥就说得出 TitleID，查表只是把结论从「哪个游戏」抬到
    /// 「哪个游戏的哪个版本」。没取过就是 `None`，依据里会如实写这一句。
    pub titledb: Option<&'a titledb::store::Store>,
}

/// 识别的选项。
#[derive(Debug, Clone)]
pub struct Options {
    /// 主库那**一组根**：拿变体的键第一段查出那块盘在哪。只有需要回盘读字节时才用得上。
    pub roots: Roots,
    /// 允许回盘读吗。关掉之后**一个字节都不读主库**：容器里零解压可得的 CRC-32
    /// 照撞，裸文件与去头那套则报「无判据」。
    pub read_library: bool,
    /// 单份内容读到多大就不读了；`None` 是不设上限。
    pub max_read_bytes: Option<u64>,
    /// 每算完多少个变体就把这一批结论写进中立库。
    pub write_batch: usize,
}

impl Options {
    /// 对着一组主库根的默认选项。
    #[must_use]
    pub fn new(roots: Roots) -> Self {
        Self {
            roots,
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

/// **卡带那一层**这一趟干了什么。
///
/// 六个数永远同进同出（一层探测的全部账目），所以是一个类型而不是六个字段：
/// [`Run`] 与 [`Outcome`] 上各摆一份，`run()` 收尾时整个搬过去——散成六个字段的话，
/// 加一个计数器要改三处，而漏掉一处不会有任何人告诉你。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CartCount {
    /// 探了几份内容。
    pub probed: u64,
    /// 其中读出了游戏码的。
    pub with_id: u64,
    /// 撞出来的候选条数。
    pub candidates: u64,
    /// **靠这一层才认出来**的变体数。
    pub only: u64,
    /// 这一趟没读到几份。**读不到不是结论，不落库**（ADR-0021）。
    pub missed: u64,
    /// 内部头说的平台与目录声明的平台对不上的份数（ADR-0011）。
    pub conflicts: u64,
}

/// **文件名那一层**这一趟干了什么（票 11）。
///
/// 与 [`CartCount`] 同一个道理：这几个数永远同进同出，散成六个字段的话，
/// 加一个计数器要改三处，而漏掉一处不会有任何人告诉你。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FuzzyCount {
    /// 为几个变体撞过。
    pub variants: u64,
    /// 一共拿几串字去撞的。
    pub tried: u64,
    /// 撞出来的候选条数。
    pub candidates: u64,
    /// 其中两道交叉校验都对上、够得着中置信的。
    pub strong: u64,
    /// **靠这一层才有候选**的变体数。
    pub only: u64,
    /// 有几个名字因为**还是乱码**没敢撞（票 03 有损转换留下的，见 [`fuzzy`]）。
    pub garbled: u64,
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
    /// 卡带那一层这一趟干了什么。
    pub cart: CartCount,
    /// SHA-1 那一层撞出来的候选条数。
    pub sha1_hits: u64,
    /// **靠 SHA-1 才认出来**的变体数。
    pub sha1_only: u64,
    /// 文件名那一层这一趟干了什么（票 11）。
    pub fuzzy: FuzzyCount,
    /// **Switch 那一层**这一趟干了什么（票 27）。
    pub switch: switch::Found,
    /// **靠 Switch 那一层才认出来**的变体数。
    pub switch_only: u64,
    /// **模型推断那一层**这一趟干了什么（票 12）：残渣多少、问了几个请求、花了多少。
    pub model: model::ModelCount,
    /// 剥离规则**归不了类的记号**，按出现次数从多到少。
    ///
    /// **它是这一层的主要产出之一**：维护者照着它往剥离规则里补，补完重跑一遍就看得见
    /// 效果——那正是「规则是配置而不是硬编码」真正兑现的地方。
    pub unknown_marks: Vec<(String, u64)>,
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
    /// 卡带那一层探出来的事实：内部头、游戏码、平台（票 10）。
    cart: Option<cart::Facts>,
    /// Switch 那一层探出来的事实：明文文件名表、TitleID、结构指纹（票 27）。
    switch: Option<switch::Facts>,
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
    ammo: &Ammo<'_>,
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
        ammo: ammo.repo.platforms()?,
        // **只有真有 SHA-1 弹药的平台才付整份读一遍那笔钱**（票 10）。真机实测这份
        // 名单上的卡带平台只有 FC 与 MD——GoodNES 那两份只记 SHA-1。
        sha1_ammo: ammo.repo.sha1_only_platforms()?,
        // **独占目录**：这个目录下只有这一个变体。文件名那一层据此决定敢不敢拿目录名
        // 去撞——`FC/` 底下三千个 zip 挤在一起时，目录名属于谁根本说不清
        // （判据与刮削那一侧认本地媒体的规则同源，`scrape::local`）。
        exclusive_dirs: exclusive_dirs(&variants),
        // 谁挨着谁——模型推断那一层的「同目录还有」（票 12）。
        neighbours: Neighbours::build(&variants),
        ..Run::default()
    };
    let mut batch: Vec<Identification> = Vec::new();
    let mut interrupted = false;

    for variant in &variants {
        if cancel.is_cancelled() {
            interrupted = true;
            break;
        }
        let record = identify_variant(library, catalog, ammo, options, variant, &mut state)?;
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

    // ⭐ **第二趟：模型推断兜底**（票 12）。
    //
    // 它跑在主循环**之后**而不是里面，理由有两条，都不是风格问题：
    //
    // 1. **批量打包**要先把一批凑齐——边跑边问就退化成逐条问。
    // 2. **「一共要问多少、要花多少」这个数，只有残渣全部确定之后才说得出来**，
    //    而那正是发第一个请求之前必须先摆出来的东西（[`model::Plan`]）。
    //
    // 被中断时整个不跑：那时残渣是半份的，照着半份算出来的计划会骗人。
    if !interrupted {
        ask_model(catalog, ammo, &variants, &mut state, cancel)?;
    }

    let mut report = IdentifyReport::build(catalog, ammo.repo)?;
    // 花费要留得下痕迹：`--json` 存的是这份报告，只在标准错误上说一句的话，跑完就没了。
    if state.model.residue > 0 {
        report.model = Some(state.model.clone());
    }

    Ok(Outcome {
        report,
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
        cart: state.cart,
        sha1_hits: state.sha1_hits,
        sha1_only: state.sha1_only,
        fuzzy: state.fuzzy,
        switch: state.switch,
        switch_only: state.switch_only,
        unknown_marks: rank_marks(state.unknown_marks),
        model: state.model,
    })
}

/// **模型推断那一层**：把残渣打包问出去，答案落库，候选追加到已有的结论上。
///
/// 三件事按这个顺序，一件都不能挪：
///
/// 1. **先算计划**。它在发第一个请求之前就说得出问多少个、打成几个请求、花费的下界与
///    上界——而 `--model-plan` 到这里就停了，一个请求都不发。
/// 2. **一批一批问**。每收到一个响应就**当场落库**：这一层每一条都付过钱，攒到最后
///    一次性写的话，跑到一半被 Ctrl-C 就等于把已经花掉的钱扔了。
/// 3. **候选只追加，不重写结论**（[`Catalog::append_candidates`]）。
fn ask_model(
    catalog: &mut Catalog,
    ammo: &Ammo<'_>,
    variants: &[VariantRow],
    state: &mut Run,
    cancel: &CancelToken,
) -> Result<(), IdentifyError> {
    let guessing = ammo.guessing;
    let pending = std::mem::take(&mut state.model_pending);
    if pending.is_empty() {
        return Ok(());
    }
    let questions: Vec<model::Question> = pending.iter().map(|one| one.question.clone()).collect();
    let plan = model::plan(
        &guessing.model,
        guessing.price,
        &guessing.limits,
        &questions,
        state.model.from_cache,
        &guessing.checked,
    );
    state.model.estimated_input_tokens = plan.input_tokens;
    state.model.batch_size = u64::try_from(guessing.limits.batch.max(1)).unwrap_or(0);
    // **摆在第一个请求之前。** 「花费可预估」说的是花钱之前看得见，
    // 不是跑完之后报告里有个数。
    if let Some(announce) = guessing.announce {
        announce(&plan);
    }
    state.model.plan = Some(plan);
    let Some(net) = guessing.net else {
        // 只用缓存的那一趟（含 `--model-plan`）：计划算出来了，一个请求都不发。
        return Ok(());
    };
    let by_key: BTreeMap<&str, &VariantRow> = variants
        .iter()
        .map(|variant| (variant.key.as_str(), variant))
        .collect();
    let batch = guessing.limits.batch.max(1);
    for chunk in pending.chunks(batch) {
        if cancel.is_cancelled() {
            break;
        }
        let asked: Vec<model::Question> = chunk.iter().map(|one| one.question.clone()).collect();
        let body = model::body_of(&guessing.model, &guessing.limits, &asked);
        let before = net.usage();
        let response = match net.ask(&body) {
            Ok(response) => response,
            // **停下来不是错误**：手里已经落库的答案照旧有效，报告里说清为什么停。
            Err(model::Failure::Halt(halt)) => {
                state.model.halted = Some(halt.describe());
                state.model.halt = Some(halt.kind());
                break;
            }
            // 只有这一批没问成，别的照问。
            Err(model::Failure::Skip { why }) => {
                if state.model.halted.is_none() {
                    state.model.halted = Some(format!("有一批没问成：{why}"));
                }
                continue;
            }
        };
        let after = net.usage();
        state.model.usage = after;
        catalog.put_model_call(
            &guessing.model,
            u64::try_from(chunk.len()).unwrap_or(0),
            after.input_tokens - before.input_tokens,
            after.output_tokens - before.output_tokens,
            after.cost_micros - before.cost_micros,
        )?;
        // 真的答话的那个模型照抄响应里的，未必等于我们请求的那个。
        let answered_by = response
            .get("model")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(guessing.model.as_str())
            .to_string();
        let answers = model::parse_answers(&response, chunk.len());
        let mut rows: Vec<ModelAnswerRow> = Vec::new();
        for (at, answer) in &answers {
            let Some(one) = chunk.get(*at) else {
                continue;
            };
            rows.push(ModelAnswerRow {
                variant_key: one.question.variant_key.clone(),
                ask: one.ask.clone(),
                model: answered_by.clone(),
                answer: answer.to_json(),
            });
        }
        catalog.put_model_answers(&rows)?;
        // **问出去的按这一批的条数算，不按答回来的条数算。** 少答的那几条钱一样付了，
        // 记成「没问」的话，报告会说这一趟比实际便宜。
        state.model.asked += u64::try_from(chunk.len()).unwrap_or(0);
        for (at, answer) in &answers {
            let Some(question) = chunk.get(*at).map(|one| &one.question) else {
                continue;
            };
            let Some(variant) = by_key.get(question.variant_key.as_str()) else {
                continue;
            };
            let built = fold_answer(&mut state.model, variant, question, answer, &answered_by);
            catalog.append_candidates(&question.variant_key, &built)?;
        }
    }
    Ok(())
}

/// 只装着一个变体的那些目录。
///
/// 它一趟算完（`variants` 本来就整份在手上），而不是每个变体查一次库——真库上那是
/// 46,444 次查询，换来的是一件早就摆在眼前的事实。
fn exclusive_dirs(variants: &[VariantRow]) -> BTreeSet<String> {
    let mut counts: BTreeMap<&str, u32> = BTreeMap::new();
    for variant in variants {
        if let Some(dir) = parent_dir(&variant.key) {
            *counts.entry(dir).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(dir, _)| dir.to_string())
        .collect()
}

/// 键的上一级目录；顶层的东西没有。
fn parent_dir(key: &str) -> Option<&str> {
    let at = key.rfind('/')?;
    (at > 0).then(|| &key[..at])
}

/// **谁挨着谁**：每个目录下的变体名字，连每个变体在自己那个目录里排第几。
///
/// 模型推断那一层的「同目录还有」（票 12）要它。捏成一个类型而不是两张散着的表，
/// 是因为**那个下标离了那张名单没有意义**——两张表分开传，早晚有人拿甲的下标去查乙。
#[derive(Debug, Default)]
struct Neighbours {
    /// 目录 → 这个目录下每个变体的名字，按键的顺序。
    by_dir: BTreeMap<String, Vec<String>>,
    /// 变体的键 → 它在上面那张名单里排第几。
    ///
    /// **不存它就得每次现找**（`position()`），而那是一次线性扫描：真库里
    /// `3ds/3DSCH/` 这样的目录底下几千个变体，一次识别就是几百万次比较——
    /// 实测那让整趟识别从 38 秒涨到 91 秒。
    at: BTreeMap<String, usize>,
}

impl Neighbours {
    /// 一趟算完（`variants` 本来就整份在手上），而不是每个变体查一次库。
    ///
    /// **名字整份留着，不截断**：全库加起来正好是变体的条数（真机 46,444 个名字，
    /// 几 MB）。截断的代价是真的——只留目录里头几个的话，同一个目录下每一条残渣
    /// 拿到的**是同一组名字**，与它自己旁边是什么无关，那就不是「同目录其他文件」，
    /// 是「本目录头几个」。
    fn build(variants: &[VariantRow]) -> Self {
        let mut out = Self::default();
        for variant in variants {
            let Some(dir) = parent_dir(&variant.key) else {
                continue;
            };
            let slot = out.by_dir.entry(dir.to_string()).or_default();
            out.at.insert(variant.key.clone(), slot.len());
            slot.push(file_name_of_key(&variant.key).to_string());
        }
        out
    }

    /// 同一个目录下**紧挨着它**的那几个变体叫什么。
    ///
    /// **取的是它左右两侧的邻居，不是目录的开头**：一个装着三千个 zip 的目录里，
    /// 头几个的名字对第两千条说明不了什么，而挨着它的那几个多半是同一批东西
    /// （同一个整理者、同一次打包、同一个系列）——那才是这条上下文的全部价值。
    ///
    /// **自己那一条剔掉**：问「这是什么」的时候把问题本身混进上下文里，
    /// 只会让模型把它当成一条旁证。
    fn around(&self, key: &str) -> Vec<String> {
        let Some(dir) = parent_dir(key) else {
            return Vec::new();
        };
        let Some(names) = self.by_dir.get(dir) else {
            return Vec::new();
        };
        let at = self.at.get(key).copied().unwrap_or(0);
        let from = at.saturating_sub(model::CONTEXT_LIMIT / 2);
        names
            .iter()
            .enumerate()
            .skip(from)
            .filter(|(seat, _)| *seat != at)
            .map(|(_, name)| name.clone())
            .take(model::CONTEXT_LIMIT)
            .collect()
    }
}

/// 把一条答复折成候选，顺手记进账上。
///
/// 缓存那一路与真问那一路**共用它**：两边都要「折候选 + 数一笔」，各写一遍的话，
/// 加一个计数器就会漏掉其中一处——而漏掉的那一处不会有任何人告诉你
/// （同 [`CartCount`] 与 [`FuzzyCount`] 捆成一个类型的道理）。
fn fold_answer(
    count: &mut model::ModelCount,
    variant: &VariantRow,
    question: &model::Question,
    answer: &model::Answer,
    answered_by: &str,
) -> Vec<Candidate> {
    if answer.guesses.is_empty() {
        // **模型自己说不出**是这一层的正当产出，不是失败：那批模拟器、存档与主题包
        // 压根不是任何一部作品。把它数出来，才看得出这一层花的钱有多少落在了这上面。
        count.speechless += 1;
        return Vec::new();
    }
    let built = model::candidates_of(variant, question, answer, answered_by);
    count.with_candidates += 1;
    count.candidates += u64::try_from(built.len()).unwrap_or(0);
    built
}

/// 认不出的记号按出现次数排个序，多的在前。
fn rank_marks(marks: BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = marks.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out.truncate(40);
    out
}

/// 一趟识别路上攒着的东西。
#[derive(Default)]
struct Run {
    progress: Progress,
    /// 把 NFC 的键折回盘上真名那条退路上，每个目录只列一次（[`library_path`]）。
    ///
    /// **住在这儿而不是每次现开一份**：一个目录名是分解形式，它底下整棵子树的键都要
    /// 走那条退路，几百个变体会把同一份 listing 读上几百遍。挂在这一趟上也就够了——
    /// 主库只读（ADR-0004），一趟里盘上的名字不会变。
    dirs: DirCache,
    /// DAT 库覆盖到的平台。
    ammo: BTreeSet<String>,
    /// **值得为它算 SHA-1** 的平台：库里有第一命中层够不着的记录的那几个（票 10）。
    sha1_ammo: BTreeSet<String>,
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
    /// 卡带那一层这一趟干了什么。
    cart: CartCount,
    /// SHA-1 那一层撞出来的候选条数。
    sha1_hits: u64,
    /// **靠 SHA-1 才认出来**的变体数。
    sha1_only: u64,
    /// 这一轮探出来的卡带事实，攒够一批写一次。
    carts: Vec<CartFactRow>,
    /// Switch 那一层这一趟干了什么（票 27）。
    switch: switch::Found,
    /// 靠 Switch 那一层才认出来的变体数（前面几层一条候选都没有）。
    switch_only: u64,
    /// 这一轮探出来的 Switch 容器事实，攒够一批写一次。
    switches: Vec<SwitchFactRow>,
    /// 只装着一个变体的那些目录（文件名那一层拿目录名去撞的前提）。
    exclusive_dirs: BTreeSet<String>,
    /// 文件名那一层这一趟干了什么。
    fuzzy: FuzzyCount,
    /// 剥离规则归不了类的记号，连出现次数。
    unknown_marks: BTreeMap<String, u64>,
    /// 谁挨着谁（模型推断那一层的「同目录还有」）。
    neighbours: Neighbours,
    /// 模型推断那一层这一趟干了什么。
    model: model::ModelCount,
    /// **落到模型推断这一层、而缓存里还没有答案**的那些问题，连它们的提问指纹。
    ///
    /// 攒起来等主循环跑完再打包问，而不是边跑边问：批量打包本来就要求先把一批凑齐，
    /// 而且「一共要问多少、要花多少」这个数只有在残渣全部确定之后才说得出来
    /// ——**而那正是发第一个请求之前必须先摆出来的东西**。
    model_pending: Vec<model::Asking>,
}

/// DAT 库里有没有这个平台的记录。平台认不出来时当作**有**——那时无从判断，
/// 而少读一次的代价是一条永远认不出来的变体。
fn has_ammo(variant: &VariantRow, state: &Run) -> bool {
    variant
        .platform
        .as_deref()
        .is_none_or(|platform| state.ammo.contains(platform))
}

/// 这个平台**值得为它算 SHA-1** 吗——库里有第一命中层够不着的记录吗（票 10）。
///
/// 与 [`has_ammo`] 那条反过来：**平台认不出来时当作没有**。那一条少读一次会丢一条候选，
/// 这一条多读一次是把整份内容读一遍，而认不出平台的东西按定义撞不上任何一份专属 DAT。
fn has_sha1_ammo(variant: &VariantRow, state: &Run) -> bool {
    variant
        .platform
        .as_deref()
        .is_some_and(|platform| state.sha1_ammo.contains(platform))
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
    catalog.put_cart_facts(&state.carts)?;
    state.carts.clear();
    catalog.put_switch_facts(&state.switches)?;
    state.switches.clear();
    catalog.write_identifications(batch)?;
    batch.clear();
    Ok(())
}

fn identify_variant(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    ammo: &Ammo<'_>,
    options: &Options,
    variant: &VariantRow,
    state: &mut Run,
) -> Result<Identification, IdentifyError> {
    let (repo, verdicts) = (ammo.repo, ammo.verdicts);
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
    // 这个平台的 DAT 里有只记 SHA-1 的记录吗。有的话，**该读的那几份顺手把 SHA-1 也
    // 算出来**——边际成本几乎为零，而且省掉了下面那一步为它们再读一遍（票 10）。
    let want = if has_sha1_ammo(variant, state) {
        Want::sha1_if_free()
    } else {
        Want::crc_only()
    };
    let read_bytes = if has_ammo(variant, state) {
        fill_in(library, options, &mut units, &cached, want, state)?
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

    // 二之二、**SHA-1 那条窄路**（票 10）。DAT 库里有一批记录连 `crc32` 与 `size` 两列
    // 都是空的（GoodNES 实测 30,244 条，其中 748 条中文汉化），第一命中层够不到它们
    // ——不是撞不上，是**根本没有可以对的那一列**。
    //
    // 它排在撞 CRC **之后**，理由与光盘那一层的 `worth_probing` 一模一样：**第一层办成了
    // 的事不必重办**，而这一层要把内容整份读一遍。真机上 FC 全平台 2.01 GiB，撞上了的
    // 占 1.97 GiB——先撞后读，读的是剩下那 0.45 GiB。
    let first_layer_empty = units.iter().all(|unit| unit.hits.is_empty());
    let crc_accepted = units
        .iter()
        .any(|unit| unit.hits.iter().any(|(hit, _)| hit.is_exact()));
    let read_bytes = read_bytes
        + if want.sha1.wanted() && !crc_accepted {
            let more = fill_in(
                library,
                options,
                &mut units,
                &cached,
                Want::pay_for_sha1(),
                state,
            )?;
            let mut added = 0;
            for unit in &mut units {
                let Some(print) = unit.print else { continue };
                if let Some(sha1) = print.sha1 {
                    let mut hits = lookup_sha1(repo, sha1, print.size, Convention::AsIs)?;
                    added += hits.len();
                    unit.hits.append(&mut hits);
                }
                // 去头那套照撞，理由与 CRC 那一层一模一样：含头与去头是两档 DAT。
                // 两套落在同一串字节上时不重复撞。
                if let Some(bare) = print.headerless_sha1()
                    && Some(bare) != print.sha1
                    && let Some((size, _)) = print.headerless_pair()
                {
                    let mut hits = lookup_sha1(repo, bare, size, Convention::Headerless)?;
                    added += hits.len();
                    unit.hits.append(&mut hits);
                }
            }
            state.sha1_hits += u64::try_from(added).unwrap_or(0);
            if added > 0 && first_layer_empty {
                state.sha1_only += 1;
            }
            more
        } else {
            0
        };

    // 三、⭐ **光盘那一层**：只读几百字节把内部标识取出来，顺便按**逻辑**偏移 0x200
    // 验 NKit。它在**接受**任何 CRC 命中之前跑完——NKit 处理过的镜像的 CRC32 可能与
    // 好转储相同（Dolphin），验完才敢说那句「命中」。
    let found = probe_discs(
        library, catalog, options, variant, &members, &mut units, state,
    )?;
    let read_bytes = read_bytes + found.read_bytes;

    // 三之二、⭐ **卡带那一层**（票 10）：哈希撞不上时，从卡带内部头里读出游戏码。
    // **汉化补丁通常不改内部头**，所以一份对不上任何数据库的汉化版照样说得出它基于
    // 哪一次发行——缺的只是「谁汉化的第几版」，那归裁决（ADR-0008）。
    let carted = probe_carts(library, catalog, options, variant, &mut units, state)?;
    let read_bytes = read_bytes + carted.read_bytes;

    // 三之三、⭐ **Switch 那一层**（票 27）：容器的**明文文件名表**免密钥就说得出
    // 「这是哪个游戏」。它与前两层不同的地方是**自己产出候选不撞 DAT**——No-Intro 的
    // 每日镜像里 334 份 DAT 一个 Switch 都没有，Redump 与 libretro 也没有，判据只能
    // 来自容器自己与第三方 TitleID 数据库。
    let switched = probe_switch(library, catalog, options, variant, &mut units, ammo, state)?;
    let read_bytes = read_bytes + switched.read_bytes;

    // 四、拿标识撞 DAT 的序列号索引。**光盘与卡带各撞一次**，为的是数得出「靠哪一层
    // 才认出来的」——两层混在一起撞，那个数就只剩一个总和。
    let mut serial_candidates = serial::candidates(repo, &found.evidence)?;
    state.serial_candidates += u64::try_from(serial_candidates.len()).unwrap_or(0);
    let mut cart_candidates = serial::candidates(repo, &carted.evidence)?;
    state.cart.candidates += u64::try_from(cart_candidates.len()).unwrap_or(0);
    // **靠卡带那一层才认出来**：前面几层一条候选都没有，而它撞出来了。
    if !cart_candidates.is_empty()
        && serial_candidates.is_empty()
        && units.iter().all(|unit| unit.hits.is_empty())
    {
        state.cart.only += 1;
    }
    serial_candidates.append(&mut cart_candidates);
    // **靠 Switch 那一层才认出来**：前面几层一条候选都没有，而它产出了。
    if !switched.candidates.is_empty()
        && serial_candidates.is_empty()
        && units.iter().all(|unit| unit.hits.is_empty())
    {
        state.switch_only += 1;
    }
    // 父条目名那一栏是 `None`：这一层的候选不来自 DAT，没有 No-Intro 的 parent/clone
    // 那套关系（ADR-0010 说的是 No-Intro 与 MAME 的）。
    serial_candidates.extend(
        switched
            .candidates
            .into_iter()
            .map(|candidate| (candidate, None)),
    );

    // 标识那几层交出来的东西捆在一起交给 `assemble`：它们同出一源，也同去一处。
    let mut from_ids = FromIds {
        evidence: found
            .evidence
            .into_iter()
            .chain(carted.evidence)
            .chain(switched.evidence)
            .collect(),
        candidates: serial_candidates,
    };
    let mut record = assemble(
        catalog,
        variant,
        &units,
        &mut from_ids,
        read_bytes,
        ammo,
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
        // **一组分卷是一个容器**（CONTEXT 的「透明容器」条）：非入口段没有自己的头，
        // 整组由入口段代表。它当附属文件时一声不吭地跳过——入口段已经把整组的内部
        // 构成交出来了，再为每一段各记一条「读不了」等于把一组数成好几个未命中。
        if volume::is_non_entry_part(file_name_of_key(key)) {
            if *role == Role::Companion {
                continue;
            }
            // 当主文件就说明**这一组的入口卷不在这个变体里**——真库里那个
            // `废都物语_资料合辑_220928.7z.006` 正是这样：`.001` 到 `.005` 都不在库里。
            visible.saw_inside = false;
            units.push(blocked_unit(
                key,
                "",
                format!(
                    "分卷的一段，而这一组的入口卷（{}）不在这个变体里",
                    volume::volume_of(file_name_of_key(key))
                        .map_or("入口卷", |it| it.scheme.entry_hint())
                ),
                true,
            ));
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
                        cart: None,
                        switch: None,
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
                    cart: None,
                    switch: None,
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
            // 归类说它是**透明容器**、穿透层却认不出来的，只剩这一版还没做的格式。
            // 说清楚是「还穿不透」而不是「没有内容」——两句话指向完全不同的下一步。
            Category::TransparentContainer => {
                visible.saw_inside = false;
                units.push(blocked_unit(
                    key,
                    "",
                    "这个容器格式还穿不透".to_string(),
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
        cart: None,
        switch: None,
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
    want: Want,
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
        let Some(demand) = demand_of(unit, want) else {
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
        let path = library_path(library, &options.roots, &mut state.dirs, &member);
        if units[indexes[0].0].in_container {
            read_bytes += read_from_container(library, &path, units, &indexes, want, state);
        } else {
            let (index, demand) = indexes[0];
            read_bytes += read_bare(library, &path, &mut units[index], demand, want, state);
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
fn demand_of(unit: &ContentUnit, want: Want) -> Option<Demand> {
    let Some(print) = unit.print else {
        // 一个字节都还没看过——裸文件就是这一档。
        return Some(Demand::All);
    };
    // **专门为 SHA-1 读一趟**（票 10）。只有 [`Sha1::Pay`] 那一档走到这儿——「顺手算」
    // 那一档绝不在这里要求读，它只是挂在别的理由已经决定要读的那一趟上。
    //
    // **体积闸在这儿是硬的**（ADR-0002 的再修订：「按文件体积决定解压深度」）：
    // 有 SHA-1 弹药的平台不全是卡带——MAME 的 `psx.xml` / `saturn.xml` / `dc.xml`
    // 那几份 `<disk>` 也只记 SHA-1，而它们说的是 CHD 那条逻辑流，与磁盘上这份
    // `.iso` 本来就对不上（ADR-0014 的修订段）。不设闸的话，一份 700 MB 的镜像会被
    // 整份读一遍去算一个注定撞不上的摘要。
    if want.sha1.pays_for_a_read() && print.sha1.is_none() && unit.size <= MAX_SHA1_READ {
        return Some(Demand::All);
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

/// **专门为 SHA-1 读一趟**时，肯读多大的一份。
///
/// 与 [`MAX_INLINE_READ`] 同一个数但不是同一件事，所以是两个常量：那一个挡的是
/// 「整份读进内存」，这一个挡的是「值不值得为一个摘要读这么多」。真机上卡带那几个
/// 平台最大的一份 NDS 卡是 512 MB，而未命中的那批平均只有几 MB——64 MiB 宽出一个
/// 数量级，同时把光盘镜像整批挡在外面。
const MAX_SHA1_READ: u64 = 64 << 20;

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
    want: Want,
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
        _ => match fingerprint::of_reader(&unit.name, unit.size, &mut handle, want) {
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
    want: Want,
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
            unit.print = Some(Fingerprint::of_bytes(&unit.name, bytes, want));
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

/// 主库里那个文件在哪。键是「根名 + 相对那个根的路径」，分隔符是 `/`（ADR-0020）。
///
/// 认不出根名、或者盘上压根没有这条路径时，原样把拼出来的那条交回去：它开不了，于是
/// 这一条走的还是「读不到」那一支——与盘不在位是同一种处置，不必在这里多长一条岔路。
///
/// **键是 NFC 的，而盘上那个名字有 1.99% 是分解形式。** 在**分解敏感**的文件系统上
/// （Windows 的 NTFS、Linux 的 ext4，而 ADR-0018 说主力机正是 Windows），直接拼出来的
/// 那条路径根本开不了；识别把这个失败读成**无判据**，于是几百个变体从此认不出来，
/// 报告里说的却是「拿不到可撞的东西」。所以拼不出来的要折回盘上真实的那条
/// （[`Roots::real_path_in`]）。macOS 上看不见这条：fskit 的 NTFS 驱动查找不分解敏感。
///
/// **只有折得开的键才去折。** 折那一趟要先原样试一次（多一次 `open`），而识别一趟要回盘
/// 读几万个文件、每份本来就要 open 一次——让 98% 的路径替另外那 2% 多付一次系统调用不
/// 合算。键里一个字符都分解不开时（纯 ASCII、汉字、不带浊音符的假名都是这一档），盘上
/// 那个名字折成 NFC 既然等于这条键，就只可能与它逐字节相同，直接拼出来的那条一定对。
fn library_path(library: &dyn LibraryFs, roots: &Roots, dirs: &mut DirCache, key: &str) -> PathBuf {
    let direct = || roots.join(key).unwrap_or_else(|| PathBuf::from(key));
    if is_nfd_quick(key.chars()) == IsNormalized::Yes {
        return direct();
    }
    roots
        .real_path_in(library, dirs, key)
        .unwrap_or_else(direct)
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

/// 拿一串 SHA-1 撞一次，并记住撞上时用的是哪套口径。
fn lookup_sha1(
    repo: &DatRepo,
    sha1: [u8; 20],
    size: u64,
    hashed_as: Convention,
) -> Result<Vec<(Hit, Convention)>, RepoError> {
    repo.lookup_sha1(&fingerprint::hex(sha1), size)
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
        sha1: row.sha1.as_deref().and_then(fingerprint::from_hex),
        bare_sha1: row.bare_sha1.as_deref().and_then(fingerprint::from_hex),
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
        sha1: print.sha1.map(fingerprint::hex),
        bare_sha1: print.bare_sha1.map(fingerprint::hex),
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

/// 探一份内容的结果。`T` 是那一层自己的事实类型。
///
/// 两档分开，是因为它们的**保质期**完全不同：
///
/// - [`Self::Read`] 是关于这份内容的**结论**，只要文件没变就一直成立 → 落进中立库，
///   第二趟直接取回。
/// - [`Self::Missed`] 是「**这一趟**没读到」——盘不在位、容器解不开、
///   或者那 4,085 个在 macOS 上连 `stat` 都失败的文件（ADR-0021 的第三态）。
///   **它绝不落库**：缓存一次读失败等于让它永久生效，而那批文件在 Windows 上是正常的
///   （ADR-0018 定的工作方式正是两台机器轮流碰同一块盘）。
enum Probed<T> {
    /// 读到了，这是结论。
    Read(T),
    /// 这一趟没读到，理由在这儿。
    Missed(String),
}

/// 光盘那一层探完之后攒下来的东西。
struct Discovered {
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
) -> Result<Discovered, IdentifyError> {
    let mut probed = Discovered {
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
    let (cached, (mut facts, todo)) = plan_probe::<disc::Facts>(
        catalog,
        &wanted,
        options.read_library,
        &|catalog, member| catalog.disc_facts(member),
    )?;
    for (member, indexes) in todo {
        let path = library_path(library, &options.roots, &mut state.dirs, &member);
        // **壳子认不出来的一条都不读**——那不是光盘形态的东西。
        let (got, read) = fetch_prefixes(library, &path, &wanted, &indexes, &|it| {
            disc::by_name(&it.name).map(disc::probe_len)
        });
        for (at, prefix) in got {
            let it = &wanted[at];
            facts[at] = Some(match prefix {
                Prefix::Bytes(bytes) => Probed::Read(disc::probe(&it.name, &bytes, it.size)),
                // 排得太深是一条**结论**不是一次读失败（判据零解压可得），照样落库。
                Prefix::TooDeep(note) => Probed::Read(disc::Facts {
                    note: Some(note),
                    ..disc::Facts::default()
                }),
                Prefix::Missed(why) => Probed::Missed(why),
            });
        }
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
            Some(Probed::Missed(why)) => {
                state.missed += 1;
                if let Some(index) = it.unit
                    && units[index].print.is_none()
                    && units[index].blocked.is_none()
                {
                    units[index].blocked = Some(why);
                }
                continue;
            }
            Some(Probed::Read(found)) => found,
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
        let is_disc = found.ids.iter().any(|id| id.kind == ident::IdKind::DiscId);
        let nkit_clean = found.nkit != Some(true) && (!is_disc || found.nkit.is_some());
        for id in &found.ids {
            probed.evidence.push(Evidence {
                member: it.member.clone(),
                inner: it.inner.clone(),
                id: id.clone(),
                from_content: true,
                nkit_clean,
                platforms: Vec::new(),
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
            platforms: Vec::new(),
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

/// 卡带那一层探完之后攒下来的东西。
struct Carted {
    /// 收集到的游戏码。
    evidence: Vec<Evidence>,
    /// 这一趟为它读了多少字节。
    read_bytes: u64,
}

/// ⭐ **卡带内部头**：哈希撞不上时，从卡带自己的字节里读出「这是哪个游戏」。
///
/// 它与[光盘那一层](probe_discs)是同一条路——**只读几百字节、算过的不再算、这一趟
/// 没读到的不落库**——三处差别写在这儿：
///
/// 1. **判据是内部头不是光盘结构**（[`cart`]），要连**总长**一起给：SFC 的拷贝机头
///    没有魔数，唯一的判据是总长的余数。
/// 2. **撞上了就不必再探**。这一层没有 NKit 那样「CRC 会骗人所以照验」的例外——
///    卡带头认出来的是发行版，而精确哈希已经认得更细。
/// 3. **它顺带回答一个不撞库的问题**：内部头说的平台与目录声明的平台对不对得上
///    （ADR-0011 说那正是最该报告的产出之一）。答案落进 `content_cart.platform`，
///    报告一条 SQL 数得出来。
fn probe_carts(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &Options,
    variant: &VariantRow,
    units: &mut [ContentUnit],
    state: &mut Run,
) -> Result<Carted, IdentifyError> {
    let mut carted = Carted {
        evidence: Vec::new(),
        read_bytes: 0,
    };
    let hint = variant.platform.as_deref();
    // 一、要探哪几份。**精确命中过的不探**：第一层办成了的事不必重办。
    let mut wanted: Vec<Wanted> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if cart::by_name(&unit.name, hint).is_none()
            || unit.hits.iter().any(|(hit, _)| hit.is_exact())
        {
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
    if wanted.is_empty() {
        return Ok(carted);
    }

    // 二、算过的先取回来（不读盘），剩下的才回盘。
    let (cached, (mut facts, todo)) = plan_probe::<cart::Facts>(
        catalog,
        &wanted,
        options.read_library,
        &|catalog, member| catalog.cart_facts(member),
    )?;
    for (member, indexes) in todo {
        let path = library_path(library, &options.roots, &mut state.dirs, &member);
        let (got, read) = fetch_prefixes(library, &path, &wanted, &indexes, &|it| {
            cart::by_name(&it.name, hint).map(|kind| cart::probe_len(kind, it.size))
        });
        for (at, prefix) in got {
            let it = &wanted[at];
            facts[at] = Some(match prefix {
                Prefix::Bytes(bytes) => Probed::Read(cart::probe(&it.name, &bytes, it.size, hint)),
                Prefix::TooDeep(note) => Probed::Read(cart::Facts {
                    note: Some(note),
                    ..cart::Facts::default()
                }),
                Prefix::Missed(why) => Probed::Missed(why),
            });
        }
        carted.read_bytes += read;
        // 这一层读的字节也进总账（挂账 D14）。
        state.progress.read_bytes += read;
        state.progress.read_files += u64::try_from(indexes.len()).unwrap_or(0);
    }

    // 三、装回 units，落库，收集游戏码，数平台冲突。
    for (at, it) in wanted.iter().enumerate() {
        let found = match facts[at].take() {
            None => continue,
            // 这一趟没读到：**不落库**，也不覆盖第一命中层写在那儿的那句话。
            Some(Probed::Missed(why)) => {
                state.cart.missed += 1;
                if let Some(index) = it.unit
                    && units[index].print.is_none()
                    && units[index].blocked.is_none()
                {
                    units[index].blocked = Some(why);
                }
                continue;
            }
            Some(Probed::Read(found)) => found,
        };
        state.cart.probed += 1;
        if !found.ids.is_empty() {
            state.cart.with_id += 1;
        }
        // 这份头认哪几个平台。它有两个用处，而两个都不能拿「头说的那一个平台」顶替：
        //
        // 1. **撞库时圈住候选**：4 个字符的游戏码跨平台撞车是现实存在的（`A83J` 在
        //    SFC 与 GBA 各有一条）。判据取自内容自己，不取自目录（ADR-0011）。
        // 2. **判平台冲突**：GB 与 GBC 共用一份卡带头，一份 CGB 卡躺在 `gb/` 目录里
        //    不是冲突，是常态。
        let kind = found.cart.as_deref().and_then(cart::Cart::from_code);
        let platforms: Vec<String> = kind
            .map(|kind| {
                kind.platforms()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // **内部头与目录声明的平台冲突**（ADR-0011）。目录只是强先验，字节说了算。
        if let Some(declared) = hint
            && !platforms.is_empty()
            && !platforms.iter().any(|it| it == declared)
        {
            state.cart.conflicts += 1;
        }
        if cached
            .get(&it.member)
            .and_then(|rows| rows.get(&it.inner))
            .is_none()
            && let Ok(text) = serde_json::to_string(&found)
        {
            state.carts.push(CartFactRow {
                key: it.member.clone(),
                inner: it.inner.clone(),
                platform: found.platform.clone(),
                family: kind.map(|kind| format!(",{},", kind.platforms().join(","))),
                facts: text,
            });
        }
        for id in &found.ids {
            let mut id = id.clone();
            // **读出来却没人看得见的字段等于没读。** 归一化做了什么、头部校验和自洽
            // 与否、发行商与地区，全写进依据那一句——尤其是校验和：调研 D.3.3 的那条
            // 推断说「校验和自洽而整文件哈希撞不上任何 DAT」高度提示这是一份改过并
            // 正确修好了头的变体，也就是汉化版，而人在裁决队列里正需要这一句。
            id.from = enrich(&id.from, &found);
            carted.evidence.push(Evidence {
                member: it.member.clone(),
                inner: it.inner.clone(),
                id,
                from_content: true,
                nkit_clean: true,
                platforms: platforms.clone(),
            });
        }
        if let Some(index) = it.unit {
            if !found.ids.is_empty() {
                units[index].blocked = None;
            } else if let Some(note) = &found.note
                && units[index].print.is_none()
            {
                units[index].blocked = Some(note.clone());
            }
            units[index].cart = Some(found);
        }
    }
    Ok(carted)
}

/// Switch 那一层探完之后攒下来的东西。
struct Switched {
    /// 收集到的 TitleID，连它们是从哪一份内容上读的。
    evidence: Vec<Evidence>,
    /// 这一层产出的候选。**它不撞 DAT**——Switch 没有 DAT（No-Intro 的每日镜像里
    /// 334 份一个都没有），判据来自容器自己与 titledb。
    candidates: Vec<Candidate>,
    /// 这一趟为它读了多少字节。
    read_bytes: u64,
}

/// **透明容器**里那一条给这一层留多长的前缀。
///
/// PFS0 的头、条目表与字符串表加起来通常几百字节，64 KiB 宽出两个数量级。它挡的是
/// 「为了读一张文件名表把整个 5 GB 的条目解出来」——而 XCI 在容器里本来就够不着
/// （`secure` 分区的头真机实测落在文件的 368 MB 处），那时如实说够不着。
const SWITCH_PREFIX: usize = 64 << 10;

/// ⭐ **Switch 那一层**（票 27）：容器的**明文文件名表**里就写着「这是哪个游戏」。
///
/// 它与前两层（[光盘](probe_discs)、[卡带](probe_carts)）是同一条路——**只读几 KB、
/// 算过的不再算、这一趟没读到的不落库**——三处差别写在这儿：
///
/// 1. **它自己产出候选，不撞 DAT。** 前两层读出编号之后交给
///    [序列号那一层](serial::candidates)去撞 DAT 的序列号索引；而 Switch **没有 DAT**
///    ——调研把那个每日镜像的 334 份全查过，命中 0 个。判据来自容器自己
///    （`.tik` 的文件名）与第三方 TitleID 数据库（[`titledb`]）。
/// 2. **裸文件要 seek，不能只读前缀。** XCI 的入口在 `secure` 分区那张 HFS0 表上，
///    而它前面还压着整套系统更新——真机实测落在文件的 368 MB 处。**跳过去只读几 KB**，
///    但必须跳得动。容器里那一条跳不动，所以只给前缀，够不着就如实说。
/// 3. **精确命中过的照探。** 一份 `.nsp` 撞上 DAT 是不可能的事（那 334 份里没有
///    Switch），所以这里没有「第一层办成了就不办」那道闸——真撞上了，说明这个
///    `switch/` 目录下躺着的其实是别的平台的东西，而那时 `probe` 自己会认不出容器
///    并如实说。
fn probe_switch(
    library: &dyn LibraryFs,
    catalog: &mut Catalog,
    options: &Options,
    variant: &VariantRow,
    units: &mut [ContentUnit],
    ammo: &Ammo<'_>,
    state: &mut Run,
) -> Result<Switched, IdentifyError> {
    let mut switched = Switched {
        evidence: Vec::new(),
        candidates: Vec::new(),
        read_bytes: 0,
    };
    // 一、要探哪几份。判据是**名字**——那是最便宜的强先验；字节在 `probe` 里说了算。
    let mut wanted: Vec<Wanted> = Vec::new();
    for (index, unit) in units.iter().enumerate() {
        if !switch::by_name(&unit.name) {
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
    if wanted.is_empty() {
        return Ok(switched);
    }

    // 二、算过的先取回来（不读盘），剩下的才回盘。
    let (cached, (mut facts, todo)) = plan_probe::<switch::Facts>(
        catalog,
        &wanted,
        options.read_library,
        &|catalog, member| catalog.switch_facts(member),
    )?;
    for (member, indexes) in todo {
        let path = library_path(library, &options.roots, &mut state.dirs, &member);
        // **裸文件走 seek，容器里那一条只给前缀。** 两条路的差别只在取字节的办法上，
        // 解析器是同一份（`switch::Source` 的两个实现）。
        if wanted[indexes[0]].in_container {
            let (got, read) =
                fetch_prefixes(library, &path, &wanted, &indexes, &|_| Some(SWITCH_PREFIX));
            for (at, prefix) in got {
                let it = &wanted[at];
                facts[at] = Some(match prefix {
                    Prefix::Bytes(bytes) => {
                        Probed::Read(switch::probe(&it.name, &mut switch::Prefix(&bytes)))
                    }
                    Prefix::TooDeep(note) => Probed::Read(switch::Facts {
                        note: Some(note),
                        ..switch::Facts::default()
                    }),
                    Prefix::Missed(why) => Probed::Missed(why),
                });
            }
            switched.read_bytes += read;
            state.progress.read_bytes += read;
            state.progress.read_files += u64::try_from(indexes.len()).unwrap_or(0);
            continue;
        }
        for at in &indexes {
            let it = &wanted[*at];
            facts[*at] = Some(match library.open(&path) {
                Ok(mut handle) => {
                    let mut source = switch::Seeked::new(handle.as_mut(), switch::BUDGET);
                    let found = switch::probe(&it.name, &mut source);
                    let read = source.read();
                    switched.read_bytes += read;
                    // 这一层读的字节也进总账（挂账 D14）。
                    state.progress.read_bytes += read;
                    state.progress.read_files += 1;
                    Probed::Read(found)
                }
                Err(error) => Probed::Missed(format!("读不动：{error}")),
            });
        }
    }

    // 三、装回 units，落库，收集 TitleID。
    let mut probes: Vec<(usize, switch::Facts)> = Vec::new();
    for (at, it) in wanted.iter().enumerate() {
        let found = match facts[at].take() {
            None => continue,
            // 这一趟没读到：**不落库**（缓存一次读失败等于让它永久生效，ADR-0021），
            // 也不覆盖第一命中层写在那儿的那句话。
            Some(Probed::Missed(why)) => {
                state.missed += 1;
                if let Some(index) = it.unit
                    && units[index].print.is_none()
                    && units[index].blocked.is_none()
                {
                    units[index].blocked = Some(why);
                }
                continue;
            }
            Some(Probed::Read(found)) => found,
        };
        if cached
            .get(&it.member)
            .and_then(|rows| rows.get(&it.inner))
            .is_none()
            && let Ok(text) = serde_json::to_string(&found)
        {
            state.switches.push(SwitchFactRow {
                key: it.member.clone(),
                inner: it.inner.clone(),
                title_id: found.title_id().map(ToString::to_string),
                kind: found.kind.clone(),
                facts: text,
            });
        }
        for id in &found.ids {
            switched.evidence.push(Evidence {
                member: it.member.clone(),
                inner: it.inner.clone(),
                id: id.clone(),
                from_content: true,
                nkit_clean: true,
                // 这一层**不撞 DAT 的序列号索引**（Switch 没有 DAT），这一列用不上。
                platforms: Vec::new(),
            });
        }
        if let Some(index) = it.unit {
            // 读出东西了，第一命中层那句「DAT 库里 SWITCH 平台一条记录都没有」就过时了
            // ——留着它，报告会把一个已经认出来的变体记成「无判据」。
            if found.usable() {
                units[index].blocked = None;
            } else if let Some(note) = &found.note
                && units[index].print.is_none()
            {
                units[index].blocked = Some(note.clone());
            }
            probes.push((at, found.clone()));
            units[index].switch = Some(found);
        } else {
            probes.push((at, found));
        }
    }

    // 四、折候选。**文件名里那个 TitleID 一起交上去**：它不是主判据，用处是与容器里
    // 读出来的那个互相印证（对不上就是极强的「被改过」信号）。
    let named = file_name_of_key(&variant.key);
    let asks: Vec<switch::Probe<'_>> = probes
        .iter()
        .map(|(at, found)| switch::Probe {
            member: wanted[*at].member.as_str(),
            inner: wanted[*at].inner.as_str(),
            facts: found,
        })
        .collect();
    let found = switch::candidates(
        ammo.titledb,
        variant.platform.as_deref(),
        &asks,
        Some(named),
    )?;
    state.switch.merge(&found);
    switched.candidates = found.candidates;
    Ok(switched)
}

/// 把卡带头读出来的那几个字段接在「这一条是从哪儿读出来的」后面。
///
/// 它们不参与命中（命中靠编号），但**依据**要说得出来：事后复核的人靠这一句判断
/// 一条候选对不对，而裁决的人靠「校验和自洽而哈希撞不上」这个信号一眼认出汉化版。
fn enrich(from: &str, found: &cart::Facts) -> String {
    let mut text = from.to_string();
    if let Some(normalized) = &found.normalized {
        text.push_str(&format!("；解析前做了归一化：{normalized}"));
    }
    for (label, value) in [
        ("发行商", found.maker.as_deref()),
        ("地区", found.region.as_deref()),
    ] {
        if let Some(value) = value {
            text.push_str(&format!("；{label} {value}"));
        }
    }
    match found.checksum {
        Some(true) => text.push_str(
            "；**头部校验和自洽**——而整文件哈希撞不上任何 DAT，\
             那多半是一份改过内容、又把头修回去的变体（调研 D.3.3）",
        ),
        Some(false) => text.push_str("；头部校验和对不上，这份头被改过且没修回去"),
        None => {}
    }
    text
}

/// 一层探测排完计划之后手上有什么：每一份的事实（还没读的是 `None`），
/// 以及「哪个成员上还要读哪几条」。
type Planned<T> = (Vec<Option<Probed<T>>>, BTreeMap<String, Vec<usize>>);

/// 一层探测开工前的那两步：**把算过的取回来，排出还要读哪几份**。
///
/// 三层（光盘、卡带、Switch）逐字相同，所以只写一处：先按成员把中立库里存着的事实
/// 整批取回来（`facts_of` 说去哪张表取），再交给 [`restore_probed`]。
/// **一个字节都不读盘。** 顺带把取回来的那张表交出去——落库那一步要靠它判断
/// 「这一条是不是本来就在库里」，不然每一趟都会把同样的事实重写一遍。
type Cached = BTreeMap<String, BTreeMap<String, String>>;

/// 「去哪张表取算过的事实」：`content_disc` / `content_cart` / `content_switch`
/// 三张表的取法一模一样，差别只在表名，所以传一个取数的办法而不是三份代码。
type FactsOf<'a> = &'a dyn Fn(&Catalog, &str) -> Result<BTreeMap<String, String>, CatalogError>;

fn plan_probe<T: serde::de::DeserializeOwned>(
    catalog: &Catalog,
    wanted: &[Wanted],
    read_library: bool,
    facts_of: FactsOf<'_>,
) -> Result<(Cached, Planned<T>), CatalogError> {
    let mut cached: Cached = BTreeMap::new();
    for it in wanted {
        if !cached.contains_key(&it.member) {
            cached.insert(it.member.clone(), facts_of(catalog, &it.member)?);
        }
    }
    let planned = restore_probed::<T>(&cached, wanted, read_library);
    Ok((cached, planned))
}

/// 把中立库里**算过的那些事实**装回来，顺带排出「还要读哪几份」。
///
/// **一个字节都不读盘。** 三层探测共用它：各层的差别全在事实类型 `T` 上，
/// 而「算过的不再算」这件事一模一样（挂账 D14）。
fn restore_probed<T: serde::de::DeserializeOwned>(
    cached: &BTreeMap<String, BTreeMap<String, String>>,
    wanted: &[Wanted],
    read_library: bool,
) -> Planned<T> {
    let mut todo: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut facts: Vec<Option<Probed<T>>> = Vec::with_capacity(wanted.len());
    for (at, it) in wanted.iter().enumerate() {
        let stored = cached
            .get(&it.member)
            .and_then(|rows| rows.get(&it.inner))
            .and_then(|text| serde_json::from_str::<T>(text).ok())
            .map(Probed::Read);
        if stored.is_none() && read_library {
            todo.entry(it.member.clone()).or_default().push(at);
        }
        facts.push(stored);
    }
    (facts, todo)
}

/// 把同一个成员上要读的那几条读出来，交出每一条的前缀字节与这一趟读了多少。
///
/// `limit_of` 说这一份读多少；答 `None` 的一条都不读——那不是这一层认得的东西。
/// 光盘与卡带两层共用它：一个是壳子、一个是卡带头，「一趟打开、每条只要前面那一段」
/// 这件事一模一样。
fn fetch_prefixes(
    library: &dyn LibraryFs,
    path: &Path,
    wanted: &[Wanted],
    indexes: &[usize],
    limit_of: &dyn Fn(&Wanted) -> Option<usize>,
) -> (BTreeMap<usize, Prefix>, u64) {
    let asks: BTreeMap<usize, Ask> = indexes
        .iter()
        .filter_map(|at| {
            let it = &wanted[*at];
            limit_of(it).map(|limit| {
                (
                    *at,
                    Ask {
                        inner: it.inner.clone(),
                        limit,
                    },
                )
            })
        })
        .collect();
    let got = if wanted[indexes[0]].in_container {
        read_prefixes_from_container(library, path, &asks)
    } else {
        asks.iter()
            .map(|(at, ask)| (*at, read_prefix_bare(library, path, ask.limit)))
            .collect()
    };
    let read = got
        .values()
        .map(|prefix| match prefix {
            Prefix::Bytes(bytes) => bytes.len() as u64,
            _ => 0,
        })
        .sum();
    (got, read)
}

/// 要读某一条的前多少字节。
struct Ask {
    /// 容器内部路径；裸文件是空串。
    inner: String,
    /// 读到这么多就够了。
    limit: usize,
}

/// 一份内容的前若干字节要来了没有。
///
/// 三档而不是 `Result`，因为**它们的保质期不一样**：[`Self::Bytes`] 与 [`Self::TooDeep`]
/// 都是关于这份内容的**结论**（前者是字节，后者是「够到它太贵」这件事，判据零解压可得），
/// 只要文件没变就一直成立，落得了库；[`Self::Missed`] 是「这一趟没读到」——盘不在位、
/// 容器解不开——**绝不落库**（ADR-0021）。
enum Prefix {
    /// 读到了。
    Bytes(Vec<u8>),
    /// 这一趟没读到，理由在这儿。
    Missed(String),
    /// 在 solid 块里排得太深，为它解开前面几个 GB 不值。
    TooDeep(String),
}

/// 读一个裸文件的前若干字节。
fn read_prefix_bare(library: &dyn LibraryFs, path: &Path, limit: usize) -> Prefix {
    match library.read_head(path, limit) {
        Ok(head) => Prefix::Bytes(head),
        Err(error) => Prefix::Missed(format!("读不动：{error}")),
    }
}

/// 读容器里几条的前若干字节：一趟打开，每条只要前面那一段（[`Demand::Prefix`]）。
///
/// ⭐ **solid block 的代价要在开工前算清楚**（ADR-0014 1.6）。7z 的一个块里几个条目是
/// 连着压的，解压器**跳不过**中间的字节——想读排在后面那一条，前面那些就得先解出来
/// 扔掉。真机上撞见过一次：一份 7z 里那个 `.iso` 排在几 GB 的东西后面，
/// 「只读几百字节」当场变成解几个 GB，一个变体卡住整趟识别。
///
/// 判据零解压可得（`block` 与每条的未压缩大小都在容器头里），所以这里**在下计划之前**
/// 就把太贵的那些踢出去，并如实说为什么——而不是等它跑几分钟。
fn read_prefixes_from_container(
    library: &dyn LibraryFs,
    path: &Path,
    asks: &BTreeMap<usize, Ask>,
) -> BTreeMap<usize, Prefix> {
    let mut out: BTreeMap<usize, Prefix> = BTreeMap::new();
    if asks.is_empty() {
        return out;
    }
    let listing = match container::list(library, path) {
        Ok(listing) => listing,
        Err(error) => {
            for at in asks.keys() {
                out.insert(*at, Prefix::Missed(format!("容器读不动：{error}")));
            }
            return out;
        }
    };
    // 同一个 solid 块里排在这一条前面的、要先解出来扔掉的字节数。
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
    let mut wanted: BTreeMap<&str, u64> = BTreeMap::new();
    for (at, ask) in asks {
        let ahead = position
            .get(ask.inner.as_str())
            .and_then(|index| before.get(index))
            .copied()
            .unwrap_or(0);
        if ahead > MAX_SOLID_DRAIN {
            out.insert(
                *at,
                Prefix::TooDeep(format!(
                    "solid 块太深：这一条前面还压着 {}，要解开才够得到它，而这一层只想读几百字节",
                    crate::report::human_bytes(ahead)
                )),
            );
            continue;
        }
        wanted.insert(ask.inner.as_str(), ask.limit as u64);
    }
    if wanted.is_empty() {
        return out;
    }
    let plan = ReadPlan::new(&listing, |entry| match wanted.get(entry.path.as_str()) {
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
    for (at, ask) in asks {
        if out.contains_key(at) {
            continue;
        }
        match got.remove(&ask.inner) {
            Some(bytes) => {
                out.insert(*at, Prefix::Bytes(bytes));
            }
            None => {
                let why = match &outcome {
                    Err(error) => format!("解不开：{error}"),
                    Ok(_) => "容器里没有这一条".to_string(),
                };
                out.insert(*at, Prefix::Missed(why));
            }
        }
    }
    out
}

/// 把撞出来的东西折成候选、结论，以及作品与发行版。
/// **标识那两层**（光盘序列号与卡带内部头）交出来的东西。
///
/// 捆成一个类型而不是两个参数：两样同出一源、同去一处——`assemble` 拿候选去排序，
/// 拿标识去数「这个变体到底有没有判据」，少传一样就会让一份读出了序列号却撞不上 DAT
/// 的镜像被记成「无判据」。
struct FromIds {
    /// 读出来的标识，连它们是从哪一份内容上读的。
    evidence: Vec<Evidence>,
    /// 拿这些标识撞出来的候选，连 DAT 那条记录的父条目名。
    candidates: Vec<(Candidate, Option<String>)>,
}

fn assemble(
    catalog: &mut Catalog,
    variant: &VariantRow,
    units: &[ContentUnit],
    from_ids: &mut FromIds,
    read_bytes: u64,
    ammo: &Ammo<'_>,
    state: &mut Run,
) -> Result<Identification, CatalogError> {
    let naming = ammo.naming;
    let evidence = &from_ids.evidence;
    let from_serial = &mut from_ids.candidates;
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

    // ⭐ **文件名那一层**（票 11）：前面几层一条**自动通过**的候选都没有时才跑。
    //
    // 判据是「自动通过」而不是「有没有候选」：一份撞上了卡带游戏码的汉化版有候选，
    // 但那条候选只说得到发行版这一层（ADR-0008），中文名照样没有着落——而这一层
    // 正是为它准备的。它一个字节都不读盘，代价只有几十微秒的查表。
    if naming.ready() && !scored.iter().any(|(candidate, _)| candidate.accepted) {
        // 年份从**已有候选的条目名**里读：TOSEC 的第一个括号是发行日期，而
        // No-Intro 的名字里根本没有年份（`scrape::dat` 的那张对照表）。
        let year = naming::year_in(scored.iter().map(|(candidate, _)| candidate.game.as_str()));
        let names = names_of(variant, units, state);
        let found = fuzzy::candidates(naming, variant, &names, platform_of(variant, units), year);
        state.fuzzy.variants += 1;
        state.fuzzy.tried += found.tried;
        state.fuzzy.garbled += found.garbled;
        state.fuzzy.strong += found.strong;
        state.fuzzy.candidates += u64::try_from(found.candidates.len()).unwrap_or(0);
        for mark in found.unknown {
            *state.unknown_marks.entry(mark).or_insert(0) += 1;
        }
        if !found.candidates.is_empty() && scored.is_empty() {
            state.fuzzy.only += 1;
        }
        // 父条目名那一栏是 `None`：中文数据源没有 parent/clone 那套关系（ADR-0010 说的
        // 是 No-Intro 与 MAME 的），而这一层产出的候选**永不自动通过**，走不到用它
        // 立发行版那一步。
        scored.extend(
            found
                .candidates
                .into_iter()
                .map(|candidate| (candidate, None)),
        );
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

    // ⭐ **最后一档：模型推断兜底**（票 12）。
    //
    // 判据是「**一条候选都没有**」，比文件名那一层的「没有自动通过的候选」严一档——
    // 已经有东西可裁的变体不重复花钱。**结论与理由在上面已经算完了，这一层不动它们**：
    // 「rar 容器这一层还穿不透」那类理由是真事实，让一句猜测覆盖掉是净损失
    // （`model` 的模块文档说得更细）。
    let mut candidates = candidates;
    if candidates.is_empty() && state_of != State::Skipped && ammo.guessing.ready() {
        state.model.residue += 1;
        let question = model::Question {
            variant_key: variant.key.clone(),
            main_key: variant.main_key.clone(),
            platform: units
                .iter()
                .find_map(|unit| unit.cart.as_ref().and_then(|f| f.platform.clone())),
            declared: variant.platform.clone(),
            names: names_of(variant, units, state)
                .into_iter()
                .map(|named| (named.text, named.from))
                .collect(),
            inner: units
                .iter()
                .filter(|unit| !unit.inner.is_empty())
                .map(|unit| file_name_of_key(&unit.inner).to_string())
                .take(model::CONTEXT_LIMIT)
                .collect(),
            siblings: state.neighbours.around(&variant.key),
            // **措辞在 `model` 那一侧**：那几句是提示词的一部分，而提问指纹认的就是它们。
            facts: model::head_fields(
                units.iter().filter_map(|unit| unit.disc.as_ref()),
                units.iter().filter_map(|unit| unit.cart.as_ref()),
            ),
            bytes: variant.bytes,
        };
        let ask = question.ask(&ammo.guessing.model, &ammo.guessing.limits);
        match ammo.guessing.answers.get(&variant.key, &ask) {
            // 问过了：**这一趟一分钱都不花**，直接把候选重建出来。
            // 依据里印的是**当时真答话的那个模型**，不是这一趟命令行上写的那一个。
            Some((answered_by, answer)) => {
                state.model.from_cache += 1;
                candidates.extend(fold_answer(
                    &mut state.model,
                    variant,
                    &question,
                    answer,
                    answered_by,
                ));
            }
            // 没问过：排进队里，等主循环跑完一起打包问。
            None => state.model_pending.push(model::Asking { question, ask }),
        }
    }

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

/// 平台交叉校验拿哪一个平台去校。
///
/// **内容说的那个优先**：卡带内部头读出来的平台是从字节里来的，而目录只是强先验
/// （ADR-0011：目录声明必须能被文件内容推翻）。真库里 `psp/` 目录下混着整包的
/// FC / GB / SFC ROM——按目录判，它们的中文名会被整批判成「平台对不上」而一条不产出。
///
/// 头读不出来（没探过、不是卡带、光盘世代）才退回目录那一个。
fn platform_of<'a>(variant: &'a VariantRow, units: &'a [ContentUnit]) -> Option<&'a str> {
    units
        .iter()
        .find_map(|unit| {
            unit.cart
                .as_ref()
                .and_then(|facts| facts.platform.as_deref())
        })
        .or(variant.platform.as_deref())
}

/// 这个变体拿哪几个名字去撞文件名那一层。
///
/// 三处，各有各的理由（[`fuzzy`] 的模块文档说得更细）：
///
/// - **变体自己的名字**：多数时候中文名就写在这儿。
/// - **独占目录的名字**：`我的暑假[ACG汉化组]/ACG_Summer_Holiday.7z` 这种，中文名在
///   目录上。**只有独占目录才收**——一个装着三千个 zip 的目录，它的名字属于谁说不清。
/// - **容器里那个文件的名字**：容器里**只有一个**内容条目时才收。有好几个的时候，
///   哪一个代表这个变体是说不清的，而这一层错一条就是往队列里塞一条错的候选。
fn names_of(variant: &VariantRow, units: &[ContentUnit], state: &Run) -> Vec<fuzzy::Named> {
    let mut names = vec![fuzzy::Named::own(file_name_of_key(&variant.key))];
    if let Some(dir) = parent_dir(&variant.key)
        && state.exclusive_dirs.contains(dir)
    {
        names.push(fuzzy::Named::directory(file_name_of_key(dir)));
    }
    let mut inside = units.iter().filter(|unit| !unit.inner.is_empty());
    if let Some(unit) = inside.next()
        && inside.next().is_none()
    {
        names.push(fuzzy::Named::inside(file_name_of_key(&unit.inner)));
    }
    // 同一串字不撞两遍。
    let mut seen: BTreeSet<String> = BTreeSet::new();
    names.retain(|named| seen.insert(named.text.clone()));
    names
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
        // **titledb 紧跟在 DAT 后面**：它给的是 eShop 上的官方名字与官方语言表，
        // 整齐程度与 No-Intro 是一个量级；排在 DAT 之后只因为它不是一份转储数据库
        // （Switch 压根没有 DAT，票 27）。
        switch::SOURCE_TITLEDB => 5,
        // **容器自己那一层**排第三档：没取过 titledb 时它的名字只是一串 TitleID，
        // 干瘪；但那一串是**从字节里读出来的**，比下面那一层靠名字猜出来的硬。
        switch::SOURCE_CONTAINER => 6,
        // **中文离线源排在最后**：它的条目名是中文 wiki 的写法，而这一层
        // 一个字节都没看（`fuzzy` 的模块文档）。它照样排在「认不出的源」前面。
        fuzzy::SOURCE => 7,
        _ => 8,
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
        "{} 的《{}》里条目「{}」的文件「{}」，按{}哈希",
        hit.source,
        hit.dat,
        hit.game,
        hit.rom,
        hashed_as.label(),
    );
    match hit.matched_by {
        Matched::CrcAndSize => {
            evidence.push_str(&format!(
                "匹配 CRC-32 {:08X}",
                unit.print.map_or(0, |print| match hashed_as {
                    Convention::Headerless =>
                        print.headerless_pair().map_or(print.crc32, |pair| pair.1),
                    _ => print.crc32,
                })
            ));
            match hit.size {
                Some(size) => evidence.push_str(&format!(" 加大小 {}", thousands(size))),
                None => evidence.push_str("；这份 DAT 没记大小，只凭 CRC-32 撞上"),
            }
        }
        // **SHA-1 那条窄路**（票 10）：这批记录连 `crc32` 与 `size` 两列都是空的，
        // 第一命中层够不到它们。160 位对上就是对上了，不必再拿大小去补一刀。
        Matched::Sha1 => {
            let digest = match hashed_as {
                Convention::Headerless => unit.print.and_then(|print| print.headerless_sha1()),
                _ => unit.print.and_then(|print| print.sha1),
            };
            evidence.push_str(&format!(
                "匹配 SHA-1 {}；这份 DAT 只记 SHA-1（连 CRC-32 与大小都没有），\
                 第一命中层够不到它",
                digest.map(fingerprint::hex).unwrap_or_default()
            ));
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn 变体(key: &str, platform: Option<&str>) -> VariantRow {
        VariantRow {
            key: key.to_string(),
            platform: platform.map(ToString::to_string),
            rule: "同名成组".to_string(),
            main_key: key.to_string(),
            files: 1,
            bytes: 1,
            unreadable_files: 0,
            manual: false,
            work_id: None,
            release_id: None,
        }
    }

    fn 一份内容(member: &str, platform: Option<&str>) -> ContentUnit {
        ContentUnit {
            member: member.to_string(),
            inner: String::new(),
            name: member.to_string(),
            size: 1,
            print: None,
            blocked: None,
            hits: Vec::new(),
            in_container: false,
            disc: None,
            cart: platform.map(|platform| cart::Facts {
                platform: Some(platform.to_string()),
                ..cart::Facts::default()
            }),
            switch: None,
        }
    }

    #[test]
    fn 平台交叉校验先听内容的再听目录的() {
        // ADR-0011：目录声明必须能被文件内容推翻。真库里 `psp/` 目录下混着整包的
        // FC / GB / SFC ROM——按目录判，它们的中文名会被整批判成「平台对不上」。
        let variant = 变体("psp/整理包/超级马里奥.zip", Some("PSP"));
        let units = vec![一份内容("psp/整理包/超级马里奥.zip", Some("FC"))];
        assert_eq!(platform_of(&variant, &units), Some("FC"));
    }

    #[test]
    fn 内容说不出平台时才退回目录() {
        let variant = 变体("PSV/游戏.7z", Some("PSV"));
        let units = vec![一份内容("PSV/游戏.7z", None)];
        assert_eq!(platform_of(&variant, &units), Some("PSV"));
        // 连目录都说不出时就是说不出——那一道校验会记成「说不出」，不许当成「对得上」。
        assert_eq!(platform_of(&变体("游戏.7z", None), &units), None);
    }

    #[test]
    fn 独占目录才拿目录名去撞() {
        // 判据与刮削那一侧认本地媒体的规则同源：一个装着三千个 zip 的目录，
        // 它的名字属于谁根本说不清。
        let variants = vec![
            变体("FC/合集/一.zip", Some("FC")),
            变体("FC/合集/二.zip", Some("FC")),
            变体("FC/我的暑假[某汉化组]/game.zip", Some("FC")),
        ];
        let dirs = exclusive_dirs(&variants);
        assert!(dirs.contains("FC/我的暑假[某汉化组]"));
        assert!(!dirs.contains("FC/合集"));
    }
}
