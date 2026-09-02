//! **能力档案**：目标设备吃得下什么，目标存储放得下什么。
//!
//! `CONTEXT.md` 的词条把它定成「一个子库所用前端与模拟器核心能吃哪些容器与镜像格式的
//! 声明」，ADR-0017 的补充段又给它加了一半：**还必须涵盖目标存储的限制**。两半缺一不可
//! ——只管格式的话，一份 5 GiB 的 PS2 镜像会被判成「原样搬」，然后在 FAT32 的卡上
//! **传到一半失败**，卡上留下半份文件、清单里留下一条谎。
//!
//! ## 一份档案 = 一张平台矩阵 × 一个文件系统
//!
//! 分开写是因为它们真的各变各的：同一个 RetroArch，卡是 exFAT 还是 FAT32 完全是另一件
//! 事。内置那份 [`profiles.toml`](../../src/capability/profiles.toml) 把两个坐标各列一遍，
//! 再拼成几份具名档案（`retroarch-fat32`、`独立模拟器-exfat`……）。
//!
//! ## 三条纪律
//!
//! **一、没查过就写 `*`，不猜。** [`Accepts::Anything`] 是「不作声称」而不是「什么都能
//! 吃」：工具于是既不转也不报。ADR-0017 那句「**矩阵错误比不转换更糟**」说的正是相反的
//! 那一面——用户会以为工具已经处理妥当，直到在掌机上打不开才发现。宁可说「没查过」。
//!
//! **二、每条都带来源与核实日期。** [`Claim::cite`] 与 [`Claim::verified`]，报告原样印出
//! 来，超过 [`STALE_DAYS`] 天标成**陈旧**。矩阵会随模拟器更新而过时，这是让它过时得
//! **看得见**的唯一办法。
//!
//! **三、转不了就如实报出来。** 这一版转得了的只有**透明容器**这一层：
//! [`Recipe::Rezip`]（重打包成 zip）与 [`Recipe::Unpack`]（把容器里那一份原样取出来）。
//! **压缩镜像**之间的互转（cue+bin→chd、wbfs→rvz、zso→cso）要外部工具，做不到。
//! 做不到的落在 [`Decision::Unsupported`]，在**差量预览**里点名说出口。
//!
//! ## 判的是**主文件**，不是每一个成员
//!
//! **主文件**是「用来交给模拟器启动的那一个」（`CONTEXT.md`），能力矩阵描述的正是
//! 「模拟器启动得了什么」。拿它去判每一个成员的话，一个 PSV 目录树变体底下几千个
//! **内部资源**会各自领一条「吃不下」——报告当场变成噪音，而那几千个文件本来就不是
//! 拿去启动的。

pub mod filesystem;
pub mod report;

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;

use crate::container::Contents;
use crate::path::fold;

pub use filesystem::{Filesystem, RejectReason};

/// 内置的那一份。
const BUILTIN: &str = include_str!("capability/profiles.toml");

/// 本程序认得的配置版本。
pub const ROSTER_VERSION: u32 = 1;

/// ZIP 的经典头部里三个大小字段都是 32 位，越界要写 ZIP64，而这一版不写（挂账 D90）。
///
/// 判据与那句话都在这里一处说了算：[`decide`] 排计划时判一次（拿中立库里记着的未压缩
/// 大小），[`convert`](crate::convert) 真写产物时再判一次（拿盘上解出来的实际字节）。
/// 两处判的是**两个不同时刻的两个数**，但说出来的话必须是同一句。
pub const ZIP_LIMIT: u64 = u32::MAX as u64;

/// 越界时说的那句话。
pub const ZIP_LIMIT_SAYS: &str = "越过了 ZIP 的 4 GiB 边界，而这一版不写 ZIP64";

/// 核实日期比这个还老就标成**陈旧**。
///
/// 半年。模拟器一年发好几版，而这张表错了比不转换更糟（ADR-0017）——标出来至少让
/// 「该去核一遍了」变成看得见的事，而不是靠谁记得。
pub const STALE_DAYS: i64 = 180;

/// 没配档案的子库用哪一份。
///
/// **「不作声称」**：不转换、不检查，与票 20 的行为完全一致。默认必须是这个而不是某份
/// 真的矩阵——一份没人挑过的矩阵替用户做了决定，而它可能是错的。
pub const DEFAULT_PROFILE: &str = "不作声称";

// ── 声明的出处 ─────────────────────────────────────────────────────────────

/// 一条声明的出处：**从哪儿来的、哪天核实的**。
///
/// 它不是装饰。ADR-0017：矩阵错误比不转换更糟，因此「这条是谁说的」必须能当场翻出来。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Claim {
    /// 一手来源：源码文件与函数名、官方文档原话、本仓库里的调研段落号。
    pub cite: String,
    /// 核实日期，`YYYY-MM-DD`。
    pub verified: String,
}

impl Claim {
    /// 这条声明离 `today` 有多少天；日期读不懂时是 `None`。
    ///
    /// `today` 也写成 `YYYY-MM-DD`。**不去看系统时钟**：这一层是纯的，时钟由调用方给
    /// ——否则「报告里那句陈旧警告」就没法在测试里钉死。
    #[must_use]
    pub fn age_days(&self, today: &str) -> Option<i64> {
        Some(day_number(today)? - day_number(&self.verified)?)
    }

    /// 这条声明陈旧了吗。日期读不懂时算**陈旧**——读不懂就是核不了。
    #[must_use]
    pub fn is_stale(&self, today: &str) -> bool {
        self.age_days(today).is_none_or(|days| days > STALE_DAYS)
    }
}

