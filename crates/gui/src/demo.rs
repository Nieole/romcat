//! 合成数据：一份不碰主库就能撑起十万行表格的**中立库**。
//!
//! **主库只读**（ADR-0004），而验证「十万行滚得动」这件事不需要真库——它要的只是十万个
//! 变体。所以这里就地造一份内存里的中立库，走的是与真库**同一套表、同一条查询路径**：
//! 那正是这份合成数据的意义，若是绕过 SQLite 直接造一个 `Vec`，量出来的帧率就与真实
//! 情况无关了。
//!
//! 名字里**故意混进中日文、繁体、假名与符号**。表格里画的字若全是 ASCII，
//! 字体那条验收就等于没测——豆腐块只会在真的要画汉字时才出现。

use romcat_core::catalog::identify::{ContentHash, Identification};
use romcat_core::catalog::{Candidate, Catalog, CatalogError, Confidence, State};
use romcat_core::dat::Convention;
use romcat_core::platform::Manifest;
use romcat_core::shape::{Role, SINGLE_FILE_RULE, SPLIT_VOLUME_RULE, Variant};
use romcat_core::site::Site;
use romcat_core::verdict::Store;

/// 造名字用的作品名。繁简、假名、罗马数字、带圈数字、音符、星号各占几条。
const WORKS: &[&str] = &[
    "幻想传说",
    "潛龍諜影 Ⅲ",
    "ゼルダの伝説 ～時のオカリナ～",
    "勇者鬥惡龍Ⅺ",
    "皇家騎士團 ①",
    "最终幻想 Ⅶ ★特别版★",
    "洛克人 X ♪サウンドトラック付",
    "女神轉生 Ⅱ",
    "机器人大战 α",
    "圣剑传说 3 · 汉化版",
    "モンスターハンター ポータブル",
    "太空戰士 Ⅵ（繁中）",
    "秘密の花園 ♥",
    "三国志曹操传",
    "鬼武者 ～Onimusha～",
    "英雄伝説 空の軌跡 FC",
    "口袋妖怪 · 绿宝石 ①②③",
    "街霸 Ⅱ ターボ",
    "仙劍奇俠傳",
    "轩辕剑外传 · 天之痕",
];

/// 平台目录名。真库就是按平台分目录的（ADR-0011），合成数据照做。
const PLATFORMS: &[&str] = &[
    "SFC", "PS1", "PS2", "PSP", "NDS", "GBA", "MD", "N64", "SS", "DC", "WII", "PSV", "3DS", "NSW",
    "FC", "MAME", "PCE",
];

/// 汉化组的记号，真库里的文件名带这类后缀。
const MARKS: &[&str] = &[
    "汉化版",
    "官中",
    "日版",
    "美版",
    "繁中",
    "UnDUB",
    "英化",
    "同人移植",
];

/// 造一个装着 `rows` 个变体的内存中立库。
///
/// # Errors
/// 建库或写库失败时返回错误。
pub fn synthetic(rows: u64) -> Result<Catalog, CatalogError> {
    let mut catalog = Catalog::open_in_memory()?;
    let variants: Vec<Variant> = (0..rows)
        .map(|i| {
            let platform = PLATFORMS[(i as usize) % PLATFORMS.len()];
            let work = WORKS[(i as usize / 3) % WORKS.len()];
            let mark = MARKS[(i as usize / 7) % MARKS.len()];
            let key = format!("{platform}/{work}（{mark}）#{i:06}.zip");
            Variant {
                main_key: key.clone(),
                key,
                // 每十七个留一个**平台未知**：那是真库里存在的一档（认不出平台的内容
                // 照常入库），按平台排序时 `NULL` 不该让表格翻车。
                platform: (i % 17 != 3).then(|| platform.to_string()),
                // 成型规则只能是真有的那几条：库里绝大多数是一文件一变体，
                // 分卷压缩是少数（`docs/library-facts.md`）。
                rule: if i % 11 == 0 {
                    SPLIT_VOLUME_RULE.to_string()
                } else {
                    SINGLE_FILE_RULE.to_string()
                },
                manual: false,
                files: 1 + i % 9,
                // 乘一个质数再取模，让容量既不单调也不重复太多——按容量排序时
                // 那才是个真的排序。
                bytes: (i.wrapping_mul(2_654_435_761)) % 8_000_000_000,
                unreadable_files: 0,
                members: Vec::new(),
            }
        })
        .collect();
    catalog.replace_variants(&variants, 1, &Manifest::default())?;
    Ok(catalog)
}

