//! **两个壳读同一份平台清单与优先级表**（票 `core-answers-once/04`，收挂单 `Q1031` / `Q976`）。
//!
//! 两件事：
//!
//! - **同一份库，两个壳看见的平台表要一样**：工作目录里放了自己那份 `platforms.toml`，
//!   界面就走那一份，不是内置那份。
//! - **读不动要说出来**（ADR-0021 第三态）：工作目录里那份平台清单或优先级表读不动时，
//!   开窗那一屏上有一句话，而且说清眼下用的是哪一份——不许静默退回内置。
//!
//! 摆的都是临时工作目录，一份真库都不碰。

use std::fs;
use std::path::{Path, PathBuf};

use romcat_core::catalog::Catalog;
use romcat_core::site::Site;
use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, revoke_file, temp_dir};
use romcat_gui::app::App;
use romcat_gui::headless;

mod shared;
use shared::{等任务台空了, 跑一帧};

/// 一份与内置不同的清单：只有一个平台，目录叫「假想机」。
const 自己那一份: &str = r#"
"版本" = 1

[['成型规则']]
"名" = "假想机一文件一变体"
"方式" = "同名成组"
"组内扩展名" = ["zip"]

[['平台']]
"名" = "假想机"
"全名" = "Imaginary Console"
"目录" = ["假想机"]
"扩展名" = ["zip"]
"成型" = ["假想机一文件一变体"]
"#;

/// 一份工作目录加一份中立库、一扇开着的窗。
struct 现场 {
    工作区: TempDir,
    app: App,
}

impl 现场 {
    /// 在这个工作目录里建一份中立库、开一扇窗。
    ///
    /// **工作目录里那几份配置是开窗那一刻读的**：要造「读不动」的现场，得在开窗之前就把
    /// 权限收掉（`revoke_file` 的守卫留到这一条测试结束）。
    fn 开在(工作区: TempDir) -> Self {
        drop(Catalog::create(&Self::库文件(&工作区), "fixture").expect("建得出中立库"));
        let site =
            Site::open_file(工作区.path(), &Self::库文件(&工作区), None).expect("开得出现场");
        let app = App::new(site, 工作区.path().to_path_buf());
        Self { 工作区, app }
    }

    fn 库文件(工作区: &TempDir) -> PathBuf {
        工作区.path().join("catalog").join("fixture.sqlite3")
    }

    /// 加一个根、扫一趟，等任务台空了。
    fn 扫(&mut self, 盘: &Path) {
        let (screen, site, _) = self.app.roots_site_and_tasks();
        screen.add_root(site, &盘.to_string_lossy(), "主库");
        let (screen, site, tasks) = self.app.roots_site_and_tasks();
        screen.scan(site, tasks, "主库");
        等任务台空了(&mut self.app);
    }

    /// 按一下「重新体检」，等台上空了。
    ///
    /// **这一趟才是界面自己折报告的那条路**（`health::check_run`）：扫描交回来的那一份是
    /// 扫描那一趟照 `ScanOptions.manifest` 折的，两条路都要走同一份清单。
    fn 重新体检(&mut self) {
        let (screen, site, tasks) = self.app.roots_site_and_tasks();
        screen.check_health(site, tasks);
        等任务台空了(&mut self.app);
    }

    /// 体检报告里「假想机」那一行（没有就当场炸，并列出报告里真有的那几个平台）。
    fn 假想机那一行(&self) -> &romcat_core::report::PlatformStats {
        let report = &self
            .app
            .roots()
            .health()
            .checked()
            .expect("体检过一趟该有一份报告")
            .report;
        report
            .platforms
            .iter()
            .find(|row| row.name == "假想机")
            .unwrap_or_else(|| {
                let 有的: Vec<&str> = report
                    .platforms
                    .iter()
                    .map(|row| row.name.as_str())
                    .collect();
                panic!("体检报告里没有「假想机」那一行，用的还是内置那份清单：{有的:?}")
            })
    }