/// 这一年这一月有几天。**日期得真的校验**：一条写着 `2026-02-30` 的核实日期
/// 是笔误，而笔误的核实日期比没有更糟——它看着像核过了。
fn days_in(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// `YYYY-MM-DD` 折成「儒略日式」的天数，用来相减。
///
/// 只为算两个日期差几天而存在，不必是真的儒略日——只要同一套公式算出来的两个数相减
/// 等于真实天数就够。公式是 civil-from-days 的逆（Howard Hinnant 的 `days_from_civil`）。
fn day_number(text: &str) -> Option<i64> {
    let mut parts = text.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: i64 = parts.next()?.parse().ok()?;
    let day: i64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || day < 1 || day > days_in(year, month)
    {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

// ── 平台能力 ───────────────────────────────────────────────────────────────

/// 这个平台吃得下哪些扩展名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Accepts {
    /// **不作声称**：没查过。既不转也不报。
    ///
    /// 与「什么都能吃」不是一回事，尽管行为看着一样——区别在报告里说得出口，
    /// 而说得出口正是这张表能被维护的前提。
    Anything,
    /// 就这些扩展名（小写、不带点）。
    Only(BTreeSet<String>),
}

impl Accepts {
    /// 吃得下这个扩展名吗。
    #[must_use]
    pub fn takes(&self, extension: &str) -> bool {
        match self {
            Self::Anything => true,
            Self::Only(set) => set.contains(extension),
        }
    }

    /// 不作声称吗。
    #[must_use]
    pub fn is_anything(&self) -> bool {
        matches!(self, Self::Anything)
    }
}

/// 吃不下时往哪儿转。**只有这一版真的做得到的那两种。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Recipe {
    /// 把**透明容器**重打包成 zip。
    ///
    /// ADR-0014 定的落点就是 zip 而不是 7z：兼容面最广，而且 zip 的中央目录存 CRC-32，
    /// 转换后仍然零解压识别得了。
    Rezip,
    /// 把**透明容器**里那一份原样取出来，成为**裸文件**。
    ///
    /// 光盘类模拟器一个都不吃归档，这条是它们唯一的路。
    Unpack,
}

impl Recipe {
    /// 配置里写的那个词，也是报告里印的那个。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Rezip => "zip",
            Self::Unpack => "裸文件",
        }
    }

    /// 从配置里那个词读回来。
    #[must_use]
    pub fn from_label(label: &str) -> Option<Self> {
        Some(match label {
            "zip" => Self::Rezip,
            "裸文件" => Self::Unpack,
            _ => return None,
        })
    }

    /// 这条配方每处理一字节**未压缩数据**要多久（秒）。见 [`estimate_secs`]。
    fn secs_per_byte(self) -> f64 {
        1.0 / (self.throughput_mib() * 1024.0 * 1024.0)
    }

    /// 粗估吞吐，按**未压缩数据量**算（MiB/s）。
    ///
    /// ## 为什么按未压缩量而不是按源文件大小
    ///
    /// 真机上栽过一次：按源文件（压缩后）大小估，54 MiB 的 7z 估出「1 秒」，实际跑了
    /// **3 分 17 秒**——因为那 54 MiB 解出来是 827 MiB，而解压器与压缩器要处理的是
    /// 后面那个数。压缩率越高，按源大小估就错得越离谱，而卡带包的压缩率恰恰很高。
    ///
    /// ## 这两个数从哪儿来
    ///
    /// **两条都是真机实测**（2026-09-02，release 构建，源是外置 NTFS 只读盘、
    /// 目标是本机 SSD；票 21 的验收记录里有原始数字）：
    ///
    /// - [`Rezip`](Self::Rezip)：24 个 SFC 卡带包重打包成 zip，未压缩 827.43 MiB，
    ///   用时 28 秒 → **29.6 MiB/s**。取 25 是留一点余量：宁可估多，别估少。
    /// - [`Unpack`](Self::Unpack)：26 个 PS1 容器解出裸文件，未压缩 575.85 MiB，
    ///   用时 7 秒 → **82.3 MiB/s**。取 80，同理。
    ///
    /// **它们会随机器、随源盘、随目标介质变**——SD 卡的写入速度就能把后一个数砍掉一半。
    /// 报告因此把预估**明说成粗估**，绝不印成一个像是算准了的时间。
    fn throughput_mib(self) -> f64 {
        match self {
            // 只解压 + 顺序写，没有压缩那一头。
            Self::Unpack => 80.0,
            // 解压 + 重新 deflate 压缩。压比解贵得多。
            Self::Rezip => 25.0,
        }
    }
}

/// 一条平台能力声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// 这一条管哪几个平台；`["*"]` 是兜底那一条。
    pub platforms: Vec<String>,
    /// 吃得下什么。
    pub accepts: Accepts,
    /// 吃不下时往哪儿转；没有就是转不了。
    pub convert_to: Option<Recipe>,
    /// 出处。
    pub claim: Claim,
}

/// 一张**平台矩阵**：某套前端与核心吃什么。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Matrix {
    /// 矩阵名。
    pub name: String,
    /// 一句话说这是什么。
    pub note: String,
    /// 逐条声明，按文件里的顺序。
    pub entries: Vec<Entry>,
    /// 平台名（已折成小写 NFC）→ 条目下标。
    by_platform: BTreeMap<String, usize>,
    /// 兜底那一条的下标。
    fallback: Option<usize>,
}

impl Matrix {
    /// 这个平台走哪一条声明。平台是 `None`（成型时没定出平台）时走兜底那条。
    #[must_use]
    pub fn entry_for(&self, platform: Option<&str>) -> Option<&Entry> {
        let index = platform
            .and_then(|name| self.by_platform.get(&fold(name)).copied())
            .or(self.fallback)?;
        self.entries.get(index)
    }
}

// ── 能力档案 ───────────────────────────────────────────────────────────────

/// 一份**能力档案**：一张平台矩阵 × 一个文件系统。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Profile {
    /// 档案名，子库靠它引用。
    pub name: String,
    /// 一句话说这是什么设备。
    pub note: String,
    /// 前端与核心吃什么。
    pub matrix: Matrix,
    /// 目标存储放得下什么。
    pub filesystem: Filesystem,
}

impl Profile {
    /// 什么都不声称的那一份：不转换、不检查。
    #[must_use]
    pub fn unclaimed() -> Self {
        Self {
            name: DEFAULT_PROFILE.to_string(),
            note: "不转换、不检查".to_string(),
            matrix: Matrix {
                name: DEFAULT_PROFILE.to_string(),
                note: "对任何平台都不作声称".to_string(),
                entries: Vec::new(),
                by_platform: BTreeMap::new(),
                fallback: None,
            },
            filesystem: Filesystem::unlimited(),
        }
    }

    /// 这份档案里有几条声明是陈旧的。
    #[must_use]
    pub fn stale_claims(&self, today: &str) -> usize {
        self.matrix
            .entries
            .iter()
            .filter(|entry| entry.claim.is_stale(today))
            .count()
            + usize::from(self.filesystem.claim.is_stale(today))
    }
}

// ── 判处置 ─────────────────────────────────────────────────────────────────

/// 一趟格式转换：读什么、写成什么。
///
/// **它只描述产物，不描述来源以外的任何主库改动**——转换只产生新文件，主库一个字节不改
/// （ADR-0004、ADR-0015）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conversion {
    /// 用哪条配方。
    pub recipe: Recipe,
    /// 产物落在子库里的哪条路径（相对子库根）。
    pub path: String,
    /// 产物多大。
    pub bytes: u64,
    /// 上面那个数是估的吗。
    ///
    /// [`Recipe::Unpack`] 不是：内部条目的未压缩大小零解压就在容器头里，是准数。
    /// [`Recipe::Rezip`] 是：重压之后多大要压完才知道，这里填的是**未压缩总量**，
    /// 也就是上界。
    pub estimated: bool,
    /// 要读多少源字节。耗时预估拿它算。
    pub source_bytes: u64,
    /// [`Recipe::Unpack`] 取容器里的第几条内部条目。
    pub inner: Option<usize>,
    /// 那条内部条目叫什么（进报告，也让 `--json` 看得懂）。
    pub inner_name: Option<String>,
}

