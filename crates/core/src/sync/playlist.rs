//! 多碟变体同步到卡上时多生成一份 **`.m3u` 播放列表**（票 `verdict-store-and-sync/11`）。
//!
//! 一套多碟游戏要在模拟器里换碟，得拿播放列表启动，模拟器才知道还有第二张、第三张碟可换。这一份按碟序列出
//! 每张碟的主文件，路径相对播放列表自己所在的目录。
//!
//! ## 前端条目拿它启动
//!
//! 票 `verdict-store-and-sync/18`：卡上生成了播放列表的那个变体，前端条目改指它，几张碟本身不再各成一条
//! （[`Laid::launching`] 交出 [`Launch`]，收敛照它写条目、铺媒体照它起名）。拿主意的人 2026-10-04 裁的两条路：
//!
//! - **ES-DE**：`<path>` 指播放列表，封面照播放列表的名字铺（ES-DE 照 `<path>` 那份文件的名字找媒体），每张碟的主文件
//!   各写一条 `<hidden>true</hidden>`。碟**不挪地方**——卡上的布局照旧照搬键。ES-DE 的「Show hidden games」默认开着，
//!   藏起来的那几条默认只是变淡，要用户在 ES-DE 里关一次才真的看不见（挂单 `Q1827`）。
//! - **Pegasus**：`files:` 里那个变体那一行写播放列表。Pegasus 拿 `{file.path}` 交给启动命令，指什么交什么；一个文件
//!   不弹挑选框。
//!
//! **放不进目标的播放列表不在卡上，条目照旧指头一张碟**——条目指着一份卡上没有的文件，比指头一张碟更糟。所以
//! [`Laid::launching`] 只交筛过之后还在的那几份。没生成播放列表的那几套（档案说不吃 `.m3u`、有一张碟放不进或落成
//! 吃不下的形态）照旧指头一张碟，别的碟在前端里点不到（挂单 `Q1828`）。
//!
//! ## 只在同步这一侧生成
//!
//! **导出到主库那一侧不生成**：导出铺在主库上，而主库里只许多出元数据文件与媒体目录（ADR-0004），一份 `.m3u`
//! 两样都不是。同步的目标是卡（ADR-0015）：播放列表与前端元数据一样是**生成物**，清单照管——进清单、换了内容就重写、
//! 那个变体移出子库就跟着删掉，全是计划器对一切 [`DesiredFile`] 一视同仁的那一套，这里一行都不另写。
//!
//! ## 生不生成，三处各答一问
//!
//! - **这个变体是不是多碟、各张碟的主文件是哪几份、按什么次序**：成型那一处答（[`shape::discs`]）。
//! - **这个前端用不用得上播放列表**：适配器答（[`Adapter::uses_playlists`]）。
//! - **这个平台的模拟器吃不吃**：能力档案答，判的是「吃不吃这个扩展名」那同一处（[`Accepts::takes_key`]，
//!   ADR-0017）——`.m3u` 本身要吃，列进去的每一张碟也要吃。档案对这个平台不作声称时照样生成：不作声称说的是
//!   「没查过」，不是「吃不下」。
//!
//! ## 列的是卡上真落着、模拟器吃得下的那几份
//!
//! 播放列表里每一行是那张碟的主文件**在卡上的落点**，取自期望状态，不取主库里的键：主文件要转格式的，卡上躺的是
//! 转出来那一份（[`DesiredFile::convert`]）。所以它排在**按目标存储筛过一遍之后**。一份指着卡上没有、或者模拟器
//! 读不了的文件的播放列表，启动那一刻才报错，比没有更糟，于是两种都不生成：
//!
//! - 有一张碟**放不进目标**（太大、文件名不收、撞车）：那张碟本身照旧报在差量预览「放不进目标」那一栏里。
//! - 有一张碟落到卡上是**档案说这个平台吃不下的形态**：多碟变体只有头一张碟照档案转格式，别的碟原样搬（挂单
//!   `Q1650`），PS1 两张 `.zip` 碟在只吃裸镜像的档案下第二张就是这样（差量预览的「转不了」只判主文件，眼下
//!   不报它，同一条挂单）。
//!
//! 只看每张碟的主文件：`.cue` 引的那几份 `.bin` 落没落下，是那张碟自己的事，与播放列表无关。

