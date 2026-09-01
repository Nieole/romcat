//! 整条同步链路的离线走查：列举 → 排计划 → 取 → 解析 → 入库 → 报告。
//!
//! **一个字节都不上网。** DAT 是几百 MB 的东西，每跑一次测试就去拉一遍既慢又不礼貌；
//! 而且真要拉，这些断言就成了「今天那几个站上有什么」的快照，明天自己会红。
//! 服务器会怎么答直接摆在这里，与 [`MemFs`](romcat_core::fs::MemFs) 是同一个用法。

use std::collections::BTreeMap;

use romcat_core::dat::registry::Registry;
use romcat_core::dat::repo::DatRepo;
use romcat_core::dat::report::DatReport;
use romcat_core::dat::sync::{self, Action, SyncOptions};
use romcat_core::dat::{CannedFetcher, Convention};
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::temp_dir;

const NOINTRO_RELEASE: &str =
    "https://api.github.com/repos/hugo19941994/auto-datfile-generator/releases/latest";
const NOINTRO_ZIP: &str = "https://github.com/hugo19941994/auto-datfile-generator/releases/download/Daily_Rebuild/no-intro.zip";
const REDUMP_DOWNLOADS: &str = "https://redump.info/downloads/";
const REDUMP_PSX: &str = "https://redump.info/datfile/PSX";
const TOSEC_TREE: &str =
    "https://api.github.com/repos/smesgr9000/TOSEC-DAT/git/trees/main?recursive=1";
const TOSEC_NGPC: &str = "https://raw.githubusercontent.com/smesgr9000/TOSEC-DAT/main/TOSEC/SNK%20Neo-Geo%20Pocket%20Color%20-%20Games.dat";
const MAME_TREE: &str =
    "https://api.github.com/repos/mamedev/mame/git/trees/master:hash?recursive=1";
const MAME_NES: &str = "https://raw.githubusercontent.com/mamedev/mame/master/hash/nes.xml";
const BIZHAWK_TREE: &str =
    "https://api.github.com/repos/TASEmulators/BizHawk/git/trees/master:Assets/gamedb?recursive=1";
const GOODNES: &str = "https://raw.githubusercontent.com/TASEmulators/BizHawk/master/Assets/gamedb/gamedb_goodnes.txt";

fn logiqx(name: &str, games: &[&str]) -> Vec<u8> {
    let mut xml = format!(
        "<?xml version=\"1.0\"?>\n<datafile><header><name>{name}</name><version>1</version></header>"
    );
    for (index, game) in games.iter().enumerate() {
        xml.push_str(&format!(
            "<game name=\"{game}\"><rom name=\"{game}.bin\" size=\"16\" crc=\"{:08x}\" \
             sha1=\"{:040x}\"/></game>",
            index + 1,
            index + 1
        ));
    }
    xml.push_str("</datafile>");
    xml.into_bytes()
}

fn nointro_bundle() -> Vec<u8> {
    zip_container(&[
        // 同一个平台上两份 DAT，哈希口径相反——这张票最要紧的那件事。
        ZipEntrySpec::stored(
            "Nintendo - Nintendo Entertainment System (Headerless) (20260101-000000).dat",
            logiqx(
                "Nintendo - Nintendo Entertainment System (Headerless)",
                &["Some Game (Japan)", "Other Game (USA) (Zh)"],
            ),
        ),
        ZipEntrySpec::stored(
            "Nintendo - Nintendo Entertainment System (Headered) (20260101-000000).dat",
            logiqx(
                "Nintendo - Nintendo Entertainment System (Headered)",
                &["Some Game (Japan)"],
            ),
        ),
        // 没有映射命中的那一份不该进库。
        ZipEntrySpec::stored(
            "Commodore - Amiga (20260101-000000).dat",
            logiqx("Commodore - Amiga", &["Not Ours (Europe)"]),
        ),
    ])
}

