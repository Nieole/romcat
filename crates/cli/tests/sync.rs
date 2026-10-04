//! 端到端验收 `romcat sublibrary plan`：**差量预览**在命令行上出得来，
//! 而且这条命令**一个文件都不写**。
//!
//! 计划器本身由 `romcat-core` 那一侧验（`crates/core/tests/sync.rs` 与 `sync` 的
//! 单元测试）。这个文件验的是命令行这一层：目标不在位时停得住、手动拷进去的东西
//! 一条都不出现在计划里、`--json` 出的**就是计划本身**。
//!
//! 工作目录一律显式指到临时目录：绝不能让测试往开发者真实的
//! `~/.local/share/romcat` 里写东西。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

fn romcat(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .args(["--workspace", &workspace.display().to_string()])
        .output()
        .expect("能启动 romcat")
}

fn 子库(workspace: &Path, args: &[&str]) -> std::process::Output {
    let mut all = vec!["sublibrary"];
    all.extend_from_slice(args);
    all.extend_from_slice(&["--library", "测试库"]);
    romcat(workspace, &all)
}

fn 出来的话(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// 数一数一棵目录树底下有几个文件。用来证明这条命令没往目标上写东西。
fn 文件数(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            let path = entry.path();
            if path.is_dir() { 文件数(&path) } else { 1 }
        })
        .sum()
}

