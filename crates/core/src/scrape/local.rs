//! **主库里现成的东西**：变体的文件名，以及躺在变体旁边的图片与视频。
//!
//! 这两个源是离线档的兜底与唯一媒体来源。
//!
//! ## 文件名源：保证每个变体都有一个能显示的标题
//!
//! 真库里 46,444 个变体只有 30,024 个被认出来。剩下那 16,420 个如果没有标题，前端里
//! 就是一片空白——而它们的文件名里其实写着中文名。所以文件名是一个**正经的源**，
//! 只是**排在优先级链的最后**：DAT 说得出话时轮不到它，DAT 沉默时它接上。
//!
//! ## 本地媒体源：两条**保守**的归属规则
//!
//! 「这张图是哪个变体的」在一个手工养了多年的库上没有通用答案。这里只认两条错不了的：
//!
//! - **同名兄弟**：`游戏.zip` 旁边的 `游戏.png`。名字对上就是它。
//! - **独占目录**：一个目录里**只有一个变体**，那这个目录里的图就是它的。
//!   目录里有两个以上变体就一张都不认——`FC/` 底下三千个 zip 挤在一起时，
//!   同目录的图属于谁根本说不清，**宁可不给也不能给错**。
//!
//! 两条规则的实测产出：真库上 230 个变体、646 份媒体（10.0 GiB，大头是汉化版的
//! `视频预览.mp4`）。数字不大，但它们**恰恰落在在线源覆盖不到的汉化版上**——
//! ScreenScraper 眼里那些正是「未识别 ROM」（ADR-0007）。
//!
//! **目录树转储里的图一张都不会被认走**：那些图躺在变体内部更深的目录里，那些目录里
//! 一个变体主文件都没有，两条规则都够不着。它们是**内部资源**，本来就不该被当成封面。

use std::collections::{BTreeMap, BTreeSet};

use crate::catalog::{Catalog, CatalogError, VariantRow};
use crate::path::file_name_of_key;

use super::{
    Failure, Field, Harvest, LocalMedia, Locality, MediaClaim, MediaFrom, MediaKind, Source,
    Subject,
};

use super::pool::{extension_of, normalized_ext};

/// **文件名源**：从变体主文件的名字里读一个标题。
#[derive(Debug, Clone, Default)]
pub struct FilenameSource;

impl FilenameSource {
    /// 造一个。
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

/// 这个源在优先级表与依据里叫什么。
pub const FILENAME: &str = "文件名";

/// 本地媒体源在优先级表与依据里叫什么。
pub const LOCAL_MEDIA: &str = "本地媒体";

impl Source for FilenameSource {
    fn name(&self) -> &str {
        FILENAME
    }

    fn locality(&self) -> Locality {
        Locality::Local
    }