fn redump_psx_zip() -> Vec<u8> {
    zip_container(&[ZipEntrySpec::stored(
        "Sony - PlayStation - Datfile (2) (2026-08-31 07-59-02).dat",
        logiqx(
            "Sony - PlayStation",
            &[
                "'98 Koushien (Japan) (Demo)",
                "Another (Japan) (Ja,Zh-Hant)",
            ],
        ),
    )])
}

fn tree_json(entries: &[(&str, &str)], truncated: bool) -> Vec<u8> {
    let blobs: Vec<String> = entries
        .iter()
        .map(|(path, sha)| {
            format!("{{\"path\":\"{path}\",\"type\":\"blob\",\"sha\":\"{sha}\",\"size\":1}}")
        })
        .collect();
    format!(
        "{{\"sha\":\"root\",\"truncated\":{truncated},\"tree\":[{}]}}",
        blobs.join(",")
    )
    .into_bytes()
}

fn canned() -> CannedFetcher {
    CannedFetcher::new()
        .with(
            NOINTRO_RELEASE,
            format!(
                "{{\"tag_name\":\"Daily_Rebuild\",\"assets\":[\
                 {{\"name\":\"no-intro.zip\",\"size\":106776071,\
                 \"updated_at\":\"2026-07-08T13:31:47Z\",\
                 \"browser_download_url\":\"{NOINTRO_ZIP}\"}},\
                 {{\"name\":\"redump.zip\",\"size\":1,\"updated_at\":\"2026-08-31T18:25:48Z\",\
                 \"browser_download_url\":\"https://github.com/x/redump.zip\"}}]}}"
            )
            .into_bytes(),
        )
        .with(NOINTRO_ZIP, nointro_bundle())
        .with(
            REDUMP_DOWNLOADS,
            b"<a href=\"/datfile/PSX\">PS</a><a href=\"/datfile/PS4\">PS4</a>".to_vec(),
        )
        .with_headers(
            REDUMP_PSX,
            &[(
                "Content-Disposition",
                "attachment; filename=\"Sony - PlayStation - Datfile (2) (2026-08-31 07-59-02).zip\"",
            )],
            redump_psx_zip(),
        )
        .with(
            TOSEC_TREE,
            tree_json(
                &[
                    ("TOSEC/SNK Neo-Geo Pocket Color - Games.dat", "sha-ngpc"),
                    ("TOSEC-PIX/Nintendo Wii - Books.dat", "sha-pix"),
                    ("README.md", "sha-readme"),
                ],
                false,
            ),
        )
        .with(
            TOSEC_NGPC,
            logiqx(
                "SNK Neo-Geo Pocket Color - Games",
                &[
                    "Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]",
                    "Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh]",
                    "Metal Slug - 1st Mission (1999)(SNK)(en-ja)[tr zh][a]",
                    "Normal Game (1999)(SNK)",
                ],
            ),
        )
        .with(
            MAME_TREE,
            tree_json(&[("nes.xml", "sha-nes"), ("amiga.xml", "sha-amiga")], false),
        )
        .with(
            MAME_NES,
            br#"<softwarelist name="nes" description="NES cartridges">
                <software name="89denku"><description>'89 Dennou Kyuusei Uranai</description>
                <part name="cart" interface="nes_cart"><dataarea name="prg" size="262144">
                <rom name="ipc-j1-0 prg" size="262144" crc="ba58ed29"
                     sha1="56fe858d1035dce4b68520f457a0858bae7bb16d"/>
                </dataarea></part></software></softwarelist>"#
                .to_vec(),
        )
        .with(
            BIZHAWK_TREE,
            tree_json(
                &[
                    ("gamedb_goodnes.txt", "sha-good"),
                    ("gamedb_a2600.txt", "sha-2600"),
                ],
                false,
            ),
        )
        .with(
            GOODNES,
            "\
;GoodNES SHA-1 List\n\
859a8b0459496fb4e998ffcf04414a69155c1eff\tT\t1942 (JU) [T+Chi_MS emumax]\tNES\n\
6d419c58b56098b94d5f665139fd84a9e0f2f60d\tT\t4 Nin Uchi Mahjong (J) (PRG1) [T+Chi]\tNES\n\
2d5b194357b46ad993a6ac716949fb1688a46939\tU\t!Clik! (2008) (PD)\tNES\n"
                .as_bytes()
                .to_vec(),
        )
}

