//! `romcat-gui --font-check`：**拿真库的键去核对字体覆盖面**，而不是等豆腐块出现在屏幕上。
//!
//! 子集只覆盖 GBK + Big5 + 假名 + U+2100–27BF；之外的字（CJK 扩展 B 的生僻字、谚文……）
//! 照样画不出来。这不是失手，是取舍——完整字体 16.9 MB。所以要有一条能问出
//! 「**我的库**撞不撞得上」的路，`docs/research/egui-viability.md` 的第 2 步说的就是它。

use std::process::Command;

use romcat_core::catalog::Catalog;
use romcat_core::platform::Manifest;
use romcat_core::shape::{SINGLE_FILE_RULE, Variant};
use romcat_core::testing::{TempDir, temp_dir};
use romcat_core::workspace::{Slug, catalog_path};

/// 建一份带这些键的**中立库**文件。
fn 建库(tag: &str, keys: &[&str]) -> (TempDir, std::path::PathBuf) {
    let dir = temp_dir(tag);
    let path = catalog_path(dir.path(), Slug::Named("小库"));
    let mut catalog = Catalog::open(&path).expect("开得出中立库");
    let variants: Vec<Variant> = keys
        .iter()
        .map(|key| Variant {
            key: (*key).to_string(),
            platform: Some("SFC".into()),
            rule: SINGLE_FILE_RULE.into(),
            main_key: (*key).to_string(),
            manual: false,
            files: 1,
            bytes: 1024,
            unreadable_files: 0,
            members: Vec::new(),
        })
        .collect();
    catalog
        .replace_variants(&variants, 1, &Manifest::default())
        .expect("写得进去");
    drop(catalog);
    (dir, path)
}

fn 跑(path: &std::path::Path) -> (bool, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_romcat-gui"))
        .args(["--font-check", "--catalog"])
        .arg(path)
        .output()
        .expect("跑得起来");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn 库里的键都画得出来时通过() {
    let (_dir, path) = 建库(
        "font-check-ok",
        &[
            "SFC/幻想传说（汉化版）.zip",
            "PS1/潛龍諜影 Ⅲ ★特别版★.chd",
            "NDS/ゼルダの伝説 ～時のオカリナ～.nds",
            "PSP/Pokémon Ōkami ①②③.iso",
        ],
    );
    let (ok, 输出) = 跑(&path);
    assert!(ok, "库里全是子集覆盖得到的字，却报了豆腐块：{输出}");
    assert!(输出.contains("过了中立库里 4 个变体的键"), "{输出}");
    assert!(输出.contains("没有豆腐块"), "{输出}");
}

#[test]
fn 库里有子集之外的字时报出来是哪个() {
    // `𪚥`（U+2A6A5）在 CJK 扩展 B，`한글` 是谚文——两者都在 GBK + Big5 之外。
    let (_dir, path) = 建库("font-check-miss", &["SFC/𪚥 한글.sfc"]);
    let (ok, 输出) = 跑(&path);
    assert!(!ok, "库里有画不出来的字，却说没问题：{输出}");
    assert!(输出.contains('𪚥'), "没报出那个扩展 B 的字：{输出}");
    assert!(输出.contains('한'), "没报出那个谚文字：{输出}");
}