    fn probe(&self, subject: &Subject<'_>) -> Option<String> {
        let main = subject.main_key?;
        if subject.kind != super::AnchorKind::Variant {
            return None;
        }
        Some(super::fingerprint(&[main]))
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        let Some(main) = subject.main_key else {
            return Ok(());
        };
        let name = file_name_of_key(main);
        out.value(
            Field::Title,
            title_from_filename(name),
            format!("主文件叫「{name}」"),
        );
        Ok(())
    }
}

/// 从一个文件名折出标题。
///
/// 与 [`identify::naming::work_title`](crate::identify::naming::work_title) 不同的地方
/// 只有一处，而那一处是必须的：DAT 的条目名按规范在标记组前留一个空格
/// （`1942 (Japan)`），**真实文件名不留**（`恋爱复仇战[PCSG00718]`）。按 DAT 那套切，
/// 后者整条切不动，标题里就带着一串序列号。
#[must_use]
pub fn title_from_filename(name: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    let cut = ['(', '[', '（', '【']
        .iter()
        .filter_map(|marker| stem.find(*marker))
        .min();
    let title = match cut {
        // 切成空串比留着噪音更糟：名字整个就是一个标记组时原样留着。
        Some(at) if at > 0 => &stem[..at],
        _ => stem,
    };
    let title = title.trim();
    if title.is_empty() {
        stem.trim().to_string()
    } else {
        title.to_string()
    }
}

/// **本地媒体源**：把归得上的图片与视频报出来。
///
/// 它只**说**「这个键的文件是这个变体的封面」，读字节、算哈希、往**媒体池**里放
/// 全都归引擎——采集是纯的（[`Source`] 的文档）。
#[derive(Debug, Clone, Default)]
pub struct LocalMediaSource;

impl LocalMediaSource {
    /// 造一个。
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Source for LocalMediaSource {
    fn name(&self) -> &str {
        LOCAL_MEDIA
    }

    fn locality(&self) -> Locality {
        Locality::Local
    }

    fn probe(&self, subject: &Subject<'_>) -> Option<String> {
        if subject.media.is_empty() {
            return None;
        }
        // 指纹要盖住**一切会改变结果的东西**，漏一样，那一样变了就不会重收。
        //
        // - **扫描判增量用的那个三元组**（键、大小、修改时间）：只盖键的话，一张图被
        //   原地换掉（名字没变、内容变了）就不会重收，池里那份成了旧的。
        // - **这一趟的媒体上限**：上限从 32 MiB 提到 128 MiB，同一批文件该多收进来几份。
        //   不盖它的话，调完上限重跑，缓存一口咬定「输入没变」而整条跳过——真库上那是
        //   108 份永远收不进来的媒体，而且只有整份 `--refresh` 才逃得掉。
        let mut parts: Vec<String> = vec![format!(
            "上限={}",
            subject
                .media_limit
                .map_or(-1, |cap| i64::try_from(cap).unwrap_or(i64::MAX))
        )];
        parts.extend(subject.media.iter().map(|media| {
            format!(
                "{}|{}|{}",
                media.key,
                media
                    .bytes
                    .map_or(-1, |bytes| i64::try_from(bytes).unwrap_or(i64::MAX)),
                media.mtime.unwrap_or(-1)
            )
        }));
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        Some(super::fingerprint(&refs))
    }

    fn collect(&self, subject: &Subject<'_>, out: &mut Harvest) -> Result<(), Failure> {
        for media in subject.media {
            out.picture(MediaClaim {
                kind: media.kind,
                from: MediaFrom::Library(media.key.clone()),
                bytes: media.bytes,
                why: format!("{}：{}", media.why, media.key),
            });
        }
        Ok(())
    }
}

/// 中立库里一条文件记录，本地媒体的归属规则要看的那三样。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFact {
    /// 键。
    pub key: String,
    /// 字节数；元数据读不到时是 `None`（ADR-0021）。
    pub bytes: Option<u64>,
    /// 修改时间；读不到时是 `None`。
    pub mtime: Option<i64>,
}

/// 把主库里能归给某个变体的媒体挑出来：变体的键 → 归给它的那几份。
///
/// # Errors
/// 读库失败时返回错误。
pub fn index(
    catalog: &Catalog,
    variants: &[VariantRow],
) -> Result<BTreeMap<String, Vec<LocalMedia>>, CatalogError> {
    let mut files: Vec<FileFact> = Vec::new();
    catalog.for_each_file(&mut |key, len, mtime| {
        if normalized_ext(extension_of(key)).is_some() {
            files.push(FileFact {
                key: key.to_string(),
                bytes: len,
                mtime,
            });
        }
    })?;
    Ok(match_media(variants, &files))
}

/// 归属规则本身。**纯函数**——这样两条规则可以完全在内存里测，不必造一个库。
#[must_use]
pub fn match_media(
    variants: &[VariantRow],
    files: &[FileFact],
) -> BTreeMap<String, Vec<LocalMedia>> {
    // 主文件也可能长着媒体的扩展名（真库里没有，但规则不该指望这一点）。
    // 一份内容不能既是变体本身又是它的封面。
    let mains: BTreeSet<&str> = variants.iter().map(|v| v.main_key.as_str()).collect();

    // 一个目录里有几个变体的主文件。**大于一就整个目录都不认。**
    let mut hosts: BTreeMap<&str, usize> = BTreeMap::new();
    for variant in variants {
        *hosts.entry(dir_of(&variant.main_key)).or_default() += 1;
    }

    let mut by_dir: BTreeMap<&str, Vec<&FileFact>> = BTreeMap::new();
    for file in files {
        if mains.contains(file.key.as_str()) {
            continue;
        }
        by_dir.entry(dir_of(&file.key)).or_default().push(file);
    }

    let mut out: BTreeMap<String, Vec<LocalMedia>> = BTreeMap::new();
    for variant in variants {
        let dir = dir_of(&variant.main_key);
        let Some(here) = by_dir.get(dir) else {
            continue;
        };
        let sole = hosts.get(dir).copied().unwrap_or(0) == 1;
        let stem = stem_of(file_name_of_key(&variant.main_key));
        let mut picked: Vec<LocalMedia> = Vec::new();
        for file in here {
            let name = file_name_of_key(&file.key);
            let same_name = stem_of(name).eq_ignore_ascii_case(stem);
            let why = if same_name {
                "同名兄弟"
            } else if sole {
                "独占目录"
            } else {
                continue;
            };
            picked.push(LocalMedia {
                key: file.key.clone(),
                bytes: file.bytes,
                mtime: file.mtime,
                kind: kind_of(name),
                why,
            });
        }
        if !picked.is_empty() {
            picked.sort_by(|a, b| a.key.cmp(&b.key));
            out.insert(variant.key.clone(), picked);
        }
    }
    out
}

/// 这份媒体是什么。**认不出就说认不出**——猜错了导出时会把攻略截图当封面铺出去。
#[must_use]
pub fn kind_of(name: &str) -> MediaKind {
    let ext = normalized_ext(extension_of(name)).unwrap_or("");
    if matches!(ext, "mp4" | "mkv" | "webm" | "mov" | "avi") {
        return MediaKind::Video;
    }
    let stem = stem_of(name).to_lowercase();
    const COVER: &[&str] = &["封面", "cover", "box", "front", "盒", "包装"];
    const SHOT: &[&str] = &["截图", "screen", "snap", "shot", "预览", "游戏画面"];
    if COVER.iter().any(|word| stem.contains(word)) {
        return MediaKind::Cover;
    }
    if SHOT.iter().any(|word| stem.contains(word)) {
        return MediaKind::Screenshot;
    }
    // `1.jpg` / `2.jpg` 这种纯数字名在这个库里是成组的预览图（实测 FC 与 GB 的
    // 汉化目录里遍地都是），当截图。**不当封面**——猜错方向的代价不对称：
    // 少一张封面只是缺，把截图当封面是错。
    if !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit()) {
        return MediaKind::Screenshot;
    }
    MediaKind::Other
}