fn options(cache: &std::path::Path) -> SyncOptions {
    SyncOptions {
        only: Vec::new(),
        full: false,
        dry_run: false,
        cache: cache.to_path_buf(),
    }
}

#[test]
fn 五个源一趟同步下来每个平台数得出多少条() {
    let temp = temp_dir("dat-sync");
    let fetcher = canned();
    let registry = Registry::builtin();
    let mut repo = DatRepo::in_memory().expect("开得出来");

    let outcome = sync::run(&fetcher, &mut repo, &registry, &options(temp.path())).expect("跑得完");
    assert!(outcome.problems.is_empty(), "{:?}", outcome.problems);
    // No-Intro 一件（整包）、Redump 一件、TOSEC 一份、MAME 一份、GoodNES 一份。
    assert_eq!(outcome.fetched, 5);
    // 整包里映射命中两份，另外四件各一份。
    assert_eq!(outcome.dats, 6);
    // 整包里那份 Amiga 取回来了但没映射命中——**这个数在计划里看不出来**，
    // 因为整包只算一件「取」。不报它，报告会显得 No-Intro 一份都没落下。
    assert_eq!(outcome.unmapped_dats, 1);

    let report = DatReport::build(&repo).expect("折得出报告");
    assert_eq!(report.totals.dats, 6);

    // ── 每个平台有多少条，含中文 ──────────────────────────────
    let by_platform: BTreeMap<&str, &_> = report
        .platforms
        .iter()
        .map(|row| (row.name.as_str(), row))
        .collect();

    let fc = by_platform["FC"];
    // 两份 No-Intro（口径相反）+ MAME + GoodNES
    assert_eq!(fc.dats, 4);
    assert_eq!(fc.games, 2 + 1 + 1 + 3);
    assert_eq!(fc.fan, 2, "GoodNES 的两条 [T+Chi]");
    assert_eq!(fc.official, 1, "No-Intro 的一条 (Zh)");

    let ngpc = by_platform["NGPC"];
    assert_eq!(ngpc.games, 4);
    assert_eq!(ngpc.fan, 3, "TOSEC 的三条 [tr zh]");

    let ps1 = by_platform["PS1"];
    assert_eq!(ps1.games, 2);
    assert_eq!(ps1.official, 1, "(Ja,Zh-Hant) 是官中不是汉化");
    assert_eq!(ps1.fan, 0);

    // 没有映射命中的整份不入库：Amiga / TOSEC-PIX / a2600 / amiga.xml 一条都不该在。
    assert!(!by_platform.contains_key("MSX"));
    assert_eq!(
        report.totals.games,
        7 + 4 + 2,
        "FC 7 条、NGPC 4 条、PS1 2 条"
    );

    // ── 两套哈希口径必须分得开 ────────────────────────────────
    let 去头 = report
        .dats
        .iter()
        .find(|row| row.name.contains("(Headerless)"))
        .expect("有去头那份");
    let 含头 = report
        .dats
        .iter()
        .find(|row| row.name.contains("(Headered)"))
        .expect("有含头那份");
    assert_eq!(去头.convention, Convention::Headerless.label());
    assert_eq!(含头.convention, Convention::AsIs.label());
    // TOSEC 与 GoodNES 一律含头，MAME 是逐芯片。
    for row in &report.dats {
        let expected = match row.source.as_str() {
            "TOSEC" | "GoodNES" | "Redump" => Convention::AsIs.label(),
            "MAME" => Convention::PerChip.label(),
            _ => continue,
        };
        assert_eq!(row.convention, expected, "{}", row.name);
    }
}

