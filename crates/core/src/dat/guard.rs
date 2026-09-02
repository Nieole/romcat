//! **取数纪律的闸门**：URL 在发出去之前先过这里。
//!
//! 这两条纪律写在注释里是不够的——[`registry`](super::registry) 那份数据源清单是
//! **数据**，用户可以整份换掉，而换掉的那一份完全可能把 `datomatic` 写回去。因此闸门
//! 是一个**纯函数**，同时挂在三处：
//!
//! - [`sync::plan`](super::sync::plan) 排计划时逐条查，于是「会不会碰到禁区」在
//!   一个字节都还没发出去的时候就是可断言的事实；
//! - [`HttpFetcher`](super::fetch::HttpFetcher) 每一跳（**含重定向落到的每一跳**）
//!   再查一遍，兜住计划之外的路径；
//! - **在线刮削档**（[`scrape::online`](crate::scrape::online)）发查询前查一遍，
//!   而且**响应里给回来的每一个媒体 URL 也查一遍**——那些 URL 是**服务器说了算**的，
//!   一条被改过的响应就能把请求送到禁区去。闸门是这个库里唯一一处「可以连哪儿」的
//!   声明，在线源不另开一条路。
//!
//! ## 挡什么
//!
//! | 挡掉 | 为什么 |
//! |---|---|
//! | `datomatic.no-intro.org` | 一次参数畸形的请求就触发**永久 IP 封禁**，解封要发邮件求人。它的 `robots.txt` 声称允许抓取，**不可作为安全依据**（ADR-0007） |
//! | `redump.org` / `old.redump.info` | 2026-06-20 起的**冻结镜像**：60 个系统 vs 现站 106 个，且会以「系统存在但 No discs found」的形式**误导**（ADR-0007 修订段） |
//! | `api.github.com/…/contents/…` | 该接口**在 1000 条处硬截断且不报错**。调研中因此误判 TOSEC 缺少多个平台集，改用 `git/trees?recursive=1` 才拿到完整的 4502 个 DAT |
//! | 不在名单上的主机 | 白名单而不是黑名单：新写一条数据源时，忘了想「这个站能不能抓」会被当场拦下，而不是等到被封 |
//! | 非 `https` | 顺带把 `http://redump.org/...` 这种旧写法一并挡掉 |

/// 允许连的主机。**白名单**——不在这里的一律拒。
///
/// GitHub 那几个是同一件事的不同落点：API 在 `api.github.com`，发行资产的下载会
/// 302 到 `release-assets.githubusercontent.com`（**GitHub 现在实际发出去的那一个**）
/// 或者 `objects.githubusercontent.com`，仓库里的单个文件在 `raw.githubusercontent.com`。
/// 重定向的每一跳都要过这道闸门，所以落点也得在名单里。
pub const ALLOWED_HOSTS: &[&str] = &[
    // Redump 的**现行**域名。旧的 `redump.org` 在下面被显式拒。
    "redump.info",
    "api.github.com",
    "github.com",
    // GitHub 发行资产的两个下载主机。`github.com/.../releases/download/...` 会 302
    // 到它们中的一个，而**闸门看每一跳**（`fetch::follow`），所以两个都得在名单上。
    // `release-assets.` 是 GitHub 现在实际发出去的那一个：2026-09-01 取 No-Intro 那个
    // 整包时实测就是它，少了它 No-Intro 整档一件都取不回来。
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "raw.githubusercontent.com",
    "codeload.github.com",
    // **在线刮削档**（票 14）：ScreenScraper 的 API 与它的媒体落点。
    // `api.` 是 `jeuInfos.php` / `mediaJeu.php` 的家，`www.` 是响应里那些媒体 URL
    // 常见的另一个落点。两个都得在名单上，因为**媒体 URL 是服务器给的**——闸门
    // 逐个查它们，名单之外的一律不下。
    "api.screenscraper.fr",
    "www.screenscraper.fr",
    // **模型推断兜底**（票 12）：`https://api.anthropic.com/v1/messages`。
    // 只有这一个主机，而且这一层**只 POST 不下载**——它不会像刮削那样从响应里
    // 拿到一串新的 URL 再去取，所以名单上不必为它留第二个落点。
    "api.anthropic.com",
];

/// 明确点名拒绝的主机。它们本来就不在白名单里，单列一份是为了**说得出理由**——
/// 报错里只说「不在名单上」，下一个人只会把它加进名单。
const REFUSED_HOSTS: &[(&str, Refusal)] = &[
    ("datomatic.no-intro.org", Refusal::Datomatic),
    ("redump.org", Refusal::FrozenRedump),
    ("www.redump.org", Refusal::FrozenRedump),
    ("old.redump.info", Refusal::FrozenRedump),
];

