//! **目录选择器交回一个路径之后**：开场换工作目录、添加主库向导选第一个根（票 `gui-answers-all-six/01`）。
//!
//! 弹对话框那一层（`romcat_gui::pick`）**不在这儿测**，为什么写在它自己的模块文档里。这儿测的是
//! 它交回来之后的那一半：选中了一个目录，走的是贴路径那条现成的路；取消了，什么都不动。
//!
//! 贴路径那条路本身的测试在 `tests/program.rs`，一条不改。跑帧、点按钮、打字用的是几份界面测试
//! 共用的那几样（`shared`），不另搭一份。

use std::path::Path;

use romcat_core::catalog::Catalog;
use romcat_core::testing::temp_dir;
use romcat_core::workspace::{self, Slug};
use romcat_gui::claim::{Outcome, Wizard};
use romcat_gui::headless;
use romcat_gui::opening::Screen;

mod shared;

/// 在这个工作目录里建一份叫这个名字的中立库。开场那一行上写着的就是这个名字。
fn 建一份库(工作目录: &Path, 名字: &str) {
    let slug = Slug::Named(名字);
    let 库文件 = workspace::catalog_path(工作目录, slug);
    drop(Catalog::create(&库文件, &slug.display_name()).expect("建得出中立库"));
}

// ——— 开场：换工作目录 ———

#[test]
fn 开场上选中一个目录立刻列出那个目录里的库() {
    let 甲 = temp_dir("gui-pick-开场-甲");
    let 乙 = temp_dir("gui-pick-开场-乙");
    建一份库(甲.path(), "甲那边的库");
    建一份库(乙.path(), "乙那边的库");
    let ctx = headless::context();
    let mut 开场 = Screen::new(甲.path().to_path_buf());

    let 屏上 = shared::跑一帧(&ctx, |ui| drop(开场.ui(ui)));
    assert!(屏上.contains("甲那边的库"), "一开始该看着甲：\n{屏上}");

    // 框里先打了半截字，然后改主意去点选择器：选中之后那半截字跟着贴路径那条路一起清掉。
    shared::打字(
        &ctx,
        "换一个工作目录：把路径贴在这儿",
        "/半截没打完",
        |ui| {
            drop(开场.ui(ui));
        },
    );
    开场.picked(Some(乙.path().to_path_buf()));
    let 屏上 = shared::跑一帧(&ctx, |ui| drop(开场.ui(ui)));

    assert!(屏上.contains("乙那边的库"), "选中了乙却没列出来：\n{屏上}");
    assert!(
        !屏上.contains("甲那边的库"),
        "选中了乙，甲那边的库还挂在屏上：\n{屏上}",
    );
    assert!(
        屏上.contains(&romcat_core::path::display(乙.path())),
        "抬头那一行写的还不是乙：\n{屏上}",
    );
    assert!(
        !屏上.contains("/半截没打完"),
        "换过去之后框里还留着那半截字：\n{屏上}",
    );
}

#[test]
fn 开场上取消选择器什么都不变() {
    let 甲 = temp_dir("gui-pick-开场取消-甲");
    建一份库(甲.path(), "甲那边的库");
    let ctx = headless::context();
    let mut 开场 = Screen::new(甲.path().to_path_buf());

    shared::打字(
        &ctx,
        "换一个工作目录：把路径贴在这儿",
        "/半截没打完",
        |ui| {
            drop(开场.ui(ui));
        },
    );
    let 取消之前 = shared::跑一帧(&ctx, |ui| drop(开场.ui(ui)));
    开场.picked(None);
    let 取消之后 = shared::跑一帧(&ctx, |ui| drop(开场.ui(ui)));

    assert!(
        取消之后.contains("/半截没打完"),
        "取消了选择器，框里打着的字没了：\n{取消之后}",
    );
    assert!(
        取消之后.contains("甲那边的库")
            && 取消之后.contains(&romcat_core::path::display(甲.path())),
        "取消了选择器，看的却不是甲了：\n{取消之后}",
    );
    assert_eq!(取消之前, 取消之后, "取消了选择器，屏上变了样");
}