#[test]
fn 第二趟指纹没变就一件都不取() {
    let temp = temp_dir("dat-增量");
    let registry = Registry::builtin();
    let mut repo = DatRepo::in_memory().expect("开得出来");

    let first = canned();
    sync::run(&first, &mut repo, &registry, &options(temp.path())).expect("第一趟");
    let before = DatReport::build(&repo).expect("报告").totals.games;

    let second = canned();
    let outcome = sync::run(&second, &mut repo, &registry, &options(temp.path())).expect("第二趟");
    assert_eq!(outcome.fetched, 0, "指纹没变就不该重下");
    assert_eq!(outcome.skipped, 5);

    // 问过的 URL 里只该有列举那几个，正文一个都没取。
    let asked = second.asked();
    assert!(!asked.iter().any(|url| url == NOINTRO_ZIP), "{asked:?}");
    assert!(!asked.iter().any(|url| url == TOSEC_NGPC), "{asked:?}");
    assert!(!asked.iter().any(|url| url == GOODNES), "{asked:?}");
    // Redump 那一档指纹在响应头里，HEAD 还是要发一次——但正文没读。
    assert!(asked.iter().any(|url| url == REDUMP_PSX));

    // 库里的东西一条没少也没多。
    assert_eq!(DatReport::build(&repo).expect("报告").totals.games, before);
}

#[test]
fn 全取一趟也不必把那个整包再下一遍() {
    // 「改一条映射重新入库不必重下 106 MB」这句话，只有缓存**读得回来**才成立。
    // 缓存只写不读的话，`--full` 与改映射的代价都是重下整个包。
    let temp = temp_dir("dat-缓存");
    let registry = Registry::builtin();
    let mut repo = DatRepo::in_memory().expect("开得出来");
    sync::run(&canned(), &mut repo, &registry, &options(temp.path())).expect("第一趟");

    let again = canned();
    let mut full = options(temp.path());
    full.full = true;
    let outcome = sync::run(&again, &mut repo, &registry, &full).expect("第二趟");
    assert_eq!(outcome.fetched, 5, "--full 一给，每一件都重新入库");
    assert_eq!(outcome.dats, 6);

    let asked = again.asked();
    // 整包在缓存里，指纹没变，一个字节都不必重下。
    assert!(!asked.iter().any(|url| url == NOINTRO_ZIP), "{asked:?}");
    // Redump 那一档同样命中缓存：**只**问了一次 HEAD（列举要靠它拿指纹），
    // 没有第二次把正文取回来。第一趟是 HEAD 加 GET 两次。
    assert_eq!(
        asked.iter().filter(|url| *url == REDUMP_PSX).count(),
        1,
        "{asked:?}"
    );
    // 而仓库里那几份是逐份取的，本来就没有缓存这一层，照常重取。
    assert!(asked.iter().any(|url| url == TOSEC_NGPC), "{asked:?}");
}

#[test]
fn 指纹变了就整件换掉不是叠加() {
    let temp = temp_dir("dat-换新");
    let registry = Registry::builtin();
    let mut repo = DatRepo::in_memory().expect("开得出来");
    sync::run(&canned(), &mut repo, &registry, &options(temp.path())).expect("第一趟");

    // TOSEC 那份 DAT 变了：blob sha 换一个，内容少一条。
    let changed = canned()
        .with(
            TOSEC_TREE,
            tree_json(
                &[
                    ("TOSEC/SNK Neo-Geo Pocket Color - Games.dat", "sha-新的"),
                    ("TOSEC-PIX/Nintendo Wii - Books.dat", "sha-pix"),
                ],
                false,
            ),
        )
        .with(
            TOSEC_NGPC,
            logiqx(
                "SNK Neo-Geo Pocket Color - Games",
                &["Dark Arms - Beast Buster 1999 (1999)(SNK)(en-ja)[tr zh]"],
            ),
        );
    let outcome = sync::run(&changed, &mut repo, &registry, &options(temp.path())).expect("第二趟");
    assert_eq!(outcome.fetched, 1);

    let report = DatReport::build(&repo).expect("报告");
    let ngpc = report
        .platforms
        .iter()
        .find(|row| row.name == "NGPC")
        .expect("有 NGPC");
    assert_eq!(ngpc.games, 1, "旧的四条要被整件换掉，不是变成五条");
    assert_eq!(ngpc.fan, 1);
}