/// 一个**主文件**在某份能力档案下的处置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Decision {
    /// 目标吃得下，原样搬。
    AsIs,
    /// 这份档案对这个平台**不作声称**。既不转也不报。
    Unclaimed,
    /// 要转，而且转得了。
    Convert(Box<Conversion>),
    /// 目标吃不下，而这一版转不了它——**如实报出来**。
    Unsupported {
        /// 目标想要的是什么形态。
        want: String,
        /// 为什么转不了。
        why: String,
    },
}

/// 判一个**主文件**该怎么办。
///
/// - `platform`：变体所属平台，成型没定出来时给 `None`。
/// - `key`：主文件在主库里的键（相对主库根，`/` 分隔、NFC）。
/// - `bytes`：主文件本身多大。
/// - `contents`：它作为**透明容器**时的内部构成；不是容器、或者当初穿不透时给 `None`。
///
/// 这是**纯函数**：不碰磁盘、不碰中立库。内部构成早在扫描那一趟零解压读进中立库了
/// （ADR-0014），于是「要不要转、转出来多大」在**差量预览**阶段就答得出来，不必先解一遍。
#[must_use]
pub fn decide(
    profile: &Profile,
    platform: Option<&str>,
    key: &str,
    bytes: u64,
    contents: Option<&Contents>,
) -> Decision {
    let Some(entry) = profile.matrix.entry_for(platform) else {
        return Decision::Unclaimed;
    };
    if entry.accepts.is_anything() {
        return Decision::Unclaimed;
    }
    let extension = extension_of(key);
    if entry.accepts.takes(&extension) {
        return Decision::AsIs;
    }
    let Some(recipe) = entry.convert_to else {
        return Decision::Unsupported {
            want: describe(&entry.accepts, None),
            why: format!("这个平台的档案没写转成什么，`.{extension}` 就只能原样躺在卡上"),
        };
    };
    let want = describe(&entry.accepts, Some(recipe));
    let Some(contents) = contents else {
        // **两种情况说两句不同的话。** 一句「不是透明容器」套在一个货真价实的 `.zip`
        // 上是自相矛盾的理由——那一份其实是当初**穿不透**（要密码、结构读不下去），
        // 而穿不透与「不是那个格式」在这个项目里从来是分开的两件事（`CONTEXT.md`）。
        let why = if crate::container::ContainerKind::for_path(Path::new(key)).is_some() {
            "这是**透明容器**，可中立库里没有它的内部构成：当初**穿不透**（要密码、\
             结构读不下去），或者那一趟压根没读它（zst 要 `romcat scan --zst`）。\
             内部构成读不出来，于是转不了——`romcat report` 说得出它卡在哪一类"
                .to_string()
        } else {
            format!(
                "`.{extension}` 不是这一版认得的**透明容器**（只认 zip、7z 与 zst），\
                 转换要外部工具，工具做不到"
            )
        };
        return Decision::Unsupported { want, why };
    };
    match recipe {
        Recipe::Rezip => rezip(contents, key, bytes, &want),
        Recipe::Unpack => unpack(contents, entry, key, bytes, &want),
    }
}

/// 重打包成 zip。
fn rezip(contents: &Contents, key: &str, bytes: u64, want: &str) -> Decision {
    let total: u64 = contents
        .entries
        .iter()
        .filter(|entry| entry.has_content())
        .map(|entry| entry.size)
        .sum();
    let count = contents
        .entries
        .iter()
        .filter(|entry| entry.has_content())
        .count();
    if count == 0 {
        return Decision::Unsupported {
            want: want.to_string(),
            why: "容器里一个有内容的条目都没有，重打包出来会是个空 zip".to_string(),
        };
    }
    // ZIP 的 32 位字段：单条或总量过 4 GiB 就要 ZIP64，而这一版不写（挂账 D90）。
    // **宁可说做不到，也不写一份读不回来的 zip。**
    if total >= ZIP_LIMIT || contents.entries.iter().any(|entry| entry.size >= ZIP_LIMIT) {
        return Decision::Unsupported {
            want: want.to_string(),
            why: format!("重打包出来会{ZIP_LIMIT_SAYS}"),
        };
    }
    if count > u16::MAX as usize {
        return Decision::Unsupported {
            want: want.to_string(),
            why: format!("容器里有 {count} 条，越过 ZIP 中央目录的 65535 条边界"),
        };
    }
    Decision::Convert(Box::new(Conversion {
        recipe: Recipe::Rezip,
        path: with_extension(key, "zip"),
        // 重压之后多大要压完才知道；填未压缩总量，是**上界**。
        bytes: total,
        estimated: true,
        source_bytes: bytes,
        inner: None,
        inner_name: None,
    }))
}

/// 把容器里那一份原样取出来。
fn unpack(contents: &Contents, entry: &Entry, key: &str, bytes: u64, want: &str) -> Decision {
    let mut inner = contents
        .entries
        .iter()
        .enumerate()
        .filter(|(_, item)| item.has_content());
    let Some((index, item)) = inner.next() else {
        return Decision::Unsupported {
            want: want.to_string(),
            why: "容器里一个有内容的条目都没有，解不出东西来".to_string(),
        };
    };
    // ⭐ **只解「只有一个内容条目」的容器。** 多条的解出来是好几个文件，而
    // 「哪一个才是主文件」是**成型**该答的问题，不是这里；何况 7z 的 solid 块上
    // 一条一趟会把代价放大约 100 倍（调研 1.6.2），而按块调度要重排整个执行循环。
    // 挂账 D91 记着这条路。
    if inner.next().is_some() {
        return Decision::Unsupported {
            want: want.to_string(),
            why: format!(
                "容器里有 {} 个内容条目，这一版只解只有一个的",
                contents.entries.iter().filter(|e| e.has_content()).count()
            ),
        };
    }
    let name = item.path.rsplit('/').next().unwrap_or(&item.path);
    if name.is_empty() {
        return Decision::Unsupported {
            want: want.to_string(),
            why: "容器里那一条没有名字，解出来不知道该叫什么".to_string(),
        };
    }
    // 解出来的那一份目标也吃不下，就别白解一趟：zip 里套着 rar 正是这种。
    let inner_extension = extension_of(name);
    if !entry.accepts.takes(&inner_extension) {
        return Decision::Unsupported {
            want: want.to_string(),
            why: format!("解出来是 `.{inner_extension}`，目标同样吃不下——解了也白解"),
        };
    }
    Decision::Convert(Box::new(Conversion {
        recipe: Recipe::Unpack,
        path: sibling(key, name),
        // 未压缩大小零解压就在容器头里，是**准数**（ADR-0014）。
        bytes: item.size,
        estimated: false,
        source_bytes: bytes,
        inner: Some(index),
        inner_name: Some(item.path.clone()),
    }))
}