/// 一份小 fixture 主库 + 一个配好规则的子库 + 一个当目标设备用的空目录。
fn 现场() -> (TempDir, TempDir, TempDir) {
    let library = temp_dir("sync-cli-lib");
    let workspace = temp_dir("sync-cli-ws");
    let target = temp_dir("sync-cli-card");
    写(&library.path().join("FC/魂斗罗.zip"), &zip(2048));
    写(&library.path().join("FC/超级玛丽.zip"), &zip(4096));
    写(&library.path().join("GB/口袋妖怪.zip"), &zip(8192));
    let out = romcat(
        workspace.path(),
        &[
            "scan",
            "--root-name",
            "库",
            &library.path().display().to_string(),
            "--library",
            "测试库",
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let out = 子库(
        workspace.path(),
        &[
            "set",
            "掌机",
            "--target",
            &target.path().display().to_string(),
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let out = 子库(workspace.path(), &["rule", "掌机", "--add", "平台=FC"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    (library, workspace, target)
}

#[test]
fn 差量预览出得来_而且一个文件都没写() {
    let (_library, workspace, target) = 现场();
    let out = 子库(workspace.path(), &["plan", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("差量预览"), "{text}");
    assert!(text.contains("新增"), "{text}");
    assert!(text.contains("净变化"), "{text}");
    assert!(text.contains("一个文件都没写"), "{text}");
    // 头一次同步：清单是空的，删除项无从长出。
    assert!(text.contains("清单里 0 个文件"), "{text}");
    assert_eq!(文件数(target.path()), 0, "**排计划不搬任何文件**");
}

#[test]
fn 手动拷进目标的东西一条都不出现在计划里() {
    let (_library, workspace, target) = 现场();
    写(&target.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    写(&target.path().join("cheats/金手指.txt"), b"unlimited lives");
    写(&target.path().join("screenshots/一.png"), &[7u8; 64]);

    let json = workspace.path().join("计划.json");
    let out = 子库(
        workspace.path(),
        &["plan", "掌机", "--json", &json.display().to_string()],
    );
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("清单之外"), "{text}");
    assert!(text.contains("3 个文件"), "{text}");

    // `--json` 出的**就是计划本身**，不是另算的一份。
    let plan: serde_json::Value =
        serde_json::from_slice(&fs::read(&json).expect("读得出")).expect("是 JSON");
    let steps = plan["steps"].as_array().expect("有步骤");
    assert!(!steps.is_empty(), "{plan}");
    for step in steps {
        let path = step["path"].as_str().expect("有路径");
        assert!(
            !path.starts_with("saves/")
                && !path.starts_with("cheats/")
                && !path.starts_with("screenshots/"),
            "{path} 混进了计划"
        );
        assert_eq!(step["act"], "Add", "清单是空的，只可能有新增");
    }
    assert_eq!(plan["deletes"]["files"], 0);
    assert_eq!(plan["strangers"], 3);
    assert_eq!(
        plan["net_bytes"].as_i64().expect("是数"),
        plan["adds"]["bytes"].as_i64().expect("是数"),
        "净变化就是新增那些",
    );
    assert_eq!(
        文件数(target.path()),
        3,
        "目标上还是那三个，一个没多一个没少"
    );
}

#[test]
fn 目标不在位时停住并说清怎么办() {
    let (_library, workspace, target) = 现场();
    let 不在了 = target.path().join("没插上");
    let out = 子库(
        workspace.path(),
        &["plan", "掌机", "--target", &不在了.display().to_string()],
    );
    let text = 出来的话(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("目标未连接"), "{text}");
    assert!(text.contains("插上读卡器"), "{text}");
    // 核心那句只说事实；改目标路径的那条命令由命令行补（挂单 `Q851`），点得出是哪个子库。
    assert!(
        text.contains("romcat sublibrary set 掌机 --target"),
        "命令行该补上改目标路径的命令：{text}"
    );
}

#[test]
fn 子库记着这一版没带的前端格式时_说清是哪个格式_补上换一个的命令() {
    // 挂单 `Q797`：核心库那一问（`Sublibrary::adapter`）只说事实——是哪个格式、眼下带的是哪几个；
    // 换一个的命令由命令行补。来路只剩一种：旧库里存着这一版没带的格式（`set --format` 当场拦）。
    let (_library, workspace, _target) = 现场();
    {
        let mut catalog =
            romcat_core::catalog::Catalog::open(&romcat_core::workspace::catalog_path(
                workspace.path(),
                romcat_core::workspace::Slug::Named("测试库"),
            ))
            .expect("开得出中立库");
        let mut 旧库里存着的 = catalog.sublibrary("掌机").expect("读得动").expect("在");
        旧库里存着的.format = "这一版没带的格式".to_string();
        catalog.put_sublibrary(&旧库里存着的).expect("写得进");
    }
    let out = 子库(workspace.path(), &["plan", "掌机"]);
    let text = 出来的话(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("这一版没带「这一版没带的格式」"), "{text}");
    assert!(
        text.contains("romcat sublibrary set 掌机 --format"),
        "命令行该补上换一个格式的命令：{text}"
    );
}

#[test]
fn 差量预览底下那几件怪事_核心那句只说事实_命令行照种类补上那条命令() {
    // 挂单 `Q622` 那一族（`Prepared::concerns` 整份交成 `sync::Concern`，挂单 `Q1348`）：读不懂的规则、
    // 媒体池里找不到的媒体、超过半年没核实的档案声明——核心那几句不带命令，命令行照种类各补一条。
    let (_library, workspace, _target) = 现场();
    let ws = workspace.path();
    // 一份陈旧的名册：内置那份每一条的核实日期都拨回 2020 年。
    let 底稿 = ws.join("底稿.toml");
    let out = romcat(
        ws,
        &["capability", "--dump-builtin", &底稿.display().to_string()],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let 陈旧的: String = fs::read_to_string(&底稿)
        .expect("读得出")
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("\"核实日期\"") {
                "\"核实日期\" = \"2020-01-01\"".to_string()
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(ws.join("capability.toml"), 陈旧的).expect("写得进");
    let out = 子库(ws, &["set", "掌机", "--capability", "retroarch-exfat"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    {
        let mut catalog =
            romcat_core::catalog::Catalog::open(&romcat_core::workspace::catalog_path(
                ws,
                romcat_core::workspace::Slug::Named("测试库"),
            ))
            .expect("开得出中立库");
        // 一条读不懂的规则（换一版程序、或者人手改过这个 SQLite 文件）。
        catalog
            .add_rule(
                "掌机",
                &romcat_core::sublibrary::Rule {
                    text: "这不是一条规则".to_string(),
                    root: romcat_core::sublibrary::Group::new(
                        romcat_core::sublibrary::Join::All,
                        Vec::new(),
                    ),
                },
                None,
            )
            .expect("写得进");
        // 库里记着一张封面，媒体池里却没有那个文件。
        catalog
            .put_media(
                &"ab".repeat(32),
                "png",
                16,
                romcat_core::scrape::measure::Measured::default(),
            )
            .expect("记得下");
        catalog
            .put_scraped(&[romcat_core::catalog::scrape::Harvested {
                anchor: romcat_core::scrape::AnchorKind::Variant.label().to_string(),
                subject: "库/FC/魂斗罗.zip".to_string(),
                source: "本地媒体".to_string(),
                input: "库/FC/魂斗罗.zip/封面".to_string(),
                values: Vec::new(),
                media: vec![romcat_core::catalog::scrape::HarvestedMedia {
                    kind: romcat_core::scrape::MediaKind::Cover.label().to_string(),
                    hash: "ab".repeat(32),
                    evidence: "测试".to_string(),
                }],
            }])
            .expect("引用写得进");
    }

    let out = 子库(ws, &["plan", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    for (该有, 怎么回事) in [
        ("条规则读不懂", "读不懂的规则那一句"),
        (
            "romcat sublibrary show 掌机",
            "读不懂的规则：看是哪几条的命令",
        ),
        ("在媒体池里找不到那个文件", "媒体池里找不到那一句"),
        ("romcat scrape", "媒体池里找不到：重新刮削的命令"),
        ("超过半年没核实", "陈旧声明那一句"),
        (
            "romcat capability retroarch-exfat",
            "陈旧声明：看是哪几条的命令，点得出是哪一份档案",
        ),
    ] {
        assert!(text.contains(该有), "{怎么回事}没印出来：{text}");
    }
}

#[test]
fn 主文件放不进目标的变体_差量预览底下说一句没上卡前端里也不列_命令行补上是哪几个与排除的命令() {
    // 票 `verdict-store-and-sync/21`（挂单 `Q1847`）：主文件放不进目标的变体卡上的前端元数据不列、媒体也不铺。核心那句
    // 只说几个、各是哪一类（`sync::Concern::LeftOffCard`），是哪几个变体与排除它们的命令由命令行补。
    let (_library, workspace, _target) = 现场();
    let ws = workspace.path();
    // 一张「小卡」：内置名册，只把 FAT32 的单文件上限改成 3000 字节——超级玛丽（4 KiB）放不进，魂斗罗（2 KiB）放得下。
    let 底稿 = ws.join("底稿.toml");
    let out = romcat(
        ws,
        &["capability", "--dump-builtin", &底稿.display().to_string()],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let 原来 = "\"单文件上限\" = 4294967295";
    let 名册 = fs::read_to_string(&底稿).expect("读得出");
    assert!(
        名册.contains(原来),
        "内置名册里 FAT32 那一行变了，这份夹具得跟着改"
    );
    fs::write(
        ws.join("capability.toml"),
        名册.replace(原来, "\"单文件上限\" = 3000"),
    )
    .expect("写得进");
    let out = 子库(ws, &["set", "掌机", "--capability", "retroarch-fat32"]);
    assert!(out.status.success(), "{}", 出来的话(&out));

    let out = 子库(ws, &["plan", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    for (该有, 怎么回事) in [
        (
            "有 1 个变体放不进目标、没上卡，前端里也不列：超过单文件上限 1 个。",
            "没上卡那一句",
        ),
        ("库/FC/超级玛丽.zip（超过单文件上限）", "是哪一个、哪一类"),
        (
            "romcat sublibrary except 掌机 --exclude <变体的键>",
            "排除它们的命令",
        ),
    ] {
        assert!(text.contains(该有), "{怎么回事}没印出来：{text}");
    }
    assert!(
        !text.contains("库/FC/魂斗罗.zip（"),
        "放得下的不该列进没上卡那几个：{text}"
    );
}

#[test]
fn 子库不在时说得清怎么建() {
    let (_library, workspace, _target) = 现场();
    let out = 子库(workspace.path(), &["plan", "备用卡"]);
    let text = 出来的话(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("没有叫「备用卡」的子库"), "{text}");
    // 核心那句只说事实，建一个的命令由命令行补（挂单 `Q622` 那一族）。
    assert!(
        text.contains("romcat sublibrary set 备用卡 --target"),
        "命令行该补上建一个的命令：{text}"
    );
}

#[test]
fn 撞车明细在json里_撞在一起的几份归成一处() {
    // 票 `verdict-store-and-sync/13`（挂单 `Q1029`）：「撞的是哪几份」由核心一处归堆
    // （`Collision`），可 `--json` 里从前只有一条一条平铺的 `rejected`——读它的工具得自己按
    // 落点再归一次堆，那正是 ADR-0024 要挡的第二处判据。
    //
    // 第二块盘上同一条相对路径：子库里的落点剥掉了根名，两份都要落在卡上 `FC/魂斗罗.zip`。
    let (_library, workspace, _target) = 现场();
    let 另一块盘 = temp_dir("sync-cli-lib2");
    写(&另一块盘.path().join("FC/魂斗罗.zip"), &zip(1024));
    let out = romcat(
        workspace.path(),
        &[
            "scan",
            "--root-name",
            "另一块盘",
            &另一块盘.path().display().to_string(),
            "--library",
            "测试库",
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));

    let json = workspace.path().join("计划.json");
    let out = 子库(
        workspace.path(),
        &["plan", "掌机", "--json", &json.display().to_string()],
    );
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    let plan: serde_json::Value =
        serde_json::from_slice(&fs::read(&json).expect("读得出")).expect("是 JSON");

    let 撞车 = plan["collisions"]
        .as_array()
        .unwrap_or_else(|| panic!("--json 里没有撞车明细：{plan}"));
    assert_eq!(撞车.len(), 1, "撞的是一处：{plan}");
    assert_eq!(撞车[0]["path"], "FC/魂斗罗.zip");
    assert_eq!(撞车[0]["only_folded"], false);
    // **一处里是哪几份**：主库侧那条完整的键（带根名），按键排——与命令行报告、界面同一批。
    let 来自: Vec<&str> = 撞车[0]["files"]
        .as_array()
        .expect("有撞上的那几份")
        .iter()
        .map(|one| one["source"].as_str().expect("有主库侧的键"))
        .collect();
    assert_eq!(
        来自,
        ["另一块盘/FC/魂斗罗.zip", "库/FC/魂斗罗.zip"],
        "{plan}"
    );
    // 平铺的那一份照旧在：明细是**多出来**的字段，不是换掉了 `rejected`。
    assert_eq!(plan["rejected"].as_array().map(Vec::len), Some(2), "{plan}");

    // 印出来的那一份：两份，容量这一处只算一次（取大的那份 2048），并写明口径——与界面同一句。
    assert!(
        text.contains(&format!(
            "2 个、{}。",
            romcat_core::report::human_bytes(2048)
        )),
        "{text}"
    );
    assert!(
        text.contains(romcat_core::sync::REJECTED_BYTES_BASIS),
        "{text}"
    );
}

// ───────────────────────── `romcat sublibrary sync`：真的往目标上写

/// 卡上文件的 `(相对路径, 字节数)`。
fn 卡上有什么(dir: &Path) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            out.push((
                path.strip_prefix(dir)
                    .expect("在树里")
                    .display()
                    .to_string(),
                entry.metadata().expect("读得到").len(),
            ));
        }
    }
    out.sort();
    out
}

#[test]
fn 主库的根未连接时同步停住_命令行补上换位置的旗标() {
    // 挂单 `Q622` 那一族：「这几个根未连接」那句核心库只说事实（界面也印它），`--library-root` 是命令行
    // 自己的旗标，由命令行补。
    let (library, workspace, target) = 现场();
    drop(library);
    let out = 子库(workspace.path(), &["sync", "掌机", "--yes"]);
    let text = 出来的话(&out);
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("这几个根未连接：库"), "{text}");
    assert!(
        text.contains("--library-root 根名=路径"),
        "命令行该补上换位置的旗标：{text}"
    );
    assert_eq!(卡上有什么(target.path()).len(), 0, "停住了就一个字节都没写");
}

#[test]
fn 同步之前一定先印一遍差量预览() {
    // ADR-0016：「永远不能点了同步就开始传」。预览与计划是同一个值，于是这一句
    // 印的就是等下真要做的事。
    let (_library, workspace, target) = 现场();
    let out = 子库(workspace.path(), &["sync", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("差量预览"), "{text}");
    assert!(text.contains("同步结果"), "{text}");
    // 预览排在结果前面——反过来就不叫「先呈现」了。
    assert!(
        text.find("差量预览") < text.find("同步结果"),
        "预览必须印在动手之前：{text}"
    );
    assert_eq!(卡上有什么(target.path()).len(), 3, "两个 ROM 加一份元数据");
}

#[test]
fn 干跑一个字节都不写() {
    let (_library, workspace, target) = 现场();
    let out = 子库(workspace.path(), &["sync", "掌机", "--dry-run"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("差量预览"), "{text}");
    assert!(text.contains("一个字节都没写"), "{text}");
    assert_eq!(文件数(target.path()), 0, "**干跑不搬任何文件**");
}

#[test]
fn 有删除时不点头就一个字节都不动() {
    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    let 放好了 = 卡上有什么(target.path());
    assert!(!放好了.is_empty());

    // 规则改成只要 GB：FC 那两个加那份元数据都该被删——但**删之前要点头**。
    assert!(
        子库(workspace.path(), &["rule", "掌机", "--remove", "1"])
            .status
            .success()
    );
    assert!(
        子库(workspace.path(), &["rule", "掌机", "--add", "平台=GB"])
            .status
            .success()
    );
    let out = 子库(workspace.path(), &["sync", "掌机"]);
    let text = 出来的话(&out);
    assert!(!out.status.success(), "没点头不该算成功：{text}");
    assert!(text.contains("加 `--yes` 再跑一次"), "{text}");
    assert_eq!(卡上有什么(target.path()), 放好了, "卡上一个字节都没变");

    // 点头之后才真的删。
    let out = 子库(workspace.path(), &["sync", "掌机", "--yes"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    let 现在 = 卡上有什么(target.path());
    assert!(
        现在.iter().all(|(path, _)| !path.starts_with("FC")),
        "{现在:?}"
    );
    assert!(
        现在.iter().any(|(path, _)| path.starts_with("GB")),
        "{现在:?}"
    );
}

#[test]
fn 同步完清单记的是目标的真实状态_再跑一趟什么都不用动() {
    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    let out = 子库(workspace.path(), &["sync", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("一个文件都不用动"), "{text}");
    assert!(text.contains("一个字节都没写"), "{text}");
    assert_eq!(卡上有什么(target.path()).len(), 3);
}

#[test]
fn 在掌机上删掉的东西不会自己长回来() {
    // 用户故事 65。清单更新成目标的真实状态之后，那一格记着「你删过、我不补」。
    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    fs::remove_file(target.path().join("FC/魂斗罗.zip")).expect("删得掉");

    let text = 出来的话(&子库(workspace.path(), &["sync", "掌机"]));
    assert!(text.contains("没了"), "第一趟要如实报一次：{text}");
    assert!(!target.path().join("FC/魂斗罗.zip").exists(), "不静默补回");

    let text = 出来的话(&子库(workspace.path(), &["sync", "掌机"]));
    assert!(text.contains("你删过"), "{text}");
    assert!(!target.path().join("FC/魂斗罗.zip").exists(), "还是不补");

    // 明说要补才补——**明知故犯不是静默**。
    let text = 出来的话(&子库(workspace.path(), &["sync", "掌机", "--restore"]));
    assert!(text.contains("补回"), "{text}");
    assert!(target.path().join("FC/魂斗罗.zip").exists(), "这次补回来了");
}

#[test]
fn 差量预览里设备上缺失那一类的说明句与界面同一句_不带写给开发者的出处() {
    // 票 `gui-draws-the-rest-of-the-design/15`（差距 D-14、`F-5` A）：异常各栏那句「工具不会做什么」照稿改短、
    // 去掉 ADR 编号；命令行印的是核心同一处那一句（`SurpriseKind::refusal`），跟着变短。
    use romcat_core::sync::SurpriseKind;

    let (_library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    fs::remove_file(target.path().join("FC/魂斗罗.zip")).expect("删得掉");

    let out = 子库(workspace.path(), &["plan", "掌机"]);
    let text = 出来的话(&out);
    assert!(out.status.success(), "{text}");
    let 那一句 = SurpriseKind::Gone.refusal();
    assert_eq!(
        那一句,
        "清单里有、设备上找不到的文件，可能被手动删除了。默认不补回。"
    );
    let 印的那一行 = text
        .lines()
        .find(|line| line.trim() == 那一句)
        .unwrap_or_else(|| panic!("命令行没印界面那同一句：\n{text}"));
    assert!(!印的那一行.contains("ADR-"), "{印的那一行}");
}

#[test]
fn 手动拷进目标的东西同步之后一个字节都没变() {
    let (_library, workspace, target) = 现场();
    写(&target.path().join("saves/魂斗罗.sav"), &[9u8; 512]);
    写(&target.path().join("cheats/金手指.txt"), b"unlimited lives");
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    assert_eq!(
        fs::read(target.path().join("saves/魂斗罗.sav")).expect("还在"),
        vec![9u8; 512],
    );
    assert_eq!(
        fs::read(target.path().join("cheats/金手指.txt")).expect("还在"),
        b"unlimited lives",
    );
}

#[test]
fn 子库里的元数据路径全部相对子库根() {
    let (library, workspace, target) = 现场();
    assert!(子库(workspace.path(), &["sync", "掌机"]).status.success());
    let text = fs::read_to_string(target.path().join("FC.metadata.pegasus.txt")).expect("落到位了");
    assert!(
        !text.contains(&library.path().display().to_string()),
        "绝不写主库的绝对路径：\n{text}"
    );
    assert!(text.contains("files: FC/魂斗罗.zip"), "{text}");
}