/// 真机上**待确认队列**有多少条（票 08 实测）。
///
/// 16,656 ＝ 未命中 11,823 ＋ 无判据 4,537 ＋ 命中但一条候选都没自动通过 296。
pub const QUEUE_ROWS: u64 = 16_656;

/// 真机上「无判据」那一档有多少条。
const NO_EVIDENCE: u64 = 4_537;

/// 真机上「命中但一条都没自动通过」有多少条。**它们才是有候选可挑的那些。**
const MATCHED: u64 = 296;

/// 一批待裁决的变体：**同一个目录、同一种名字规律**，也就是一次批量裁决盖得住的那一批。
struct Bucket {
    /// 落在哪个目录下。**按目录**那个轴数的就是它。
    dir: &'static str,
    /// 平台；数据源目录（GoodNES3.1 那种）认不出平台，那在真库里是常态。
    platform: Option<&'static str>,
    /// 主文件的扩展名。**故意不用容器扩展名**：容器要 `container_entry` 才穿得透，
    /// 而这份合成数据不造容器——真库里那条路由票 03 覆盖。
    ext: &'static str,
    /// 名字里带不带「汉化」。`--name 汉化` 一条覆盖的就是这些。
    translated: bool,
    /// 真机上这一批有多少条。
    count: u64,
}

/// 真机队列的**形状**（票 08 实测的那几个数）。
///
/// 造合成数据不是为了好看：这三个数是 ADR-0002 那句「一条命令覆盖几百条」的量纲，
/// 界面上「点一行选中多少」若不在这个量级上，实测出来的响应就与真机无关。
///
/// - `--under gba/【全部汉化】` **852** 条
/// - `--under GoodNES3.1` **1,543** 条
/// - `--name 汉化` **1,986** 条（852 ＋ 300 ＋ 834）
const SHAPE: &[Bucket] = &[
    Bucket {
        dir: "gba/【全部汉化】",
        platform: Some("GBA"),
        ext: "gba",
        translated: true,
        count: 852,
    },
    Bucket {
        dir: "nds/【汉化合集】",
        platform: Some("NDS"),
        ext: "nds",
        translated: true,
        count: 834,
    },
    Bucket {
        dir: "GoodNES3.1",
        platform: None,
        ext: "nes",
        translated: true,
        count: 300,
    },
    Bucket {
        dir: "GoodNES3.1",
        platform: None,
        ext: "nes",
        translated: false,
        count: 1_243,
    },
    Bucket {
        dir: "SFC",
        platform: Some("SFC"),
        ext: "sfc",
        translated: false,
        count: 3_000,
    },
    Bucket {
        dir: "PS1",
        platform: Some("PS1"),
        ext: "bin",
        translated: false,
        count: 2_600,
    },
    Bucket {
        dir: "MD",
        platform: Some("MD"),
        ext: "md",
        translated: false,
        count: 2_100,
    },
    Bucket {
        dir: "PSP",
        platform: Some("PSP"),
        ext: "iso",
        translated: false,
        count: 1_900,
    },
    Bucket {
        dir: "3DS",
        platform: Some("3DS"),
        ext: "3ds",
        translated: false,
        count: 1_500,
    },
    Bucket {
        dir: "MAME",
        platform: Some("MAME"),
        ext: "bin",
        translated: false,
        count: 2_327,
    },
];

/// 真机上点得出名字的那三个**汉化组**记号，连各自的条数（票 08 实测）。
///
/// ADR-0002 点名的「按汉化组命名规律一次套用几百条」，落到真库上就是这几行。
const TEAMS: &[(&str, u64)] = &[("ACG汉化组", 129), ("CG汉化组", 54), ("巴士汉化组", 33)];