/// 闸门为什么拦下这个 URL。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// 指向 No-Intro 的 DAT 生成站点。
    #[error(
        "拒绝连 datomatic.no-intro.org：一次参数畸形的请求就会触发永久 IP 封禁，\
         解封要发邮件求人（ADR-0007）。No-Intro 的 DAT 走 GitHub 每日镜像。"
    )]
    Datomatic,
    /// 指向 Redump 的冻结旧镜像。
    #[error(
        "拒绝连 Redump 的冻结镜像：redump.org 自 2026-06-20 起不再更新，\
         只有 60 个系统（现站 106 个），且会以「系统存在但 No discs found」误导。\
         走 redump.info（ADR-0007 修订段）。"
    )]
    FrozenRedump,
    /// 从那个每日镜像里取 Redump 的数据。
    #[error(
        "拒绝从 {repo} 取 {asset}：那个生成器的 redump.py 硬编码了已冻结的 redump.org，\
         产出的 redump.xml / redump.zip 是 60 个系统的旧数据（现站 106 个）。\
         Redump 走 redump.info（ADR-0007 修订段）。"
    )]
    MirrorRedumpAsset {
        /// 哪个仓库。
        repo: String,
        /// 哪个资产。
        asset: String,
    },
    /// 用了会静默截断的那个接口。
    #[error(
        "拒绝用 GitHub 的 /contents 接口：它在 1000 条处硬截断且不报错，\
         调研中因此误判 TOSEC 缺少多个平台集。列举仓库文件用 git/trees?recursive=1。"
    )]
    ContentsApi,
    /// 主机不在白名单上。
    #[error(
        "主机 {host} 不在允许名单上。要新加一个数据源，先想清楚那个站能不能抓，再把它写进 ALLOWED_HOSTS。"
    )]
    NotAllowed {
        /// 被拒的主机。
        host: String,
    },
    /// URL 根本不像个 URL，或者不是 `https`。
    #[error("{url} 不是一个 https URL")]
    Malformed {
        /// 被拒的 URL。
        url: String,
    },
}

/// 那个每日镜像里，哪些资产不许碰。
///
/// 「Redump 不走那个镜像」这条纪律在 URL 上看不出来——`github.com/…/redump.zip`
/// 的主机与 `no-intro.zip` 一模一样，闸门的白名单拦不住它。真正的判据是**资产名**，
/// 所以这一条单独查一次。
///
/// # Errors
/// 资产是那个镜像产出的 Redump 数据时返回 [`Refusal::MirrorRedumpAsset`]。
pub fn check_asset(repo: &str, asset: &str) -> Result<(), Refusal> {
    let folded = asset.to_ascii_lowercase();
    if repo.to_ascii_lowercase().contains("auto-datfile-generator") && folded.starts_with("redump")
    {
        return Err(Refusal::MirrorRedumpAsset {
            repo: repo.to_string(),
            asset: asset.to_string(),
        });
    }
    Ok(())
}

/// 拆出 `https://` URL 的主机与路径（含查询串）。
///
/// 只认 `https`：明文 `http` 一律不走，顺带把 `http://redump.org/...` 这种旧写法
/// 一起挡掉。用户信息（`user@host`）与端口都从主机里剥掉再比对，免得
/// `datomatic.no-intro.org:443` 或 `x@datomatic.no-intro.org` 从名单边上绕过去。
fn split(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("https://")?;
    let (authority, path) = match rest.find('/') {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, "/"),
    };
    let authority = match authority.rfind('@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    let host = authority.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some((host, path))
}

/// 这个 URL 能不能发出去。放行时返回它的主机。
///
/// # Errors
/// 命中任何一条纪律、或主机不在白名单上时返回 [`Refusal`]。
pub fn check(url: &str) -> Result<&str, Refusal> {
    let Some((host, path)) = split(url) else {
        return Err(Refusal::Malformed {
            url: url.to_string(),
        });
    };
    let folded = host.to_ascii_lowercase();
    for (refused, why) in REFUSED_HOSTS {
        if folded == *refused || folded.ends_with(&format!(".{refused}")) {
            return Err(why.clone());
        }
    }
    // `/contents` 那条只对 GitHub 的 API 成立；别的站上这个词是普通路径。
    if folded == "api.github.com" && path.split('?').next().is_some_and(is_contents_path) {
        return Err(Refusal::ContentsApi);
    }
    if !ALLOWED_HOSTS.contains(&folded.as_str()) {
        return Err(Refusal::NotAllowed { host: folded });
    }
    Ok(host)
}