use std::collections::BTreeMap;

use crate::adapter::Adapter;
use crate::adapter::converge::Launch;
use crate::capability::{Accepts, Profile};
use crate::path;
use crate::platform::Manifest as PlatformManifest;
use crate::shape::{self, Role};

use super::{Desired, DesiredFile, FileKind, Footprint};

/// 播放列表的扩展名。
pub const EXTENSION: &str = "m3u";

/// 折出来的播放列表。
#[derive(Debug, Clone, Default)]
pub struct Laid {
    /// 目标上该有的播放列表，按路径排。
    pub files: Vec<DesiredFile>,
    /// 相对子库根的路径 → 那份播放列表的字节。与前端元数据一样**直接写它**：主库里没有源文件。
    pub bytes: BTreeMap<String, Vec<u8>>,
    /// 变体的键 → 它的条目在卡上启动哪一份：那份播放列表，与它列的那几张碟。**还没筛过**，所以不交出去：
    /// 交给收敛与铺媒体的是 [`Self::launching`]。
    launch: BTreeMap<String, Launch>,
}

impl Laid {
    /// 哪几个变体的条目在卡上启动播放列表：**照筛过之后的期望状态**，只交播放列表还在 `desired` 里的那几份。
    ///
    /// 播放列表折出来之后要照文件系统声明再筛一遍（[`Desired::screen`]）——两套多碟游戏的播放列表撞在同一条路径上，
    /// 两份都不放行。那时条目还指着它，前端里点下去就是一份卡上没有的文件；所以收敛与铺媒体读的是这一份，不是没筛过的那一份。
    #[must_use]
    pub fn launching(&self, desired: &Desired) -> BTreeMap<String, Launch> {
        let kept: std::collections::BTreeSet<&str> = desired
            .files
            .iter()
            .filter(|file| file.kind == FileKind::Playlist)
            .map(|file| file.path.as_str())
            .collect();
        self.launch
            .iter()
            .filter(|(_, launch)| kept.contains(launch.file.as_str()))
            .map(|(variant, launch)| (variant.clone(), launch.clone()))
            .collect()
    }
}

/// 给这个子库里的多碟变体各折一份播放列表（判据见模块文档）。
///
/// `footprint` 是这份选择集在主库里的脚印（成员连身份、平台），`desired` 是**按目标存储筛过一遍之后**的期望状态
/// ——碟在卡上落在哪取自它。`platform_manifest` 是平台清单：认各张碟走的是成型那一份规则。
///
/// **纯函数**：不碰磁盘、不读库。
#[must_use]
pub fn lay(
    footprint: &Footprint,
    desired: &Desired,
    adapter: &dyn Adapter,
    profile: &Profile,
    platform_manifest: &PlatformManifest,
) -> Laid {
    let mut out = Laid::default();
    if !adapter.uses_playlists() {
        return out;
    }
    // 主库里的键 → 它在卡上的落点。只有 ROM 那一类有主库里的键；放不进目标的已经不在这里了。
    let landing: BTreeMap<&str, &str> = desired
        .files
        .iter()
        .filter(|file| file.kind == FileKind::Rom)
        .map(|file| (file.source.as_str(), file.path.as_str()))
        .collect();
    let mut by_variant: BTreeMap<&str, Vec<(String, Role)>> = BTreeMap::new();
    for member in footprint.members.iter().filter(|member| member.is_file) {
        by_variant
            .entry(member.variant_key.as_str())
            .or_default()
            .push((member.key.clone(), member.role));
    }
    for (variant, members) in &by_variant {
        let platform = footprint.platforms.get(*variant).and_then(Option::as_deref);
        // 档案对这个平台吃什么：没有那一条、或者不作声称，就当吃（`Accepts::Anything`）。
        let accepts = profile
            .matrix
            .entry_for(platform)
            .map_or(&Accepts::Anything, |entry| &entry.accepts);
        if !accepts.takes(EXTENSION) {
            continue;
        }
        let discs = shape::discs(members, platform_manifest);
        let Some(paths) = discs
            .iter()
            .map(|key| landing.get(key.as_str()).copied())
            .collect::<Option<Vec<&str>>>()
        else {
            continue;
        };
        if !paths.iter().all(|at| accepts.takes_key(at)) {
            continue;
        }
        let Some(first) = paths.first() else {
            continue;
        };
        let at = place(first, platform_manifest);
        let bytes = render(&at, &paths);
        // 碟换了落点、换了次序，内容就变、源那一格就变，计划器当场判出要重写（`DesiredFile::generated`）。
        out.files.push(DesiredFile::generated(
            at.clone(),
            FileKind::Playlist,
            &bytes,
            (*variant).to_string(),
        ));
        out.bytes.insert(at.clone(), bytes);
        out.launch.insert(
            (*variant).to_string(),
            Launch {
                file: at,
                hidden: paths.iter().map(ToString::to_string).collect(),
            },
        );
    }
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// 播放列表落在哪：头一张碟那个目录里，名字是那张碟剥掉碟片标记之后的那一截（[`shape::disc_family`]）。
fn place(first: &str, platform_manifest: &PlatformManifest) -> String {
    let name = path::file_name_of_key(first);
    let family = shape::disc_family(name, platform_manifest);
    match first.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{family}.{EXTENSION}"),
        None => format!("{family}.{EXTENSION}"),
    }
}

