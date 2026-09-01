//! 端到端验收 DAT 仓库的三个子命令。
//!
//! **一律不联网**：这里只验「不同步的时候说得对不对」。真同步那条链路由
//! `romcat-core` 的 `tests/dat_sync.rs` 用备好的响应完整走一遍。

use std::path::Path;
use std::process::Command;

use romcat_core::testing::temp_dir;

fn 跑(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .arg("dat")
        .args(args)
        .args(["--workspace", &workspace.to_string_lossy()])
        .output()
        .expect("跑得起来")
}

#[test]
fn 还没同步过时报告说得清下一步() {
    let temp = temp_dir("dat-cli-空");
    let out = 跑(temp.path(), &["report"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("romcat dat sync"), "{text}");
}

#[test]
fn 数据源清单看得见也导得出() {
    let temp = temp_dir("dat-cli-源");
    let out = 跑(temp.path(), &["sources"]);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    for name in ["No-Intro", "Redump", "TOSEC", "MAME", "GoodNES"] {
        assert!(text.contains(name), "{text}");
    }
    // 两条取数纪律要在用户看得见的地方说出来，不能只写在代码注释里。
    assert!(text.contains("datomatic"), "{text}");
    assert!(text.contains("redump.info"), "{text}");

    let draft = temp.path().join("我的源.toml");
    let out = 跑(
        temp.path(),
        &["sources", "--dump-builtin", &draft.to_string_lossy()],
    );
    assert!(out.status.success());
    let toml = std::fs::read_to_string(&draft).expect("导得出底稿");
    assert!(toml.contains("\"版本\" = 1"), "{toml}");

    // 导出来的底稿要能原样读回去——不然「照着它改」这句话是空的。
    let out = 跑(
        temp.path(),
        &["sources", "--sources", &draft.to_string_lossy()],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn 导出的底稿里没有任何禁区地址() {
    // 用户会照着这份底稿改。它要是把 datomatic 或冻结的 redump.org 写在里面，
    // 早晚有人把注释删掉、把地址留下。
    let temp = temp_dir("dat-cli-禁区");
    let draft = temp.path().join("底稿.toml");
    跑(
        temp.path(),
        &["sources", "--dump-builtin", &draft.to_string_lossy()],
    );
    let toml = std::fs::read_to_string(&draft).expect("导得出底稿");
    for line in toml.lines() {
        let 是地址 = line.contains("https://") || line.contains("http://");
        assert!(
            !(是地址 && (line.contains("datomatic") || line.contains("redump.org"))),
            "底稿里出现了指向禁区的地址：{line}"
        );
    }
    // 唯一该出现的 Redump 地址是现行域名。
    assert!(toml.contains("https://redump.info"), "{toml}");
}

#[test]
fn 点名一个不存在的源要当场说() {
    let temp = temp_dir("dat-cli-错源");
    let out = 跑(temp.path(), &["sync", "--source", "Datomatic", "--dry-run"]);
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("没有 Datomatic 这个源"), "{text}");
}

#[test]
fn 数据源清单里的平台名对不上平台清单时开工前就拦住() {
    // 打错一个字，那份 DAT 会归到一个谁也查不到的平台上，报告里多出一行没人认得的名字。
    let temp = temp_dir("dat-cli-错平台");
    let sources = temp.path().join("坏的.toml");
    std::fs::write(
        &sources,
        "\"版本\" = 1\n\
[[\"数据源\"]]\n\"名\" = \"Redump\"\n\"取法\" = \"Redump 站点\"\n\
\"站点\" = \"https://redump.info\"\n\"格式\" = \"Logiqx\"\n\"口径\" = \"含头\"\n\
[[\"映射\"]]\n\"源\" = \"Redump\"\n\"名\" = \"PSX\"\n\"平台\" = \"PlayStation1\"\n",
    )
    .expect("写得出");
    let out = 跑(
        temp.path(),
        &["sync", "--sources", &sources.to_string_lossy(), "--dry-run"],
    );
    assert!(!out.status.success());
    let text = String::from_utf8_lossy(&out.stderr);
    assert!(text.contains("PlayStation1"), "{text}");
}