/// 把一份声明折成一句「目标要的是什么」。
///
/// **配方的落点排在最前面**：一个平台可能认几十种扩展名，按字典序列前八个的话，
/// 用户最需要看见的那一个（「转成 zip 就能玩」）多半根本排不进来。
fn describe(accepts: &Accepts, convert_to: Option<Recipe>) -> String {
    let Accepts::Only(set) = accepts else {
        return "（不作声称）".to_string();
    };
    let mut head: Vec<&str> = Vec::new();
    if let Some(Recipe::Rezip) = convert_to {
        head.push(Recipe::Rezip.label());
    }
    for ext in set.iter().map(String::as_str) {
        if head.len() >= 6 {
            break;
        }
        if !head.contains(&ext) {
            head.push(ext);
        }
    }
    format!("{} 等 {} 种", head.join(" / "), set.len())
}

/// 一条键的扩展名，小写、不带点；没有扩展名时是空串。
#[must_use]
pub fn extension_of(key: &str) -> String {
    let name = key.rsplit('/').next().unwrap_or(key);
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default()
}

/// 换掉一条键的扩展名。
fn with_extension(key: &str, extension: &str) -> String {
    match key.rsplit_once('.') {
        // 点在最后一段之前（`a.b/c`）不算扩展名。
        Some((head, tail)) if !tail.contains('/') => format!("{head}.{extension}"),
        _ => format!("{key}.{extension}"),
    }
}

/// 同目录下换一个名字。
fn sibling(key: &str, name: &str) -> String {
    match key.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{name}"),
        None => name.to_string(),
    }
}