/// 播放列表的字节：一张碟一行，相对 `at` 所在的目录，`/` 分隔，UTF-8、`\n` 收尾。
fn render(at: &str, discs: &[&str]) -> Vec<u8> {
    let from = at.rsplit_once('/').map_or("", |(dir, _)| dir);
    let mut out = String::new();
    for disc in discs {
        out.push_str(&relative(from, disc));
        out.push('\n');
    }
    out.into_bytes()
}

/// `to`（相对子库根）从目录 `from`（同样相对子库根）看过去的相对路径。
fn relative(from: &str, to: &str) -> String {
    let from: Vec<&str> = from.split('/').filter(|part| !part.is_empty()).collect();
    let to: Vec<&str> = to.split('/').collect();
    let shared = from
        .iter()
        .zip(&to)
        .take_while(|(a, b)| a == b)
        .count()
        // 文件名那一段不算目录：`to` 最后一段永远留着。
        .min(to.len().saturating_sub(1));
    let mut parts: Vec<&str> = vec![".."; from.len() - shared];
    parts.extend(&to[shared..]);
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::gamelist::Gamelist;
    use crate::catalog::MemberFile;
    use crate::sync::Stamp;

    /// 一个变体的脚印：`(键, 身份)` 那几条成员，平台 PS1。
    fn 脚印(variant: &str, members: &[(&str, Role)]) -> Footprint {
        Footprint {
            platforms: BTreeMap::from([(variant.to_string(), Some("PS1".to_string()))]),
            members: members
                .iter()
                .map(|(key, role)| MemberFile {
                    variant_key: variant.to_string(),
                    key: (*key).to_string(),
                    is_file: true,
                    len: Some(1024),
                    mtime_ns: Some(1),
                    role: *role,
                })
                .collect(),
            contents: BTreeMap::new(),
        }
    }

    /// 期望状态：每条成员原样落在剥掉根名的那条路径上；`落点` 里点了名的换成那条落点（转格式那一支）。
    fn 期望(footprint: &Footprint, 落点: &[(&str, &str)]) -> Desired {
        let 换 = BTreeMap::from_iter(落点.iter().copied());
        Desired {
            files: footprint
                .members
                .iter()
                .map(|member| DesiredFile {
                    path: 换.get(member.key.as_str()).map_or_else(
                        || path::relative_of_key(&member.key).to_string(),
                        |at| (*at).to_string(),
                    ),
                    kind: FileKind::Rom,
                    bytes: 1024,
                    unreadable: false,
                    source: member.key.clone(),
                    source_stamp: Stamp {
                        bytes: 1024,
                        mtime_ns: Some(1),
                    },
                    variant: member.variant_key.clone(),
                    convert: None,
                })
                .collect(),
            ..Desired::default()
        }
    }

    fn 折(footprint: &Footprint, desired: &Desired) -> Laid {
        lay(
            footprint,
            desired,
            &Gamelist,
            &Profile::unclaimed(),
            &PlatformManifest::builtin(),
        )
    }

    fn 正文(laid: &Laid, at: &str) -> String {
        String::from_utf8(
            laid.bytes
                .get(at)
                .unwrap_or_else(|| panic!("该有 {at}：{:?}", laid.bytes.keys()))
                .clone(),
        )
        .expect("UTF-8")
    }

    #[test]
    fn 几张碟分在几个目录里_播放列表落在头一张碟旁边_别的碟写成相对它的路径() {
        let footprint = 脚印(
            "库/ps/龙骑士传说[简]Disc A/Legend (Disc 1).chd",
            &[
                ("库/ps/龙骑士传说[简]Disc A/Legend (Disc 1).chd", Role::Main),
                (
                    "库/ps/龙骑士传说[简]Disc B/Legend (Disc 2).chd",
                    Role::Companion,
                ),
            ],
        );
        let laid = 折(&footprint, &期望(&footprint, &[]));
        let at = "ps/龙骑士传说[简]Disc A/Legend.m3u";
        assert_eq!(
            正文(&laid, at),
            "Legend (Disc 1).chd\n../龙骑士传说[简]Disc B/Legend (Disc 2).chd\n"
        );
        assert_eq!(laid.files.len(), 1);
        assert_eq!(laid.files[0].path, at);
        assert_eq!(laid.files[0].kind, FileKind::Playlist);
        assert_eq!(laid.files[0].bytes, laid.bytes[at].len() as u64);
    }

    #[test]
    fn 列的是卡上真落着的那一份_主文件转了格式就列转出来的那一份() {
        let footprint = 脚印(
            "库/ps/某游戏/游戏 (Disc 1).zip",
            &[
                ("库/ps/某游戏/游戏 (Disc 1).zip", Role::Main),
                ("库/ps/某游戏/游戏 (Disc 2).chd", Role::Companion),
            ],
        );
        let laid = 折(
            &footprint,
            &期望(
                &footprint,
                &[(
                    "库/ps/某游戏/游戏 (Disc 1).zip",
                    "ps/某游戏/游戏 (Disc 1).bin",
                )],
            ),
        );
        assert_eq!(
            正文(&laid, "ps/某游戏/游戏.m3u"),
            "游戏 (Disc 1).bin\n游戏 (Disc 2).chd\n"
        );
    }

    #[test]
    fn 有一张碟放不进目标_那一套不生成播放列表() {
        let footprint = 脚印(
            "库/ps/某游戏/游戏 (Disc 1).chd",
            &[
                ("库/ps/某游戏/游戏 (Disc 1).chd", Role::Main),
                ("库/ps/某游戏/游戏 (Disc 2).chd", Role::Companion),
            ],
        );
        let mut desired = 期望(&footprint, &[]);
        // 筛过一遍之后，第二张碟落进了「放不进目标」：它不在期望状态的文件里了。
        desired
            .files
            .retain(|file| !file.source.ends_with("(Disc 2).chd"));
        let laid = 折(&footprint, &desired);
        assert!(laid.files.is_empty(), "{:?}", laid.files);
        assert!(laid.bytes.is_empty());
    }

    #[test]
    fn 有一张碟落到卡上是档案说吃不下的形态_那一套不生成播放列表() {
        // 只有头一张碟照档案转格式（挂单 `Q1650`）：两张 `.zip` 碟在只吃裸镜像的档案下，头一张解成裸文件，
        // 第二张照旧是 `.zip`——播放列表第二行会指着一份模拟器读不了的文件，那就不如不生成。
        let footprint = 脚印(
            "库/ps/某游戏/游戏 (Disc 1).zip",
            &[
                ("库/ps/某游戏/游戏 (Disc 1).zip", Role::Main),
                ("库/ps/某游戏/游戏 (Disc 2).zip", Role::Companion),
            ],
        );
        let desired = 期望(
            &footprint,
            &[(
                "库/ps/某游戏/游戏 (Disc 1).zip",
                "ps/某游戏/游戏 (Disc 1).bin",
            )],
        );
        let 独立模拟器 = crate::capability::Roster::builtin()
            .find("独立模拟器-exfat")
            .expect("内置名册里有这一份")
            .clone();
        let laid = lay(
            &footprint,
            &desired,
            &Gamelist,
            &独立模拟器,
            &PlatformManifest::builtin(),
        );
        assert!(laid.files.is_empty(), "{:?}", laid.files);

        // 两张都落成档案吃得下的形态，就照常生成。
        let desired = 期望(
            &footprint,
            &[
                (
                    "库/ps/某游戏/游戏 (Disc 1).zip",
                    "ps/某游戏/游戏 (Disc 1).bin",
                ),
                (
                    "库/ps/某游戏/游戏 (Disc 2).zip",
                    "ps/某游戏/游戏 (Disc 2).bin",
                ),
            ],
        );
        let laid = lay(
            &footprint,
            &desired,
            &Gamelist,
            &独立模拟器,
            &PlatformManifest::builtin(),
        );
        assert_eq!(laid.files.len(), 1, "{:?}", laid.files);
    }

    #[test]
    fn 条目启动哪一份只交筛过之后还在的播放列表() {
        // 票 `verdict-store-and-sync/18`：条目改指播放列表、几张碟藏起来。可播放列表折出来之后还要再筛一遍（撞车、
        // 名字太长）——筛掉了的那一份不在卡上，条目指着它比指头一张碟更糟。
        let footprint = 脚印(
            "库/ps/某游戏/游戏 (Disc 1).chd",
            &[
                ("库/ps/某游戏/游戏 (Disc 1).chd", Role::Main),
                ("库/ps/某游戏/游戏 (Disc 2).chd", Role::Companion),
            ],
        );
        let mut desired = 期望(&footprint, &[]);
        let laid = 折(&footprint, &desired);
        desired.files.extend(laid.files.iter().cloned());
        assert_eq!(
            laid.launching(&desired),
            BTreeMap::from([(
                "库/ps/某游戏/游戏 (Disc 1).chd".to_string(),
                Launch {
                    file: "ps/某游戏/游戏.m3u".to_string(),
                    hidden: vec![
                        "ps/某游戏/游戏 (Disc 1).chd".to_string(),
                        "ps/某游戏/游戏 (Disc 2).chd".to_string(),
                    ],
                },
            )])
        );

        // 再筛一遍时它被挡下了（不在期望状态里了）：条目照旧指头一张碟。
        desired.files.retain(|file| file.kind != FileKind::Playlist);
        assert!(laid.launching(&desired).is_empty());
    }

    #[test]
    fn 碟的落点一变_源那一格跟着变_计划器判得出要重写() {
        let footprint = 脚印(
            "库/ps/某游戏/游戏 (Disc 1).chd",
            &[
                ("库/ps/某游戏/游戏 (Disc 1).chd", Role::Main),
                ("库/ps/某游戏/游戏 (Disc 2).chd", Role::Companion),
            ],
        );
        let 原样 = 折(&footprint, &期望(&footprint, &[]));
        let 换了 = 折(
            &footprint,
            &期望(
                &footprint,
                &[(
                    "库/ps/某游戏/游戏 (Disc 2).chd",
                    "ps/某游戏/游戏 (Disc 2).cue",
                )],
            ),
        );
        assert_eq!(原样.files[0].path, 换了.files[0].path);
        assert_ne!(原样.files[0].source, 换了.files[0].source);
    }
}