#[test]
fn 文件清单被截断要当场停下而不是照单全收() {
    // 这正是 `/contents` 那个坑：它在 1000 条处截断且**不报错**，
    // 于是清单看起来是完整的，只是少了整个平台的 DAT。trees 会说 truncated，
    // 不看它就等于把同一个坑换个地方再踩一遍。
    let temp = temp_dir("dat-截断");
    let registry = Registry::builtin();
    let mut repo = DatRepo::in_memory().expect("开得出来");
    let fetcher = canned().with(
        TOSEC_TREE,
        tree_json(
            &[("TOSEC/SNK Neo-Geo Pocket Color - Games.dat", "sha")],
            true,
        ),
    );
    let mut options = options(temp.path());
    options.only = vec!["TOSEC".to_string()];
    let error = sync::run(&fetcher, &mut repo, &registry, &options).expect_err("该停下");
    let text = format!("{error}");
    assert!(text.contains("truncated"), "{text}");
    assert!(repo.is_empty().expect("读得出"), "停下就该什么都没写");
}

#[test]
fn 换掉数据源清单也绕不过取数纪律() {
    // 数据源清单是**数据**，用户可以整份换掉。闸门要在一个字节发出去之前就生效。
    let temp = temp_dir("dat-闸门");
    let text = "\
\"版本\" = 1

[[\"数据源\"]]
\"名\" = \"No-Intro\"
\"取法\" = \"GitHub 发行资产\"
\"仓库\" = \"hugo19941994/auto-datfile-generator\"
\"资产\" = \"no-intro.zip\"
\"格式\" = \"Logiqx\"
\"口径\" = \"含头\"

[[\"映射\"]]
\"源\" = \"No-Intro\"
\"名\" = \"*\"
\"平台\" = \"FC\"
";
    let registry = Registry::parse(text, "测试").expect("读得动");
    let mut repo = DatRepo::in_memory().expect("开得出来");
    // 镜像被人改成把资产指向了 datomatic。
    let fetcher = CannedFetcher::new().with(
        NOINTRO_RELEASE,
        b"{\"assets\":[{\"name\":\"no-intro.zip\",\"size\":1,\"updated_at\":\"x\",\
          \"browser_download_url\":\"https://datomatic.no-intro.org/index.php?page=download\"}]}"
            .to_vec(),
    );
    let outcome = sync::run(&fetcher, &mut repo, &registry, &options(temp.path())).expect("跑得完");
    assert_eq!(outcome.fetched, 0);
    assert!(matches!(
        outcome.plans[0].items[0].action,
        Action::Refused(_)
    ));
    // **一个字节都没发到那个域名上。**
    assert_eq!(fetcher.asked(), vec![NOINTRO_RELEASE.to_string()]);
}

#[test]
fn 只排计划时什么都不取也不写() {
    let temp = temp_dir("dat-空跑");
    let registry = Registry::builtin();
    let mut repo = DatRepo::in_memory().expect("开得出来");
    let fetcher = canned();
    let mut options = options(temp.path());
    options.dry_run = true;
    let outcome = sync::run(&fetcher, &mut repo, &registry, &options).expect("跑得完");
    assert_eq!(outcome.fetched, 0);
    assert!(repo.is_empty().expect("读得出"));
    // 计划照样说得出这一趟会取几件。
    let total: usize = outcome.plans.iter().map(sync::Plan::to_fetch).sum();
    assert_eq!(total, 5);
    assert!(!fetcher.asked().iter().any(|url| url == NOINTRO_ZIP));
}
