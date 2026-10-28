//! 平台清单「先 `--manifest`、再工作目录那份、再内置」那条链（票 `core-answers-once/04`，收挂单 `Q1031`）。
//!
//! ADR-0024：这条查法只许有一处实现。从前命令行有一份（`ManifestArgs::load`），核心库另有一份
//! （`sources::manifest`），界面七处又各自「一律用内置那份」——同一份库，两个壳看见的平台表不一样。
//!
//! ADR-0021：工作目录里那份**在、但读不动**时不许静默退回内置。这一条是这一票真正修的那个病：
//! 从前那句「在不在」问的是 `Path::exists()`，而它把「读不动」也算成「不在」。
//!
//! 只用临时目录，不碰任何真库。

use std::fs;

use romcat_core::platform::{Manifest, Origin};
use romcat_core::testing::{Revoke, TempDir, revoke, temp_dir};

/// 一份与内置不同的清单：只有一个平台，目录叫「假想机」。
const 自己那一份: &str = r#"
"版本" = 1

[["平台"]]
"名" = "假想机"
"目录" = ["假想机"]
"成型" = []
"#;

/// 另一份，也只有一层：平台叫「另一台」。
const 另一份: &str = r#"
"版本" = 1

[["平台"]]
"名" = "另一台"
"目录" = ["另一台"]
"成型" = []
"#;

/// 一个工作目录，里头搁着一份平台清单。
fn 摆一份(tag: &str, 清单: &str) -> TempDir {
    let 目录 = temp_dir(tag);
    fs::write(目录.path().join("platforms.toml"), 清单).expect("写得下");
    目录
}

#[test]
fn 工作目录里那份平台清单说了算() {
    let 工作目录 = 摆一份("core-清单-工作目录那份", 自己那一份);
    let 挑的 = Manifest::locate(None, 工作目录.path());
    assert!(
        挑的.manifest().platform_named("假想机").is_some(),
        "工作目录里那份清单没生效，用的还是内置那一份",
    );
    assert!(
        挑的.manifest().platform_named("GBA").is_none(),
        "内置清单里的平台也还在：两份合起来了？",
    );
    assert_eq!(挑的.origin(), Origin::InWorkspace);
    assert!(挑的.unreadable().is_none(), "这份读得动，不该说读不动");
}

#[test]
fn 点名的那一份压过工作目录里那一份() {
    // 命令行的 `--manifest` 与工作目录里那份的先后照旧（票 04 验收第 2 条）。
    let 工作目录 = 摆一份("core-清单-点名那份", 自己那一份);
    let 点名的 = 工作目录.path().join("点名.toml");
    fs::write(&点名的, 另一份).expect("写得下");

    let 挑的 = Manifest::locate(Some(&点名的), 工作目录.path());
    assert!(
        挑的.manifest().platform_named("另一台").is_some(),
        "`--manifest` 点名的那一份没压过工作目录里那份",
    );
    assert!(
        挑的.manifest().platform_named("假想机").is_none(),
        "工作目录里那份也生效了：两份合起来了？",
    );
    assert_eq!(挑的.origin(), Origin::Given);
}

#[test]
fn 工作目录里没有这一份时用内置那一份() {
    let 工作目录 = temp_dir("core-清单-没有");
    let 挑的 = Manifest::locate(None, 工作目录.path());
    assert_eq!(挑的.origin(), Origin::Builtin);
    assert!(
        挑的.manifest().platform_named("GBA").is_some(),
        "工作目录里没有这一份时该用内置那份",
    );
    assert!(挑的.unreadable().is_none());
    assert!(
        挑的.unreadable_note().is_none(),
        "没有这一份不是读不动，屏上不该有那句话",
    );
}

#[test]
fn 工作目录里那份读不动时不静默退回内置的() {
    // ADR-0021：读不动是第三态。从前 `Path::exists()` 把「读不动」报成「不在」，
    // 于是整个人改过的那份清单被静默换成了内置那份——屏上一个字都没有。
    let 工作目录 = 摆一份("core-清单-读不动", 自己那一份);
    let Some(_装着) = revoke(工作目录.path(), Revoke::Read) else {
        return;
    };

    let 挑的 = Manifest::locate(None, 工作目录.path());
    let 原因 = 挑的
        .unreadable()
        .unwrap_or_else(|| panic!("读不动的那份被静默换成了内置那一份"));
    assert!(
        挑的.manifest().platform_named("GBA").is_some(),
        "读不动时眼下顶上的是内置那一份（屏上要说清这一点）",
    );
    assert_eq!(
        挑的.origin(),
        Origin::Builtin,
        "眼下用的是哪一份，交出来的就该是哪一份",
    );
    // 屏上那句话要说得出原因，也要说清眼下用的是哪一份。
    let 那句话 = 挑的.unreadable_note().expect("读不动时该有那一句");
    assert!(
        那句话.contains("读不动") && 那句话.contains("内置"),
        "那句话没说清读不动、也没说清眼下用的是内置那一份：{那句话}（{原因:?}）",
    );

    // 命令行与同步那些没处摆这句话的地方照旧停下：不静默退回内置。
    assert!(
        挑的.strict().is_err(),
        "读不动的那份该让命令行停下，而不是拿内置的顶上",
    );
    assert!(
        romcat_core::sources::manifest(工作目录.path()).is_err(),
        "`sources::manifest` 也把读不动报成了「没有」",
    );
}
