//! **按下去之前的那本账**：这一趟会发多少网络请求、大概多久。
//!
//! 界面上那块刮削面板（票 `gui-redesign/10`）底下常驻的就是它。它存在的理由只有一条：
//! **在线源赌的是你的账号与 IP**（ADR-0007）——配额同时按账号与 IP 计，撞穿了是永久
//! 封禁。一个「预计 0 个请求」而按下去发了九千个的界面，比不给估算更坏。
//!
//! ## 为什么它不是「把计划立起来数一遍」
//!
//! 那样最准，但走不通：立一份完整的[计划](super)要读全库的变体（真库 46,444 个）、
//! 全库的候选（150,959 条）与全库的文件表（216,203 条）。那是好几秒的活，而这块面板
//! 上每动一下旋钮都要重算一次——放在画帧那条线程上就是挂账 D156 那件事的翻版。
//!
//! 于是这里走一条**窄得多、但答得出同一个数**的路：
//!
//! 1. 问中立库「这一批变体牵动哪几个作品、各自的**代表变体**是谁」
//!    （[`Catalog::work_representatives`]，一句 SQL）——代表变体的挑法与
//!    `Plan::build` 是同一条（键最小的那个），因此两边挑中的是同一个；
//! 2. 逐个取**判据**（`Catalog::accepted_hash`，与计划走同一个函数）；
//! 3. 用[同一个指纹函数](super::online::query_fingerprint)折出这一趟的输入指纹，
//!    与库里记着的上一趟比——**不一样的才会发请求**。
//!
//! **三步各自都与真跑那一趟共用同一段代码**，不是照着抄一遍。抄一遍的东西会漂，
//! 而漂开的方向正是上面那句「屏上说 0 个」。
//!
//! ## 这个数是**查询**那一半
//!
//! 一次条目查询回来之后，每一份要收的图还要各下一次（`ingest_online`）。图有几份要
//! 查过才知道——源不说，谁也算不出来。所以 [`Estimate::media_downloads`] 为真时，
//! [`Estimate::requests`] 是**下界**，面板上要照实说。不收媒体时它就是准数。

use std::collections::BTreeSet;
use std::time::Duration;

use crate::catalog::browse::VariantQuery;
use crate::catalog::{Catalog, CatalogError};

use super::online::{self, Limits, SCREEN_SCRAPER};
use super::{AnchorKind, Basis, Locality, Options, Profile};

/// **一个锚点采一遍元数据要多久**。
///
/// 实测：真库 55,670 个锚点、七个本地源，元数据那一趟 **2.3 秒**
/// （`docs/library-facts.md`，2026-09-01）。42 微秒就是这么来的。
///
/// 它是个常数而不是一个学出来的数：这份估算要的是「一分半还是二十分钟」这个量级，
/// 而不是秒表。量级说对了，人就知道该不该按下去。
const PER_ANCHOR: Duration = Duration::from_nanos(42_000);

/// 按下去之前算得出的那本账。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    /// 这一趟要过多少个**变体**锚点。
    pub variants: u64,
    /// 这一趟要过多少个**作品**锚点。
    pub works: u64,
    /// **会发多少个网络请求。** 只用本地源时是 0。
    ///
    /// 它是**查询**那一半，而且是准数：一部作品发一次条目查询
    /// （`Plan::build` 的「一部作品发一次查询就够」）。
    /// [`Self::media_downloads`] 为真时，实际发出去的会比它多——多出来的是图。
    pub requests: u64,
    /// 想发的比这一趟自设的上限多时，原本想发多少。
    ///
    /// **上限是自己设的保守闸，不是服务端的配额**（`online::DEFAULT_BUDGET`）：
    /// 撞上它这一趟会当场停下，剩下的下一趟再来。屏上要把这件事说出来——
    /// 不说的话，人会以为「按一次就采完了」。
    ///
    /// **它只看得见查询那一半。** 那道闸拦的是 `Usage::requests`，而那个数
    /// 「查询与下媒体都算」——收媒体时几百个作品各带几张图，走到一小部分就撞上了，
    /// 而这里算出来的 `queries` 还没到上限。所以收媒体那一档屏上另说一句
    /// （见 [`Self::budget`]），别让人以为「没标红就是发得完」。
    pub over_budget: Option<u64>,
    /// 这一趟自设的请求上限。**查询与下媒体共用这一份。**
    ///
    /// 面板拿它把上一条说不出的那半说出来：收媒体的在线档里，图也吃这份预算，
    /// 撞上就当场停下。
    pub budget: u64,
    /// 查询之外，**每采到一份图还要各下一次**。为真时 [`Self::requests`] 是下界。
    pub media_downloads: bool,
    /// 这一趟要不要回主库读盘（收媒体、而且用了本地源）。
    ///
    /// 首趟读盘那一段不在 [`Self::elapsed`] 里：读多少取决于这一批旁边摆着多少张图、
    /// 各自多大，那要把全库的文件表折一遍才数得出来——而这块面板每动一下旋钮都要
    /// 重算一次。**说不出来就说说不出来**，别摆一个编出来的秒数。
    pub media_disk_read: bool,
    /// 大概多久。
    pub elapsed: Duration,
}

