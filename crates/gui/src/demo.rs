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

/// 合成数据这一趟的**工作目录**。
///
/// **故意不是维护者真正的工作目录**：子库那一屏会读工作目录里的**媒体池**与能力档案
/// 名册，而合成数据跑的是演示，不该让它去翻真库那一套。这个目录**不建出来**——
/// 名册与媒体池在目录不存在时都退回内置的空手状态，正是演示要的。
#[must_use]
pub fn workspace() -> std::path::PathBuf {
    std::env::temp_dir().join("romcat-合成数据")
}

/// 一份**全在内存里**的现场：合成数据配一份空沉淀库。
///
/// # Errors
/// 建库失败时返回错误。
pub fn site(catalog: Catalog) -> Result<Site, String> {
    let store = Store::in_memory().map_err(|error| format!("开不出沉淀库：{error}"))?;
    Ok(Site::in_memory(catalog, store, LIBRARY))
}

// ── 库浏览与子库那两屏的合成数据 ────────────────────────────────────────────

/// 合成数据里的**合集**。与平台正交（ADR-0011）：「我通关过的」是合集，「SFC」是平台。
const COLLECTIONS: &[(&str, u64)] = &[
    ("我通关过的", 9),
    ("适合双人玩的", 17),
    ("小时候玩过", 29),
    ("汉化精选", 41),
];

/// 发行版的地区，连它标着的语言。
///
/// **官中版在有独立序列号的世代是一条独立的发行版**（ADR-0019）：`China` 那一条带
/// `Zh-Hans`，与日版、美版平级。数字世代那一侧则是 `Asia` 加一个语言标记——
/// 这里两种形状都造得出来，筛选那一维才有得测。
const REGIONS: &[(&str, &str)] = &[
    ("Japan", "Ja"),
    ("USA", "En"),
    ("Europe", "En,Fr,De"),
    ("China", "Zh-Hans"),
    ("Asia", "Ja,Zh-Hant,En"),
];