    /// 这一行是**认得的平台**，不是「未纳入管理的目录」：前者进识别管线（`in_scope`），
    /// 后者只在库体检里照报（ADR-0011 修订段）。这一条判的正是「用的是哪份清单」。
    fn 认得这个平台(&self) {
        assert!(
            self.假想机那一行().in_scope,
            "「假想机」被当成了未纳入管理的目录——用的还是内置那份清单",
        );
        let scope = &self
            .app
            .roots()
            .health()
            .checked()
            .expect("体检过一趟该有一份报告")
            .report
            .scope;
        assert_eq!(
            scope.unmapped_dirs, 0,
            "自己的清单认得「假想机」，未纳入管理的目录该是 0 个：{:?}",
            scope.unmapped_examples,
        );
    }

    /// 开窗那一屏画出来的字。
    fn 开窗屏(&mut self) -> String {
        let ctx = headless::context();
        跑一帧(&ctx, |ui| self.app.ui(ui))
    }
}

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

#[test]
fn 工作目录里那份平台清单说了算_库体检照它出报告() {
    // 票 04 验收第 1 条。从前界面七处一律用内置那一份：同一份库，命令行看见「假想机」，
    // 界面上它落在「未纳入管理」里。
    let 工作区 = temp_dir("gui-清单-工作目录那份");
    fs::write(工作区.path().join("platforms.toml"), 自己那一份).expect("写得下");
    let mut 现场 = 现场::开在(工作区);
    let 盘 = temp_dir("gui-清单-工作目录那份-盘");
    写(&盘.path().join("假想机/某游戏.zip"), &zip(1024));
    现场.扫(盘.path());
    assert!(
        现场.假想机那一行().files > 0,
        "扫描交回来的那份报告里「假想机」一个文件都没数到",
    );
    现场.认得这个平台();

    // **再按一下「重新体检」**：这一趟是界面自己折的报告（`health::check_run`），
    // 从前它写死用内置那一份——扫描那一路改对了、这一路没改的话，按一下就会翻回「未纳入管理」。
    现场.重新体检();
    现场.认得这个平台();
}

#[test]
fn 工作目录里那份平台清单读不动时开窗屏上说出来() {
    // ADR-0021 第三态；票 04 验收第 4 条前半句。从前这一处连读都不读，一律用内置那份。
    // 收权限造不出来（不是 Unix、或者跑测试的是 root）就如实跳过——`revoke_file` 会印那一行。
    let 工作区 = temp_dir("gui-清单-读不动");
    fs::write(工作区.path().join("platforms.toml"), 自己那一份).expect("写得下");
    let Some(_装着) = revoke_file(&工作区.path().join("platforms.toml")) else {
        return;
    };
    let mut 现场 = 现场::开在(工作区);
    let 屏上 = 现场.开窗屏();

    assert!(
        屏上.contains("平台清单读不动"),
        "工作目录里那份平台清单读不动，屏上却一个字都没说：\n{屏上}",
    );
    assert!(
        屏上.contains("眼下用的是内置那一份"),
        "屏上没说清眼下用的是哪一份：\n{屏上}",
    );
    assert!(
        屏上.contains(&romcat_core::path::display(
            &现场.工作区.path().join("platforms.toml")
        )),
        "屏上没说清是哪一份读不动（工作目录里那份 platforms.toml 的路径）：\n{屏上}",
    );
}

#[test]
fn 开窗时优先级表读不动也说出来() {
    // 挂单 `Q976`、票 04 验收第 4 条后半句。从前 `App::new` 那一处是 `if let Ok(priorities)`：
    // 读不动就静默不设，浏览屏留着内置那份，屏上一个字都没有。收权限造不出来就如实跳过。
    let 工作区 = temp_dir("gui-优先级表-读不动");
    let 表 = 工作区.path().join("priorities.toml");
    fs::write(
        &表,
        format!(
            "\"版本\" = {}\n",
            romcat_core::scrape::priority::PRIORITIES_VERSION
        ),
    )
    .expect("写得下");
    let Some(_装着) = revoke_file(&表) else {
        return;
    };
    let mut 现场 = 现场::开在(工作区);
    let 屏上 = 现场.开窗屏();

    assert!(
        屏上.contains("优先级表读不动"),
        "工作目录里那份优先级表读不动，屏上却一个字都没说：\n{屏上}",
    );
    assert!(
        屏上.contains("眼下用的是内置那一份"),
        "屏上没说清眼下用的是哪一份：\n{屏上}",
    );
    assert!(
        屏上.contains(&romcat_core::path::display(&表)),
        "屏上没说清是哪一份读不动（工作目录里那份 priorities.toml 的路径）：\n{屏上}",
    );
}