/// `/repos/<主>/<仓库>/contents/...` 长这样。
///
/// 只认「路径里有一段正好是 `contents`」，不做子串匹配——某个仓库真叫
/// `contents` 时，`/repos/x/contents/git/trees/...` 不该被误伤。
fn is_contents_path(path: &str) -> bool {
    let mut segments = path.split('/').filter(|s| !s.is_empty());
    segments.next() == Some("repos")
        && segments.next().is_some()
        && segments.next().is_some()
        && segments.next() == Some("contents")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 绝不连_datomatic() {
        for url in [
            "https://datomatic.no-intro.org/index.php?page=download",
            "https://datomatic.no-intro.org/",
            "https://DATOMATIC.No-Intro.ORG/stuff/schema_nointro_datfile_v4.xsd",
            // 端口与用户信息都不该成为绕过的办法
            "https://datomatic.no-intro.org:443/index.php",
            "https://someone@datomatic.no-intro.org/index.php",
        ] {
            assert_eq!(check(url), Err(Refusal::Datomatic), "{url}");
        }
    }

    #[test]
    fn github_发行资产的两个落点都在名单上() {
        // `github.com/.../releases/download/...` 会 302 到这两个之一，而闸门看每一跳。
        // 少了 `release-assets.` 那一个，No-Intro 整档一件都取不回来——票 07 第一次
        // 真机取数就撞上了这个（那 48 份 DAT、76,429 条条目是第一命中层的主力弹药）。
        assert!(check("https://release-assets.githubusercontent.com/x/no-intro.zip").is_ok());
        assert!(check("https://objects.githubusercontent.com/x/no-intro.zip").is_ok());
    }

    #[test]
    fn 绝不连_redump_的冻结镜像() {
        for url in [
            "https://redump.org/datfile/psx/",
            "https://www.redump.org/downloads/",
            "https://old.redump.info/",
        ] {
            assert_eq!(check(url), Err(Refusal::FrozenRedump), "{url}");
        }
        // 明文那条同样要挡——旧站的写法正是 http://redump.org/
        assert!(matches!(
            check("http://redump.org/datfile/psx/"),
            Err(Refusal::Malformed { .. })
        ));
    }

    #[test]
    fn redump_的现行域名放行() {
        assert_eq!(check("https://redump.info/datfile/PSX"), Ok("redump.info"));
        assert_eq!(check("https://redump.info/downloads/"), Ok("redump.info"));
    }

    #[test]
    fn 会静默截断的那个接口不许用() {
        assert_eq!(
            check("https://api.github.com/repos/smesgr9000/TOSEC-DAT/contents/TOSEC"),
            Err(Refusal::ContentsApi)
        );
        assert_eq!(
            check(
                "https://api.github.com/repos/TASEmulators/BizHawk/contents/Assets/gamedb?ref=master"
            ),
            Err(Refusal::ContentsApi)
        );
        // 换成 trees 就放行——这正是调研里那个修正
        assert!(
            check("https://api.github.com/repos/smesgr9000/TOSEC-DAT/git/trees/abc?recursive=1")
                .is_ok()
        );
    }

    #[test]
    fn 镜像里的_redump_资产不许碰() {
        // 这条在 URL 上看不出来：`github.com/…/redump.zip` 与 `no-intro.zip` 同主机。
        // 判据是资产名——那个生成器的 redump.py 硬编码了已冻结的旧域名。
        let refused =
            check_asset("hugo19941994/auto-datfile-generator", "redump.zip").expect_err("该拒");
        assert!(matches!(refused, Refusal::MirrorRedumpAsset { .. }));
        assert!(
            check_asset(
                "hugo19941994/auto-datfile-generator",
                "redump_parent-clone.xml"
            )
            .is_err()
        );
        // 同一个镜像的 No-Intro 资产照常放行——「绝不直连 datomatic」正靠它。
        assert!(check_asset("hugo19941994/auto-datfile-generator", "no-intro.zip").is_ok());
    }

    #[test]
    fn 在线刮削的两个落点在名单上_别的站点一律拒() {
        // **媒体 URL 是服务器给的**，闸门是唯一挡得住「响应把我们送到别处」的东西。
        assert!(check("https://api.screenscraper.fr/api2/jeuInfos.php?crc=50ABC90A").is_ok());
        assert!(check("https://www.screenscraper.fr/image.php?gameid=1&media=box-2D").is_ok());
        // 一条被改过的响应把媒体 URL 指到禁区——一次请求就够触发永久封禁。
        assert_eq!(
            check("https://datomatic.no-intro.org/media/box.png"),
            Err(Refusal::Datomatic)
        );
        // 名单之外的图床同样不下：白名单不是黑名单。
        assert!(matches!(
            check("https://cdn.example.com/box.png"),
            Err(Refusal::NotAllowed { .. })
        ));
    }

    #[test]
    fn 白名单之外一律拒() {
        let refused = check("https://example.com/dat.zip").unwrap_err();
        assert!(matches!(refused, Refusal::NotAllowed { .. }));
        assert!(format!("{refused}").contains("example.com"));
    }

    #[test]
    fn 名单里没有任何_no_intro_的生成站点() {
        // 白名单是这份代码里唯一一处「可以连哪儿」的声明。
        // 它要是混进 no-intro.org，上面那条点名拒绝就形同虚设了。
        for host in ALLOWED_HOSTS {
            assert!(!host.contains("no-intro"), "{host}");
            assert!(!host.contains("redump.org"), "{host}");
        }
    }
}