/// 一份**库浏览**与**子库**用的中立库：`rows` 个变体，连作品、发行版、合集、
/// 识别结论、**标题集合**与**媒体**引用，而且每个变体真的有一个**文件成员**。
///
/// [`synthetic`] 只造变体（那一份是给十万行表格量帧率的）。这一份多造五样，因为票 25
/// 的四个筛选维度——平台、合集、语言、识别状态——各要一样：语言在发行版上、合集在
/// 关系表里、识别状态在结论表里，一样缺了那一维在界面上就是空的。第五样是**成员与
/// 条目**：子库那一屏要折出期望状态，而 `sync::desired` 拿的是
/// `variant_member` 连 `entry`——没有它们，差量预览里一个 ROM 都不会出现。
///
/// **主库只读**（ADR-0004），这条路一个字节都不碰真库；库本身在内存里。
///
/// # Errors
/// 建库或写库失败时返回错误。
#[allow(clippy::too_many_lines)]
pub fn library(rows: u64) -> Result<Catalog, CatalogError> {
    use romcat_core::catalog::identify::Provenance;
    use romcat_core::catalog::scrape::{Harvested, HarvestedMedia, HarvestedValue};
    use romcat_core::catalog::title::TitleRow;
    use romcat_core::catalog::{EntryRecord, Verdict};
    use romcat_core::fs::{EntryKind, EntryMeta};
    use romcat_core::scrape::priority::VERDICT;
    use romcat_core::scrape::{AnchorKind, Field, MediaKind};
    use romcat_core::title::{Language, TitleKind};

    let key_of = |i: u64| {
        let platform = PLATFORMS[(i as usize) % PLATFORMS.len()];
        let work = WORKS[(i as usize / 3) % WORKS.len()];
        let mark = MARKS[(i as usize / 7) % MARKS.len()];
        format!("{platform}/{work}（{mark}）#{i:06}.zip")
    };

    let mut catalog = Catalog::open_in_memory()?;
    // 零、条目与变体。**每个变体一个文件成员**：子库那一屏要靠它折出期望状态
    //     （`sync::desired` 走的是 `variant_member` 连 `entry`）。容量刻意压在
    //     几十 KiB 到几 MiB——`--bench-sublibrary` 那一趟要真的往 fixture 目录里写，
    //     几 GiB 的合成文件既没意义又写不下（本机只有十几 GiB）。
    let mut entries = Vec::with_capacity(rows as usize);
    let mut variants = Vec::with_capacity(rows as usize);
    for i in 0..rows {
        let key = key_of(i);
        let bytes = 32 * 1024 + (i.wrapping_mul(2_654_435_761)) % (4 * 1024 * 1024);
        entries.push(EntryRecord {
            key: key.clone(),
            kind: EntryKind::File,
            meta: EntryMeta::Known {
                len: bytes,
                modified: None,
            },
            non_utf8: false,
            verdict: Verdict::Added,
            sample: None,
            container: None,
        });
        variants.push(Variant {
            main_key: key.clone(),
            platform: (i % 17 != 3).then(|| PLATFORMS[(i as usize) % PLATFORMS.len()].to_string()),
            rule: if i % 11 == 0 {
                SPLIT_VOLUME_RULE.to_string()
            } else {
                SINGLE_FILE_RULE.to_string()
            },
            manual: false,
            files: 1,
            bytes,
            unreadable_files: 0,
            members: vec![(key.clone(), Role::Main)],
            key,
        });
    }
    catalog.write(1, &entries)?;
    catalog.replace_variants(&variants, 1, &Manifest::default())?;

    // 一、作品与发行版。作品 20 个，发行版是「作品 × 平台 × 地区」里真用得上的那些。
    let mut works = Vec::with_capacity(WORKS.len());
    for name in WORKS {
        works.push(catalog.add_work(name, Provenance::Identified)?);
    }
    let mut releases: Vec<i64> = Vec::new();
    for (at, work) in works.iter().enumerate() {
        for (which, (region, languages)) in REGIONS.iter().enumerate() {
            let platform = PLATFORMS[(at + which) % PLATFORMS.len()];
            releases.push(catalog.add_release(
                *work,
                Some(platform),
                Some(region),
                Some(&format!("SLPS-{:05}", at * 10 + which)),
                Some(languages),
                Provenance::Identified,
            )?);
        }
    }

    // 二、识别结论，顺带把变体挂到作品与发行版上（`write_identifications` 一趟做完，
    //     一条一条 `link_variant` 在十万行上是几万次自动提交）。
    //     **每十三个留一个压根没有结论行**：那是「还没识别」，与「未命中」不是一回事。
    let mut records = Vec::new();
    for i in 0..rows {
        if i % 13 == 7 {
            continue;
        }
        let state = State::ALL[(i as usize / 5) % State::ALL.len()];
        let at = (i as usize / 3) % works.len();
        records.push(Identification {
            variant_key: key_of(i),
            state,
            reason: (state == State::NoEvidence)
                .then(|| REASONS[(i as usize) % REASONS.len()].to_string()),
            units: 1,
            nkit: 0,
            read_bytes: 0,
            work_id: Some(works[at]),
            release_id: Some(releases[at * REGIONS.len() + (i as usize) % REGIONS.len()]),
            candidates: Vec::new(),
        });
    }
    catalog.write_identifications(&records)?;

    // 三、合集。
    for (name, every) in COLLECTIONS {
        let id = catalog.add_collection(name)?;
        for i in (0..rows).filter(|i| i % every == 0) {
            catalog.add_to_collection(id, &key_of(i))?;
        }
    }

    // 四、**标题集合**：每个作品三条叫法。中文那条是**官中版的官方译名**——
    //     ADR-0012 那句「首选变体与标题来源解耦」要看得见，就得真有这么一条。
    let mut titles = Vec::new();
    for (at, work) in WORKS.iter().enumerate() {
        for (value, language, kind, source, region) in [
            (
                format!("Work {at:02} (USA)"),
                Language::English,
                TitleKind::Official,
                "No-Intro",
                Some("USA"),
            ),
            (
                format!("Sakuhin {at:02}"),
                Language::Japanese,
                TitleKind::Official,
                "No-Intro",
                Some("Japan"),
            ),
            (
                (*work).to_string(),
                Language::Chinese,
                TitleKind::Translated,
                "Redump",
                Some("China"),
            ),
        ] {
            titles.push(TitleRow {
                work: (*work).to_string(),
                value,
                language,
                kind,
                source: source.to_string(),
                region: region.map(str::to_string),
                variant_key: None,
                confidence: Confidence::High,
                seam: None,
                evidence: "合成数据".to_string(),
                seen: 3,
            });
        }
    }
    // 一条**裁决**来源的，用来核对「人改过的东西不许被数据源覆盖」在界面上看得出来。
    titles.push(TitleRow {
        work: WORKS[0].to_string(),
        value: "幻想传说（人改过的）".to_string(),
        language: Language::Chinese,
        kind: TitleKind::Translated,
        source: VERDICT.to_string(),
        region: None,
        variant_key: None,
        confidence: Confidence::High,
        seam: None,
        evidence: "合成数据里那条人工裁决".to_string(),
        seen: 1,
    });
    catalog.put_titles(&titles)?;

    // 五、**媒体**：封面每个作品都有，截图只有一半有，视频一个都没有——
    //     「这条缺哪些媒体」那条验收要有东西可缺。
    let mut harvested = Vec::new();
    for (at, work) in WORKS.iter().enumerate() {
        let mut media = Vec::new();
        for (kind, salt) in [(MediaKind::Cover, 0), (MediaKind::Screenshot, 1)] {
            if kind == MediaKind::Screenshot && !at.is_multiple_of(2) {
                continue;
            }
            let hash = format!("{:040x}", at * 2 + salt);
            catalog.put_media(&hash, "png", 64 * 1024)?;
            media.push(HarvestedMedia {
                kind: kind.label().to_string(),
                hash,
                evidence: "合成数据".to_string(),
            });
        }
        harvested.push(Harvested {
            anchor: AnchorKind::Work.label().to_string(),
            subject: (*work).to_string(),
            source: "合成数据".to_string(),
            input: format!("合成 {at}"),
            values: vec![HarvestedValue {
                field: Field::Genre.label().to_string(),
                value: "角色扮演".to_string(),
                evidence: "合成数据".to_string(),
            }],
            media,
        });
    }
    catalog.put_scraped(&harvested)?;

    Ok(catalog)
}