fn dir_of(key: &str) -> &str {
    key.rsplit_once('/').map_or("", |(dir, _)| dir)
}

fn stem_of(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 变体(key: &str) -> VariantRow {
        VariantRow {
            key: key.to_string(),
            platform: Some("FC".to_string()),
            rule: "一文件一变体".to_string(),
            main_key: key.to_string(),
            files: 1,
            bytes: 1,
            unreadable_files: 0,
            manual: false,
            work_id: None,
            release_id: None,
        }
    }

    fn 文件(key: &str) -> FileFact {
        FileFact {
            key: key.to_string(),
            bytes: Some(100),
            mtime: Some(7),
        }
    }

    #[test]
    fn 独占目录里的图归那个变体() {
        let variants = vec![变体("FC/魂斗罗汉化/rom.zip")];
        let files = vec![文件("FC/魂斗罗汉化/1.jpg"), 文件("FC/魂斗罗汉化/封面.png")];
        let got = match_media(&variants, &files);
        let picked = &got["FC/魂斗罗汉化/rom.zip"];
        assert_eq!(picked.len(), 2);
        assert_eq!(picked[0].kind, MediaKind::Screenshot);
        assert_eq!(picked[1].kind, MediaKind::Cover);
        assert!(picked.iter().all(|m| m.why == "独占目录"));
    }

    #[test]
    fn 目录里有两个变体就一张都不认() {
        let variants = vec![变体("FC/一堆/甲.zip"), 变体("FC/一堆/乙.zip")];
        let files = vec![文件("FC/一堆/1.jpg")];
        assert!(match_media(&variants, &files).is_empty());
    }

    #[test]
    fn 同名兄弟不受目录里有几个变体影响() {
        let variants = vec![变体("FC/一堆/甲.zip"), 变体("FC/一堆/乙.zip")];
        let files = vec![文件("FC/一堆/甲.png"), 文件("FC/一堆/无关.jpg")];
        let got = match_media(&variants, &files);
        assert_eq!(got.len(), 1);
        assert_eq!(got["FC/一堆/甲.zip"][0].key, "FC/一堆/甲.png");
        assert_eq!(got["FC/一堆/甲.zip"][0].why, "同名兄弟");
    }

    #[test]
    fn 目录树转储内部的图够不着() {
        // PSV 的目录树转储：主文件是那个目录本身，图躺在更深的地方。
        let mut variant = 变体("PSV/游戏[PCSG00299]");
        variant.rule = "PSV 目录树".to_string();
        let files = vec![文件("PSV/游戏[PCSG00299]/app/media/bg.png")];
        assert!(match_media(&[variant], &files).is_empty());
    }

    #[test]
    fn 视频认得出来() {
        assert_eq!(kind_of("视频预览.mp4"), MediaKind::Video);
        assert_eq!(kind_of("封面.jpg"), MediaKind::Cover);
        assert_eq!(kind_of("3.jpg"), MediaKind::Screenshot);
        assert_eq!(kind_of("攻略.jpg"), MediaKind::Other);
    }

    #[test]
    fn 文件名折出来的标题不带标记组() {
        assert_eq!(
            title_from_filename("精灵宝可梦 银[简正确精灵名](完美LOGO).7z"),
            "精灵宝可梦 银"
        );
        assert_eq!(title_from_filename("恋爱复仇战[PCSG00718]"), "恋爱复仇战");
        assert_eq!(title_from_filename("1942 (JU).zip"), "1942");
        assert_eq!(title_from_filename("[合集].zip"), "[合集]");
    }
}