/// 今天是哪天，`YYYY-MM-DD`（UTC）。
///
/// [`Claim::age_days`] 刻意不看时钟——那样测试才钉得死。**读时钟的那一半放在这里**，
/// 而不是让每个调用方各抄一遍公历换算：抄两遍的两份公式迟早会在闰年上分家，
/// 而它们算的本来就是同一件事。
///
/// 公式是 [`day_number`] 的逆（Howard Hinnant 的 `civil_from_days`）。
#[must_use]
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    from_day_number(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// 把 [`day_number`] 那个天数折回 `YYYY-MM-DD`。
fn from_day_number(days: i64) -> String {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// 一趟转换的**粗估**耗时（秒）。
///
/// `uncompressed_bytes` 是**未压缩的数据量**（[`Conversion::bytes`]），不是源文件多大
/// ——解压器与压缩器要处理的是前者。按源文件估的话，一个压缩率高的卡带包会估出
/// 「1 秒」然后真跑三分钟（见 [`Recipe::throughput_mib`]）。
///
/// 拿实测吞吐乘一乘而已，**不是一个算准了的数**：源盘是外置 NTFS、目标是 SD 卡、
/// 中间还夹着解压与重压，三样都能把它带偏一倍。报告因此必须把「粗估」两个字印出来
/// ——一个看着精确的错数比没有数更糟。
#[must_use]
pub fn estimate_secs(recipe: Recipe, uncompressed_bytes: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    let bytes = uncompressed_bytes as f64;
    bytes * recipe.secs_per_byte()
}

// ── 一整份名册 ─────────────────────────────────────────────────────────────

/// 内置的与用户改过的那一整份档案表。
#[derive(Debug, Clone)]
pub struct Roster {
    profiles: Vec<Profile>,
    matrices: Vec<Matrix>,
    filesystems: Vec<Filesystem>,
}

/// 名册读不进来。
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    /// 文件打不开。
    #[error("能力档案读不出来：{path}（{source}）")]
    Io {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// TOML 解析失败。
    #[error("能力档案 {path} 解析失败：{source}")]
    Parse {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: Box<toml::de::Error>,
    },
    /// 版本对不上。
    #[error("能力档案 {path} 的版本是 {found}，本程序认得的是 {expected}")]
    Version {
        /// 出问题的文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
    /// 内容自相矛盾。
    #[error("能力档案 {path} 有问题：{message}")]
    Invalid {
        /// 出问题的文件。
        path: String,
        /// 说明。
        message: String,
    },
}

impl Roster {
    /// 内置的那一份。
    ///
    /// # Panics
    /// 内置名册读不出来说明这次构建本身是坏的，直接 panic 好过让工具带着半份矩阵跑。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse(BUILTIN, "（内置）").expect("内置能力档案必须是好的")
    }

    /// 内置名册的原文。
    ///
    /// 「矩阵会过时、维护者要改得动」这句话要落地，用户得先拿到一份能照着改的底稿
    /// ——`romcat capability --dump-builtin` 写的就是它。
    #[must_use]
    pub fn builtin_text() -> &'static str {
        BUILTIN
    }

    /// 从磁盘上读一份。
    ///
    /// # Errors
    /// 文件打不开、解析失败、版本对不上、引用了不存在的矩阵或文件系统时返回错误。
    pub fn load(path: &Path) -> Result<Self, CapabilityError> {
        let text = std::fs::read_to_string(path).map_err(|source| CapabilityError::Io {
            path: crate::path::display(path),
            source,
        })?;
        Self::parse(&text, &crate::path::display(path))
    }

    /// 工作目录里那份可选的名册叫什么。
    pub const IN_WORKSPACE: &'static str = "capability.toml";

    /// 先看工作目录里有没有一份，没有就用内置的。
    ///
    /// # Errors
    /// 工作目录里那份读不出来时返回错误——**不静默退回内置的**：用户放了一份在那儿
    /// 就是要用它，退回去等于拿另一张矩阵替他做了决定。
    pub fn in_workspace(workspace: &Path) -> Result<Self, CapabilityError> {
        let candidate = workspace.join(Self::IN_WORKSPACE);
        if candidate.exists() {
            Self::load(&candidate)
        } else {
            Ok(Self::builtin())
        }
    }

    /// 从一份 TOML 文本读出名册。`path` 只用于报错。
    ///
    /// # Errors
    /// 同 [`load`](Self::load)。
    pub fn parse(text: &str, path: &str) -> Result<Self, CapabilityError> {
        let raw: RawRoster = toml::from_str(text).map_err(|source| CapabilityError::Parse {
            path: path.to_string(),
            source: Box::new(source),
        })?;
        if raw.version != ROSTER_VERSION {
            return Err(CapabilityError::Version {
                path: path.to_string(),
                found: raw.version,
                expected: ROSTER_VERSION,
            });
        }
        let invalid = |message: String| CapabilityError::Invalid {
            path: path.to_string(),
            message,
        };

        // ── 扩展名组：只是给 `"吃"` 列表省字。
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for group in raw.groups {
            let folded: Vec<String> = group
                .extensions
                .iter()
                .map(|ext| ext.to_lowercase())
                .collect();
            if groups.insert(group.name.clone(), folded).is_some() {
                return Err(invalid(format!("扩展名组「{}」出现了两次", group.name)));
            }
        }

        // ── 文件系统。
        let mut filesystems: Vec<Filesystem> = Vec::new();
        for raw_fs in raw.filesystems {
            if filesystems.iter().any(|fs| fs.name == raw_fs.name) {
                return Err(invalid(format!("文件系统「{}」出现了两次", raw_fs.name)));
            }
            filesystems.push(Filesystem {
                name: raw_fs.name,
                note: raw_fs.note,
                max_file_bytes: nonzero(raw_fs.max_file_bytes),
                max_name_chars: nonzero(raw_fs.max_name_chars)
                    .and_then(|n| usize::try_from(n).ok()),
                max_path_chars: nonzero(raw_fs.max_path_chars)
                    .and_then(|n| usize::try_from(n).ok()),
                forbidden: raw_fs.forbidden.chars().collect(),
                reserved_stems: raw_fs
                    .reserved
                    .iter()
                    .map(|name| name.to_uppercase())
                    .collect(),
                claim: Claim {
                    cite: raw_fs.cite,
                    verified: raw_fs.verified,
                },
            });
        }

        // ── 平台矩阵。
        let mut matrices: Vec<Matrix> = Vec::new();
        for raw_matrix in raw.matrices {
            if matrices.iter().any(|m| m.name == raw_matrix.name) {
                return Err(invalid(format!(
                    "平台矩阵「{}」出现了两次",
                    raw_matrix.name
                )));
            }
            let mut entries = Vec::with_capacity(raw_matrix.entries.len());
            let mut by_platform: BTreeMap<String, usize> = BTreeMap::new();
            let mut fallback = None;
            for raw_entry in raw_matrix.entries {
                let index = entries.len();
                let mut accepts_set = BTreeSet::new();
                let mut anything = false;
                for token in &raw_entry.accepts {
                    if token == "*" {
                        anything = true;
                    } else if let Some(group) = token.strip_prefix('@') {
                        let members = groups.get(group).ok_or_else(|| {
                            invalid(format!(
                                "平台矩阵「{}」引用了不存在的扩展名组「{group}」",
                                raw_matrix.name
                            ))
                        })?;
                        accepts_set.extend(members.iter().cloned());
                    } else {
                        accepts_set.insert(token.to_lowercase());
                    }
                }
                if anything && !accepts_set.is_empty() {
                    return Err(invalid(format!(
                        "平台矩阵「{}」有一条同时写了 `*` 与具体扩展名。\
                         `*` 是**不作声称**，与「吃这几样」是两件事，混在一起说不清",
                        raw_matrix.name
                    )));
                }
                let convert_to = match &raw_entry.convert_to {
                    None => None,
                    Some(label) => Some(Recipe::from_label(label).ok_or_else(|| {
                        invalid(format!(
                            "平台矩阵「{}」里「转成 = {label}」不是这一版做得到的配方。\
                             只有 \"zip\" 与 \"裸文件\"",
                            raw_matrix.name
                        ))
                    })?),
                };
                for platform in &raw_entry.platforms {
                    if platform == "*" {
                        if fallback.is_some() {
                            return Err(invalid(format!(
                                "平台矩阵「{}」有不止一条兜底（`*`）",
                                raw_matrix.name
                            )));
                        }
                        fallback = Some(index);
                    } else if by_platform.insert(fold(platform), index).is_some() {
                        return Err(invalid(format!(
                            "平台矩阵「{}」里平台「{platform}」出现了两次",
                            raw_matrix.name
                        )));
                    }
                }
                entries.push(Entry {
                    platforms: raw_entry.platforms,
                    accepts: if anything {
                        Accepts::Anything
                    } else {
                        Accepts::Only(accepts_set)
                    },
                    convert_to,
                    claim: Claim {
                        cite: raw_entry.cite,
                        verified: raw_entry.verified,
                    },
                });
            }
            matrices.push(Matrix {
                name: raw_matrix.name,
                note: raw_matrix.note,
                entries,
                by_platform,
                fallback,
            });
        }

        // ── 拼成档案。
        let mut profiles = Vec::with_capacity(raw.profiles.len());
        for raw_profile in raw.profiles {
            if profiles
                .iter()
                .any(|p: &Profile| p.name == raw_profile.name)
            {
                return Err(invalid(format!(
                    "能力档案「{}」出现了两次",
                    raw_profile.name
                )));
            }
            let matrix = matrices
                .iter()
                .find(|m| m.name == raw_profile.matrix)
                .ok_or_else(|| {
                    invalid(format!(
                        "能力档案「{}」引用了不存在的平台矩阵「{}」",
                        raw_profile.name, raw_profile.matrix
                    ))
                })?
                .clone();
            let filesystem = filesystems
                .iter()
                .find(|fs| fs.name == raw_profile.filesystem)
                .ok_or_else(|| {
                    invalid(format!(
                        "能力档案「{}」引用了不存在的文件系统「{}」",
                        raw_profile.name, raw_profile.filesystem
                    ))
                })?
                .clone();
            profiles.push(Profile {
                name: raw_profile.name,
                note: raw_profile.note,
                matrix,
                filesystem,
            });
        }
        if !profiles.iter().any(|p| p.name == DEFAULT_PROFILE) {
            return Err(invalid(format!(
                "这份名册里没有叫「{DEFAULT_PROFILE}」的档案。\
                 没配档案的子库走的就是它，缺了它整批子库当场没有默认可走"
            )));
        }
        Ok(Self {
            profiles,
            matrices,
            filesystems,
        })
    }

    /// 按名字取一份档案。
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name == name)
    }

    /// 按名字取，取不到就退回**不作声称**那一份。
    ///
    /// 子库里记着的名字在这份名册里没有（用户换过名册、或者改名了）时走这条。
    /// **退回的是「不作声称」而不是随便一份**：拿另一张矩阵替用户做决定，正是
    /// ADR-0017 那句「矩阵错误比不转换更糟」说的那种错。
    #[must_use]
    pub fn find_or_unclaimed(&self, name: Option<&str>) -> Profile {
        name.and_then(|name| self.find(name))
            .or_else(|| self.find(DEFAULT_PROFILE))
            .cloned()
            .unwrap_or_else(Profile::unclaimed)
    }

    /// 全部档案。
    #[must_use]
    pub fn profiles(&self) -> &[Profile] {
        &self.profiles
    }

    /// 全部平台矩阵。
    #[must_use]
    pub fn matrices(&self) -> &[Matrix] {
        &self.matrices
    }

    /// 全部文件系统。
    #[must_use]
    pub fn filesystems(&self) -> &[Filesystem] {
        &self.filesystems
    }

    /// 档案名，按文件里的顺序。
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.profiles.iter().map(|p| p.name.as_str()).collect()
    }
}

/// 配置里 0 表示不设限。
fn nonzero(value: u64) -> Option<u64> {
    (value > 0).then_some(value)
}

// ── TOML 里的形状 ──────────────────────────────────────────────────────────
//
// 与上面公开的形状分开，理由与 `platform` 那一侧一样：配置里的字段是中文名、可省略，
// 而程序里用的是校验过、索引建好的形态。混成一个类型的话，「这个 `Vec` 校验过没有」
// 会变成读代码才知道的事。