/// 别的记号从这个池子里轮着取。真机上按命名规律折出 **3,391 组**记号，而这里是
/// 池子里的 3,388 个加上面那三个汉化组——分组那一步要数多少个桶，是响应快慢的那一半。
const MARK_POOL: u64 = 3_388;

/// 队列合成数据用的作品名。
///
/// **故意不带「汉化」二字，也不带任何括号**：这份数据要精确控制的正是那两样——
/// `--name 汉化` 一条覆盖多少、按命名规律折出多少组记号，全由插进去的 `[记号]` 说了算。
/// 名字里照旧混着繁简、假名、罗马数字、带圈数字与符号，字体那条验收才有得测。
const QUEUE_WORKS: &[&str] = &[
    "幻想传说",
    "潛龍諜影 Ⅲ",
    "ゼルダの伝説 ～時のオカリナ～",
    "勇者鬥惡龍Ⅺ",
    "皇家騎士團 ①",
    "最终幻想 Ⅶ ★特别版★",
    "洛克人 X ♪サウンドトラック",
    "女神轉生 Ⅱ",
    "机器人大战 α",
    "圣剑传说 3",
    "モンスターハンター ポータブル",
    "太空戰士 Ⅵ",
    "秘密の花園 ♥",
    "三国志曹操传",
    "鬼武者 ～Onimusha～",
    "英雄伝説 空の軌跡 FC",
    "口袋妖怪 · 绿宝石 ①②③",
    "街霸 Ⅱ ターボ",
    "仙劍奇俠傳",
    "轩辕剑外传 · 天之痕",
];

/// 「无判据」那一档的理由。真机上「为什么没定下来」是队列里最该先读的一行。
const REASONS: &[&str] = &[
    "容器穿不透：格式不认",
    "压缩镜像这一层认不了",
    "元数据读不到",
    "这个平台没有可撞的 DAT",
];

/// 造一份**待确认队列**用的中立库：`rows` 条待裁决，形状照真机来。
///
/// **主库只读**（ADR-0004），这条路一个字节都不碰真库；库本身在内存里。
/// `rows` 不等于 [`QUEUE_ROWS`] 时按比例缩放，余数落在最后一批上。
///
/// # Errors
/// 建库或写库失败时返回错误。
pub fn queue(rows: u64) -> Result<Catalog, CatalogError> {
    let mut catalog = Catalog::open_in_memory()?;
    let scale = |count: u64| count.saturating_mul(rows) / QUEUE_ROWS;
    let mut variants = Vec::new();
    let mut hashes = Vec::new();
    let mut records = Vec::new();
    let (matched, no_evidence) = (scale(MATCHED), scale(MATCHED) + scale(NO_EVIDENCE));

    let mut n: u64 = 0;
    for (which, bucket) in SHAPE.iter().enumerate() {
        // 最后一批把缩放的余数吃掉，于是总数**恰好**是 `rows`。
        let count = if which + 1 == SHAPE.len() {
            rows.saturating_sub(n)
        } else {
            scale(bucket.count)
        };
        for at in 0..count {
            let work = QUEUE_WORKS[(n as usize / 3) % QUEUE_WORKS.len()];
            let mark = mark_of(bucket, at, n, scale);
            let name = if bucket.translated {
                format!("{work}[{mark}]汉化版#{n:05}.{}", bucket.ext)
            } else {
                format!("{work}[{mark}]#{n:05}.{}", bucket.ext)
            };
            let key = format!("{}/{name}", bucket.dir);
            let state = match n {
                _ if n < matched => State::Matched,
                _ if n < no_evidence => State::NoEvidence,
                _ => State::Unmatched,
            };
            variants.push(Variant {
                main_key: key.clone(),
                platform: bucket.platform.map(str::to_string),
                rule: if n.is_multiple_of(11) {
                    SPLIT_VOLUME_RULE.to_string()
                } else {
                    SINGLE_FILE_RULE.to_string()
                },
                manual: false,
                files: 1,
                bytes: (n.wrapping_mul(2_654_435_761)) % 8_000_000_000,
                unreadable_files: 0,
                members: vec![(key.clone(), Role::Main)],
                key: key.clone(),
            });
            // **无判据那一档拿不到内容判据**——那正是它落进这一档的原因，
            // 于是它的裁决只钉得住本机路径。别的都有 CRC-32 加大小，钉在内容上。
            if state != State::NoEvidence {
                hashes.push(ContentHash {
                    key: key.clone(),
                    inner: String::new(),
                    size: 1 + n % 4_000_000,
                    #[allow(clippy::cast_possible_truncation)]
                    crc32: n.wrapping_mul(2_654_435_761) as u32,
                    looked: true,
                    header: None,
                    bare_size: None,
                    bare_crc32: None,
                    nkit: None,
                    sha1: None,
                    bare_sha1: None,
                });
            }
            records.push(Identification {
                variant_key: key.clone(),
                state,
                reason: (state == State::NoEvidence)
                    .then(|| REASONS[(n as usize) % REASONS.len()].to_string()),
                units: 1,
                nkit: 0,
                read_bytes: 0,
                work_id: None,
                release_id: None,
                // **命中但一条都没自动通过**的那些才有候选可挑；队列里绝大多数一条都没有
                // （真机上 16,360 条、98.2%），所以「手工指定」是主路径不是备用路径。
                candidates: if state == State::Matched {
                    vec![candidate_of(work, bucket, &mark)]
                } else {
                    Vec::new()
                },
            });
            n += 1;
        }
        if n >= rows {
            break;
        }
    }

    catalog.replace_variants(&variants, 1, &Manifest::default())?;
    catalog.put_content_hashes(&hashes)?;
    catalog.write_identifications(&records)?;
    Ok(catalog)
}