// ——— 添加主库向导：选第一个根 ———

#[test]
fn 向导里选中一个目录填进选根那一步开始扫描照旧往下走() {
    let 工作目录 = temp_dir("gui-pick-向导-工作目录");
    let 盘 = temp_dir("gui-pick-向导-盘");
    let ctx = headless::context();
    // 弹层标题用的是观感基线里的 `title` 字号（票 `gui-looks-like-the-design/04`），不装就当场 panic。
    romcat_gui::look::install(&ctx);
    let mut 向导 = Wizard::new(工作目录.path().to_path_buf());
    // 头一帧 egui 在量弹层多大（不画、也不接），第二帧才摆稳——先空跑两帧再去点、去打字。
    shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));
    shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));

    shared::打字(&ctx, "主库原名，例如", "我的主库", |ui| {
        drop(向导.show(ui.ctx()))
    });
    shared::点一下(&ctx, "下一步", |ui| drop(向导.show(ui.ctx())));
    向导.picked_root(Some(盘.path().to_path_buf()));
    let 屏上 = shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));
    assert!(
        屏上.contains(&romcat_core::path::display(盘.path())),
        "选中的目录没落进选根那个框：\n{屏上}",
    );

    let mut 走完 = None;
    let 屏上 = shared::点一下(&ctx, "开始扫描", |ui| {
        if let Outcome::Done(交出来的) = 向导.show(ui.ctx()) {
            走完 = Some(交出来的);
        }
    });
    let Some(交出来的) = 走完 else {
        panic!("按下开始扫描却没走完：\n{屏上}");
    };
    assert_eq!(
        Path::new(&交出来的.root.path),
        盘.path(),
        "向导交出去的第一个根不是选中的那个目录",
    );
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    assert!(
        库文件.is_file(),
        "走完了却没建出中立库：{}",
        库文件.display()
    );
}

#[test]
fn 向导里取消选择器不进下一步也不动已经打的字() {
    let 工作目录 = temp_dir("gui-pick-向导取消-工作目录");
    let ctx = headless::context();
    // 弹层标题用的是观感基线里的 `title` 字号（票 `gui-looks-like-the-design/04`），不装就当场 panic。
    romcat_gui::look::install(&ctx);
    let mut 向导 = Wizard::new(工作目录.path().to_path_buf());
    // 头一帧 egui 在量弹层多大（不画、也不接），第二帧才摆稳——先空跑两帧再去点、去打字。
    shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));
    shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));

    shared::打字(&ctx, "主库原名，例如", "我的主库", |ui| {
        drop(向导.show(ui.ctx()))
    });
    shared::点一下(&ctx, "下一步", |ui| drop(向导.show(ui.ctx())));
    shared::打字(
        &ctx,
        "那块盘上的目录",
        "/半截没打完的盘",
        |ui| {
            drop(向导.show(ui.ctx()));
        },
    );
    shared::打字(&ctx, "根名（不填", "乙盘", |ui| {
        drop(向导.show(ui.ctx()))
    });
    let 取消之前 = shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));
    向导.picked_root(None);
    let 取消之后 = shared::跑一帧(&ctx, |ui| drop(向导.show(ui.ctx())));

    assert!(
        取消之后.contains("第二步") && 取消之后.contains("开始扫描"),
        "取消了选择器，向导却不在选根那一步了：\n{取消之后}",
    );
    assert!(
        取消之后.contains("/半截没打完的盘") && 取消之后.contains("乙盘"),
        "取消了选择器，框里打着的字没了：\n{取消之后}",
    );
    assert_eq!(取消之前, 取消之后, "取消了选择器，屏上变了样");
    let 库文件 = workspace::catalog_path(工作目录.path(), Slug::Named("我的主库"));
    assert!(
        !库文件.exists(),
        "取消了选择器，中立库却建出来了：{}",
        库文件.display(),
    );
}