impl Estimate {
    /// 这一趟一共要过几个锚点。
    #[must_use]
    pub fn anchors(&self) -> u64 {
        self.variants.saturating_add(self.works)
    }
}

/// 算一遍这本账。
///
/// `limits` 是在线那一档的限流参数：两次请求之间隔多久（耗时全靠它）、这一趟自己设的
/// 请求上限（[`Estimate::over_budget`] 靠它）。离线档一个都用不上。
///
/// # Errors
/// 读库失败时返回错误。
pub fn estimate(
    catalog: &Catalog,
    options: &Options,
    limits: Limits,
) -> Result<Estimate, CatalogError> {
    let works = catalog.work_representatives(options.only.as_ref())?;
    let variants = match &options.only {
        Some(keys) => u64::try_from(keys.len()).unwrap_or(u64::MAX),
        None => catalog.variant_total(&VariantQuery::default())?,
    };
    let wanted = u64::try_from(works.len()).unwrap_or(u64::MAX);

    let queries = if options.profile == Profile::Online {
        count_queries(catalog, options, &works)?
    } else {
        // **离线档一个请求都不发。** 这不是算出来的，是那道闸门的结论：离线档只收
        // 自报本地的源，混进一个联网源会当场被拒（`scrape::sources`）。
        0
    };
    let (requests, over_budget) = if queries > limits.budget {
        (limits.budget, Some(queries))
    } else {
        (queries, None)
    };
    // 本地那一半按锚点数算，在线那一半按**限流**算——两次请求之间至少隔多久是这一档
    // 自己定的，那比什么都准。两段相加：在线那一趟本地源照跑。
    let elapsed = PER_ANCHOR.saturating_mul(u32::try_from(variants + wanted).unwrap_or(u32::MAX))
        + limits
            .interval
            .saturating_mul(u32::try_from(requests).unwrap_or(u32::MAX));
    Ok(Estimate {
        variants,
        works: wanted,
        requests,
        over_budget,
        budget: limits.budget,
        media_downloads: options.media && options.profile == Profile::Online,
        media_disk_read: options.media,
        elapsed,
    })
}

/// 有几个作品锚点这一趟真会发出一次条目查询。
fn count_queries(
    catalog: &Catalog,
    options: &Options,
    works: &[(String, String)],
) -> Result<u64, CatalogError> {
    // 上一趟的采集记录一次问回来。几千个作品各问一次是几千次库查询，而这块面板每动
    // 一下旋钮都要重算一遍。
    let known = catalog.scrape_inputs_of(AnchorKind::Work.label(), SCREEN_SCRAPER)?;
    let mut count = 0_u64;
    for (name, representative) in works {
        // 判据取不出来就是**没有可发的哈希**：这一条一个请求都不会发出去，
        // 而拿文件名去碰运气正是 431 惩罚的那件事（`ScreenScraper::probe`）。
        let Some((crc32, bytes, rom_name)) = catalog.accepted_hash(representative)? else {
            continue;
        };
        let basis = Basis {
            crc32,
            bytes,
            rom_name,
        };
        let input = options.cache_key(
            Locality::Online,
            online::query_fingerprint(&basis, options.max_media_bytes),
        );
        // **重采绕过采集记录**，于是每一条都会重新问一遍。
        if !options.refresh && known.get(name) == Some(&input) {
            continue;
        }
        count += 1;
    }
    Ok(count)
}

/// 这一批变体的键，收成估算与真跑都认的那个形状。
///
/// 界面那一侧手里是 `Catalog::scoped_variants` 交出来的一串键
/// （票 `gui-redesign/03` 钉住的那条口径：屏上写几个、面板列几个、按下去动几个，
/// 三处同一个数）。收成集合是为了让**估算与真跑用的是同一份**——两边各收一次，
/// 迟早有一次忘了去重。
#[must_use]
pub fn only(keys: impl IntoIterator<Item = String>) -> BTreeSet<String> {
    keys.into_iter().collect()
}