/// 这一条名字里带哪个记号。
///
/// 前几个是真机上点得出名字的**汉化组**（它们各带几百条，正是 ADR-0002 说的
/// 「按汉化组命名规律一次套用几百条」）；其余从池子里按**全局**次序轮着取，
/// 于是整份数据折出来的记号组数是池子的大小加那三个。
fn mark_of(bucket: &Bucket, at: u64, n: u64, scale: impl Fn(u64) -> u64) -> String {
    if bucket.translated && bucket.dir.starts_with("gba/") {
        let mut floor = 0;
        for (team, count) in TEAMS {
            let ceiling = floor + scale(*count);
            if at < ceiling {
                return (*team).to_string();
            }
            floor = ceiling;
        }
    }
    format!("组{:04}", n % MARK_POOL)
}

/// 一条**没有自动通过**的候选，带**依据**。
fn candidate_of(work: &str, bucket: &Bucket, mark: &str) -> Candidate {
    let game = format!("{work} (Japan)");
    Candidate {
        member_key: String::new(),
        inner: String::new(),
        // 中置信那一档：撞上了但有保留，通过但标记，等人裁决（ADR-0002）。
        confidence: Confidence::Medium,
        accepted: false,
        source: "TOSEC".to_string(),
        dat: format!("{}.dat", bucket.ext),
        platform: bucket.platform.unwrap_or("未知").to_string(),
        game,
        rom: format!("rom.{}", bucket.ext),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: format!("CRC-32 加大小撞上 TOSEC 的一条记录；名字里的记号是「{mark}」"),
        chinese: bucket
            .translated
            .then_some(romcat_core::dat::chinese::ChineseMark::FanTranslated),
        serial: None,
        release_id: None,
    }
}

/// 这份合成数据在**路径锚**里叫什么名字。真库永远不会叫这个。
pub const LIBRARY: &str = "合成数据";

/// 一份**全在内存里**的现场：合成数据配一份空沉淀库。
///
/// # Errors
/// 建库失败时返回错误。
pub fn site(catalog: Catalog) -> Result<Site, String> {
    let store = Store::in_memory().map_err(|error| format!("开不出沉淀库：{error}"))?;
    Ok(Site::in_memory(catalog, store, LIBRARY))
}