#[derive(Debug, serde::Deserialize)]
struct RawRoster {
    #[serde(rename = "版本")]
    version: u32,
    #[serde(rename = "扩展名组", default)]
    groups: Vec<RawGroup>,
    #[serde(rename = "文件系统", default)]
    filesystems: Vec<RawFilesystem>,
    #[serde(rename = "平台矩阵", default)]
    matrices: Vec<RawMatrix>,
    #[serde(rename = "能力档案", default)]
    profiles: Vec<RawProfile>,
}

#[derive(Debug, serde::Deserialize)]
struct RawGroup {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "扩展名")]
    extensions: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct RawFilesystem {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "说明", default)]
    note: String,
    #[serde(rename = "单文件上限", default)]
    max_file_bytes: u64,
    #[serde(rename = "文件名字符上限", default)]
    max_name_chars: u64,
    #[serde(rename = "路径字符上限", default)]
    max_path_chars: u64,
    #[serde(rename = "文件名禁用字符", default)]
    forbidden: String,
    #[serde(rename = "保留名", default)]
    reserved: Vec<String>,
    #[serde(rename = "来源")]
    cite: String,
    #[serde(rename = "核实日期")]
    verified: String,
}

#[derive(Debug, serde::Deserialize)]
struct RawMatrix {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "说明", default)]
    note: String,
    #[serde(rename = "条目", default)]
    entries: Vec<RawEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct RawEntry {
    #[serde(rename = "平台")]
    platforms: Vec<String>,
    #[serde(rename = "吃")]
    accepts: Vec<String>,
    #[serde(rename = "转成")]
    convert_to: Option<String>,
    #[serde(rename = "来源")]
    cite: String,
    #[serde(rename = "核实日期")]
    verified: String,
}

