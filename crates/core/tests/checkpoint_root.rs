//! **换挂载点之后带续跑**：断点记的扫描根对不上，从头扫一遍，不硬报错。
//!
//! ADR-0018 定的工作方式是一块盘在两台机器之间来回接，ADR-0020 又写明 macOS 重挂
//! 一次就可能从 `/Volumes/甲` 变成 `/Volumes/甲 1`——所以「换挂载点」是主路径而不是
//! 边缘情况。断点里的 `pending` 全是旧挂载点下的**绝对路径**，换了根本来就续不下去，
//! **从头扫才是正确结果**；硬报错只会把主路径堵死，而且报错之前库里根的位置已经改成
//! 了新挂载点，用户手上剩一个「改了一半」的库。
//!
//! 这一条同时把**界面**那一半也钉住：界面发起的扫描总是 `resume: true`
//! （`gui::roots`），自己没有「去掉 `--resume`」这条出路，所以核心一硬报错，界面上
//! 换挂载点之后就再也扫不动了。

use std::path::Path;
use std::time::Duration;

use romcat_core::catalog::Catalog;
use romcat_core::fs::MemFs;
use romcat_core::scan::{self, CheckpointOptions, Jobs, ScanOptions};
use romcat_core::task::Handle;
use romcat_core::testing::temp_dir;

const 根名: &str = "甲盘";

/// 一块盘：两个平台目录，够 `guard_same_root` 认出「还是同一块盘」。
fn 建盘(at: &str) -> MemFs {
    let mut library = MemFs::new();
    library.file(format!("{at}/FC/魂斗罗.zip"), b"pretend".to_vec());
    library.file(format!("{at}/GBA/黄金太阳.gba"), b"pretend too".to_vec());
    library.dir(at);
    library
}

fn 扫描选项(at: &str, 断点: &Path, resume: bool) -> ScanOptions {
    let mut options = ScanOptions::named(at, 根名);
    options.jobs = Jobs::Fixed(1);
    options.checkpoint = Some(CheckpointOptions {
        path: 断点.to_path_buf(),
        interval: Duration::ZERO,
        resume,
    });
    options
}

#[test]
fn 换挂载点之后带续跑折成从头扫而不是硬报错() {
    // ⭐ **断点对不上一律从头扫**（`scan::load_start_state` 的模块话）。这条此前只对
    // 「遍历身份」那一项做到了，扫描根那一项仍是一句硬错误。
    let workspace = temp_dir("checkpoint-root");
    let 断点 = workspace.path().join("甲盘.checkpoint.json");
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");

    // 甲盘挂在 /Volumes/甲：先完整扫一遍，库里这个根底下才有顶层条目可比。
    let 完整 = 扫描选项("/Volumes/甲", &断点, false);
    let 首扫 =
        scan::scan(&建盘("/Volumes/甲"), &mut catalog, &完整, &Handle::new()).expect("扫得动");
    assert!(!首扫.interrupted, "前提：第一趟完整跑完");

    // 再扫一趟，这一趟一开工就被叫停，断点落在盘上，里头记着 /Volumes/甲。
    let 带断点 = 扫描选项("/Volumes/甲", &断点, true);
    let 停手 = Handle::new();
    停手.cancel().cancel();
    let 中断 =
        scan::scan(&建盘("/Volumes/甲"), &mut catalog, &带断点, &停手).expect("中断也算正常返回");
    assert!(中断.interrupted, "前提：这一趟本该被中断");
    assert!(断点.exists(), "前提：断点落下来了");

    // 盘重挂成 /Volumes/甲 1，同一个根名再扫一趟，还带着续跑。
    let 新挂载 = 扫描选项("/Volumes/甲 1", &断点, true);
    let 这一趟 = scan::scan(
        &建盘("/Volumes/甲 1"),
        &mut catalog,
        &新挂载,
        &Handle::new(),
    )
    .expect("换了挂载点也该扫得动：断点对不上就从头扫，不是硬报错");

    assert!(
        !这一趟.report.resumed,
        "断点是旧挂载点下那一份，pending 全是旧绝对路径，续不了"
    );
    assert!(
        !这一趟.interrupted && 这一趟.shaped,
        "从头扫一趟走到底，收尾与成型都跟着跑"
    );
    let 说法 = 这一趟
        .resume_declined
        .as_ref()
        .expect("要说得出为什么没续上");
    assert_eq!(说法.recorded, "/Volumes/甲");
    assert_eq!(说法.current, "/Volumes/甲 1");

    assert_eq!(
        catalog.root(根名).expect("读得出").expect("有这个根").path,
        "/Volumes/甲 1",
        "根的位置就该是新挂载点——这一趟真的把它整个扫了一遍"
    );
    assert!(
        !断点.exists(),
        "完整扫完就把断点收掉，旧挂载点那一份不会一直躺在那儿"
    );
}