#[derive(Debug, serde::Deserialize)]
struct RawProfile {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "说明", default)]
    note: String,
    #[serde(rename = "平台矩阵")]
    matrix: String,
    #[serde(rename = "文件系统")]
    filesystem: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::InnerEntry;

    /// 今天。测试里钉死，于是「陈旧」那条断言不会某天自己变红。
    const 今天: &str = "2026-09-02";

    fn 内容(entries: &[(&str, u64)]) -> Contents {
        Contents {
            entries: entries
                .iter()
                .enumerate()
                .map(|(index, (path, size))| InnerEntry {
                    path: (*path).to_string(),
                    size: *size,
                    crc32: Some(1),
                    is_dir: false,
                    block: Some(index),
                    name_lossy: false,
                })
                .collect(),
            blocks: entries.len(),
        }
    }

    fn 档案(name: &str) -> Profile {
        Roster::builtin()
            .find(name)
            .unwrap_or_else(|| panic!("内置名册里该有「{name}」"))
            .clone()
    }

    #[test]
    fn 内置名册读得出来而且每条声明都带来源与核实日期() {
        let roster = Roster::builtin();
        assert!(roster.find(DEFAULT_PROFILE).is_some(), "默认那一份必须在");
        assert!(roster.profiles().len() >= 4, "内置常见前端与核心的档案");
        for matrix in roster.matrices() {
            for entry in &matrix.entries {
                assert!(
                    !entry.claim.cite.trim().is_empty(),
                    "矩阵「{}」有一条没写来源——矩阵错误比不转换更糟（ADR-0017）",
                    matrix.name,
                );
                assert!(
                    entry.claim.age_days(今天).is_some(),
                    "矩阵「{}」里「{}」的核实日期读不懂：{}",
                    matrix.name,
                    entry.platforms.join("、"),
                    entry.claim.verified,
                );
            }
        }
        for filesystem in roster.filesystems() {
            assert!(!filesystem.claim.cite.trim().is_empty());
            assert!(filesystem.claim.age_days(今天).is_some());
        }
    }

    #[test]
    fn 内置的每份档案眼下都不陈旧() {
        // 这条会随时间自己变红——**那正是它存在的意义**：矩阵会随模拟器更新而过时
        // （ADR-0017），到期就该有人去源码里核一遍，而不是让它悄悄烂在那儿。
        for profile in Roster::builtin().profiles() {
            assert_eq!(
                profile.stale_claims(今天),
                0,
                "档案「{}」里有声明超过 {STALE_DAYS} 天没核实了",
                profile.name,
            );
        }
    }

    #[test]
    fn 没挑过档案就什么都不作声称_不转也不检查() {
        let profile = 档案(DEFAULT_PROFILE);
        assert_eq!(
            decide(&profile, Some("SFC"), "SFC/一.rar", 1024, None),
            Decision::Unclaimed,
        );
        assert!(
            profile
                .filesystem
                .screen("SFC/一.rar", u64::MAX, 0)
                .is_none(),
            "不作声称就一条都不检查",
        );
    }

    #[test]
    fn 星号是不作声称而不是什么都能吃() {
        // RetroArch 那份档案对 PSV **没查过**，于是既不转也不报——宁可说「没查过」，
        // 也不拿一条猜出来的结论替用户做决定。
        let profile = 档案("retroarch-exfat");
        assert_eq!(
            decide(&profile, Some("PSV"), "PSV/一.tar.zst", 1024, None),
            Decision::Unclaimed,
        );
    }

    #[test]
    fn retroarch_吃得下_7z_而独立的_snes9x_只吃_zip() {
        // ⭐ 源码级结论：RetroArch 的 `file_archive_get_file_backend()` 认 7z；
        // 而 Snes9x 与 ares 只有 zip。同一个文件，两份档案两种答案。
        let 包 = 内容(&[("魂斗罗.sfc", 1_048_576)]);
        assert_eq!(
            decide(
                &档案("retroarch-exfat"),
                Some("SFC"),
                "SFC/魂斗罗.7z",
                600_000,
                Some(&包)
            ),
            Decision::AsIs,
        );
        let Decision::Convert(conversion) = decide(
            &档案("独立模拟器-exfat"),
            Some("SFC"),
            "SFC/魂斗罗.7z",
            600_000,
            Some(&包),
        ) else {
            panic!("Snes9x 只吃 zip，这一份该被判成要转");
        };
        assert_eq!(conversion.recipe, Recipe::Rezip);
        assert_eq!(conversion.path, "SFC/魂斗罗.zip");
        assert_eq!(conversion.source_bytes, 600_000);
        assert!(conversion.estimated, "重压之后多大要压完才知道");
    }

    #[test]
    fn 光盘类不吃归档_解出那一份镜像来() {
        let 包 = 内容(&[("最终幻想7.chd", 700_000_000)]);
        let Decision::Convert(conversion) = decide(
            &档案("独立模拟器-exfat"),
            Some("PS1"),
            "PS1/最终幻想7.zip",
            690_000_000,
            Some(&包),
        ) else {
            panic!("DuckStation 一个归档都不吃");
        };
        assert_eq!(conversion.recipe, Recipe::Unpack);
        assert_eq!(conversion.path, "PS1/最终幻想7.chd");
        assert_eq!(conversion.inner, Some(0));
        // 未压缩大小零解压就在容器头里，是**准数**——容量那一栏因此不是估的。
        assert_eq!(conversion.bytes, 700_000_000);
        assert!(!conversion.estimated);
    }

    #[test]
    fn rar_几乎无人支持_而这一版转不了它_于是如实报出来() {
        // `.rar` 不是这一版认得的透明容器（ADR-0014：自行实现头部解析器避开 UnRAR
        // 许可，而那只读头不解压）。**假装搞定了比不转换更糟**。
        let Decision::Unsupported { why, .. } = decide(
            &档案("retroarch-exfat"),
            Some("SFC"),
            "SFC/魂斗罗.rar",
            600_000,
            None,
        ) else {
            panic!("RetroArch 不支持 rar，而工具转不了它");
        };
        assert!(why.contains("透明容器"), "要说清为什么转不了：{why}");
    }

    #[test]
    fn 解出来目标同样吃不下就别白解一趟() {
        // zip 里套着一个 rar：解出来还是 rar，目标照样打不开。
        let 包 = 内容(&[("魂斗罗.rar", 600_000)]);
        let Decision::Unsupported { why, .. } = decide(
            &档案("独立模拟器-exfat"),
            Some("PS1"),
            "PS1/怪.zip",
            500_000,
            Some(&包),
        ) else {
            panic!("解出来是 rar，解了也白解");
        };
        assert!(why.contains("白解"), "{why}");
    }

    #[test]
    fn 多个内容条目的容器这一版不解_而且说清为什么() {
        let 包 = 内容(&[("盘.cue", 100), ("盘.bin", 700_000_000)]);
        let Decision::Unsupported { why, .. } = decide(
            &档案("独立模拟器-exfat"),
            Some("PS1"),
            "PS1/双轨.zip",
            600_000_000,
            Some(&包),
        ) else {
            panic!("多条的这一版不解");
        };
        assert!(why.contains("只解只有一个的"), "{why}");
    }

    #[test]
    fn es_de_看不见_nsz_与_xcz() {
        // ⭐ 库里 55 个这样的文件、74.67 GiB，占 Switch 容量的 33.4%，
        // 在 ES-DE 里默认完全看不见（switch-identification.md §6.2）。
        let profile = 档案("es-de-exfat");
        let Decision::Unsupported { want, .. } = decide(
            &profile,
            Some("SWITCH"),
            "SWITCH/某作.nsz",
            1_100_000_000,
            None,
        ) else {
            panic!("ES-DE 的 switch 扩展名表里没有 nsz");
        };
        assert!(want.contains("nsp"), "目标要的是 nsp / xci：{want}");
        assert_eq!(
            decide(
                &profile,
                Some("SWITCH"),
                "SWITCH/某作.nsp",
                1_100_000_000,
                None
            ),
            Decision::AsIs,
        );
    }

    #[test]
    fn vita3k_只认_zip_vpk_vci_于是那批_tar_zst_一个都装不了() {
        let Decision::Unsupported { want, .. } = decide(
            &档案("独立模拟器-exfat"),
            Some("PSV"),
            "PSV/某作.tar.zst",
            3_000_000_000,
            None,
        ) else {
            panic!("Vita3K 只认 .zip / .vpk / .vci");
        };
        assert!(want.contains("vpk"), "{want}");
    }

    #[test]
    fn ppsspp_不吃_zso_而_pcsx2_吃() {
        // ⭐ 与直觉相反，源码级结论：PPSSPP 的 `hdr.ver > 1` 直接报错。
        let profile = 档案("独立模拟器-exfat");
        assert!(matches!(
            decide(&profile, Some("PSP"), "PSP/某作.zso", 900_000_000, None),
            Decision::Unsupported { .. }
        ));
        assert_eq!(
            decide(&profile, Some("PS2"), "PS2/某作.zso", 4_000_000_000, None),
            Decision::AsIs,
        );
    }

    // ── 目标存储那一半 ────────────────────────────────────────────────────

    #[test]
    fn fat32_的_4_gib_单文件上限在预览阶段就拦得下() {
        let fat32 = 档案("retroarch-fat32").filesystem;
        assert_eq!(fat32.max_file_bytes, Some(4_294_967_295));
        // 一份 4.5 GiB 的 PS2 镜像：ADR-0017 补充段点名的正是这一种。
        let (reason, detail) = fat32
            .screen("PS2/某作.iso", 4_831_838_208, 0)
            .expect("放不进去");
        assert_eq!(reason, RejectReason::TooBig);
        assert!(detail.contains("FAT32"), "{detail}");
        // 差一个字节就装得下：边界不是拍出来的。
        assert!(fat32.screen("PS2/某作.iso", 4_294_967_295, 0).is_none());
        assert!(fat32.screen("PS2/某作.iso", 4_294_967_296, 0).is_some());
        // 同一份内容在 exFAT 上装得下——**这就是能力档案要分两个坐标的理由**。
        assert!(
            档案("retroarch-exfat")
                .filesystem
                .screen("PS2/某作.iso", 4_831_838_208, 0)
                .is_none()
        );
    }

    #[test]
    fn 文件名的字符限制与保留名也查() {
        let exfat = 档案("retroarch-exfat").filesystem;
        let (reason, _) = exfat.screen("SFC/魂斗罗?.zip", 1024, 0).expect("问号不收");
        assert_eq!(reason, RejectReason::BadName);
        let (reason, _) = exfat.screen("SFC/NUL.zip", 1024, 0).expect("撞上保留名");
        assert_eq!(reason, RejectReason::BadName);
        let 长名 = format!("SFC/{}.zip", "字".repeat(300));
        let (reason, _) = exfat.screen(&长名, 1024, 0).expect("太长");
        assert_eq!(reason, RejectReason::NameTooLong);
        assert!(exfat.screen("SFC/魂斗罗.zip", 1024, 0).is_none());
    }

    #[test]
    fn 路径上限比的是完整路径() {
        let fat32 = 档案("retroarch-fat32").filesystem;
        let 深 = format!("{}/一.zip", vec!["目录"; 60].join("/"));
        assert!(fat32.screen(&深, 1024, 0).is_none(), "单看相对路径还够");
        let (reason, _) = fat32
            .screen(&深, 1024, 100)
            .expect("加上子库根那一串就超了");
        assert_eq!(reason, RejectReason::PathTooLong);
    }

    // ── 出处与陈旧 ────────────────────────────────────────────────────────

    #[test]
    fn 核实日期读不懂就当陈旧算() {
        let claim = Claim {
            cite: "随手写的".to_string(),
            verified: "上个月".to_string(),
        };
        assert!(claim.age_days(今天).is_none());
        assert!(claim.is_stale(今天), "读不懂就是核不了");
    }

    #[test]
    fn 日期差算得对() {
        let claim = |verified: &str| Claim {
            cite: String::new(),
            verified: verified.to_string(),
        };
        assert_eq!(claim("2026-09-02").age_days("2026-09-02"), Some(0));
        assert_eq!(claim("2026-08-31").age_days("2026-09-02"), Some(2));
        assert_eq!(claim("2025-09-02").age_days("2026-09-02"), Some(365));
        // 闰年：2024-02-29 存在。
        assert_eq!(claim("2024-02-28").age_days("2024-03-01"), Some(2));
        assert!(claim("2026-02-30").age_days("2026-09-02").is_none());
        assert!(claim("2026-13-01").age_days("2026-09-02").is_none());
        assert!(claim("2026-09").age_days("2026-09-02").is_none());
    }

    #[test]
    fn 耗时预估随源的大小走而且不是零() {
        let 一个 = estimate_secs(Recipe::Unpack, 60 * 1024 * 1024);
        let 两个 = estimate_secs(Recipe::Unpack, 120 * 1024 * 1024);
        assert!((两个 - 一个 * 2.0).abs() < 1e-9);
        // 重压比解压贵：这不是随手写的常量顺序，是真机上量出来的。
        assert!(estimate_secs(Recipe::Rezip, 1 << 20) > estimate_secs(Recipe::Unpack, 1 << 20));
        // **按未压缩量算**：一个压缩率 15:1 的卡带包，按源文件大小估会差一个数量级。
        assert!(estimate_secs(Recipe::Rezip, 827 * (1 << 20)) > 20.0);
    }

    // ── 名册本身的校验 ────────────────────────────────────────────────────

    #[test]
    fn 引用不存在的矩阵当场报错而不是默默少一份档案() {
        let text = r#"
"版本" = 1
[["能力档案"]]
"名" = "不作声称"
"平台矩阵" = "没有这一份"
"文件系统" = "无限制"
"#;
        let error = Roster::parse(text, "（测试）").expect_err("该报错");
        assert!(format!("{error}").contains("没有这一份"), "{error}");
    }

    #[test]
    fn 名册里没有默认那一份就报错() {
        let text = r#"
"版本" = 1
[["文件系统"]]
"名" = "无限制"
"来源" = "测试"
"核实日期" = "2026-09-02"
[["平台矩阵"]]
"名" = "空"
[["能力档案"]]
"名" = "别的"
"平台矩阵" = "空"
"文件系统" = "无限制"
"#;
        let error = Roster::parse(text, "（测试）").expect_err("该报错");
        assert!(format!("{error}").contains(DEFAULT_PROFILE), "{error}");
    }

    #[test]
    fn 星号与具体扩展名混着写当场报错() {
        let text = r#"
"版本" = 1
[["文件系统"]]
"名" = "无限制"
"来源" = "测试"
"核实日期" = "2026-09-02"
[["平台矩阵"]]
"名" = "混"
[["平台矩阵"."条目"]]
"平台" = ["SFC"]
"吃" = ["*", "zip"]
"来源" = "测试"
"核实日期" = "2026-09-02"
[["能力档案"]]
"名" = "不作声称"
"平台矩阵" = "混"
"文件系统" = "无限制"
"#;
        let error = Roster::parse(text, "（测试）").expect_err("该报错");
        assert!(format!("{error}").contains("不作声称"), "{error}");
    }

    #[test]
    fn 子库记着的档案找不到时退回不作声称而不是随便挑一份() {
        let roster = Roster::builtin();
        assert_eq!(
            roster.find_or_unclaimed(Some("这份不存在")).name,
            DEFAULT_PROFILE,
        );
        assert_eq!(roster.find_or_unclaimed(None).name, DEFAULT_PROFILE);
    }

    #[test]
    fn 读时钟那一半与算日期那一半用的是同一套公式() {
        // 两处各抄一遍公历换算的话，迟早在闰年上分家。这条钉住它们互为逆。
        for text in [
            "1970-01-01",
            "2000-02-29",
            "2024-02-29",
            "2026-09-02",
            "2100-03-01",
        ] {
            let days = day_number(text).expect("读得懂");
            assert_eq!(from_day_number(days), text);
        }
        // 真的读一次时钟：格式对得上，而且自己解得回去。
        let now = today();
        assert!(day_number(&now).is_some(), "today() 出来的是 {now}");
    }

    #[test]
    fn 穿不透的容器不会被说成不是容器() {
        // 一句「不是透明容器」套在一个货真价实的 `.zip` 上是自相矛盾的理由。
        // **穿不透与不是那个格式在这个项目里从来是分开的两件事**（`CONTEXT.md`）。
        let profile = 档案("独立模拟器-exfat");
        let Decision::Unsupported { why, .. } =
            decide(&profile, Some("PS1"), "PS1/要密码的.zip", 1024, None)
        else {
            panic!("穿不透就转不了");
        };
        assert!(why.contains("穿不透"), "{why}");
        assert!(!why.contains("不是这一版认得的"), "别说它不是容器：{why}");
    }

    #[test]
    fn retroarch_那一行照抄源码里的四种_apk_也在里面() {
        // `file_archive_get_file_backend()` 的分派是 7z / zip / apk / zst 四种。
        // 抄漏一种方向上是安全的，但与自己引的那条来源对不上——那种矩阵没法复核。
        let profile = 档案("retroarch-exfat");
        for ext in ["zip", "7z", "zst", "apk"] {
            assert_eq!(
                decide(&profile, Some("FC"), &format!("FC/某作.{ext}"), 1024, None),
                Decision::AsIs,
                "RetroArch 吃得下 .{ext}",
            );
        }
    }

    #[test]
    fn 没有一手来源的平台一律落到不作声称() {
        // SS / Mega-CD / 3DO 原本跟 PS1 挤在一行里领了一张裸镜像表，而调研第 3 部分
        // 对它们**一个字的一手来源都没有**。编一条「吃得下」比不转换更糟。
        let profile = 档案("retroarch-exfat");
        for platform in ["SS", "Mega-CD", "3DO", "PS3", "SWITCH"] {
            assert_eq!(
                decide(&profile, Some(platform), "x/某作.rar", 1024, None),
                Decision::Unclaimed,
                "{platform} 这一版没查过",
            );
        }
    }

    #[test]
    fn max_path_是_windows_的限制_对_exfat_一样成立() {
        // 只写在 FAT32 那一份上是错的：卡在主力机（Windows，ADR-0018）上写，
        // 换成 exFAT 并不会让 260 这条消失。
        let 深 = format!("{}/一.zip", vec!["目录"; 60].join("/"));
        for name in ["retroarch-exfat", "retroarch-fat32"] {
            let (reason, _) = 档案(name)
                .filesystem
                .screen(&深, 1024, 100)
                .unwrap_or_else(|| panic!("{name} 该拦下这条路径"));
            assert_eq!(reason, RejectReason::PathTooLong);
        }
    }

    #[test]
    fn 内置名册导得出来而且导出来那份还读得回去() {
        let text = Roster::builtin_text();
        let roster = Roster::parse(text, "（导出的）").expect("导出来那份读得回去");
        assert_eq!(roster.names(), Roster::builtin().names());
    }
}
