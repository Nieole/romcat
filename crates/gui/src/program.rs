//! **启动那条路**，与它交出来的那两态。
//!
//! 程序入口只剩两件事：解析参数、开窗。中间那一段——定位要开哪份库、开出**现场**、
//! 进主窗口——住在这儿。抬上来是为了**它够得着测试**：从前那段摊在 `main.rs` 里，
//! 「三种给法（主库根 / `--library <名字>` / `--catalog <文件>`）行为一致」只能靠人手
//! 敲三遍命令，而这几条正是维护者第一次打开这个工具时走的路。
//!
//! ## 两态
//!
//! - **开场中**：还没有现场。**这一版是个空位**——[`Program::start`] 眼下走不到它，
//!   一个参数都不给时照旧报「说清要开哪份库」并退出。填它是票
//!   `gui-self-sufficient/03` 的事：列出这个**工作目录**里已有的中立库、认领一个新主库、
//!   或者换一个工作目录。
//! - **已开库**：现场开好了，[`App`] 在画那五屏。
//!
//! ## 为什么两态在这儿，而不在 [`App`] 里
//!
//! [`App`] 拿的是一份**已经开好的**现场，五屏全建立在「库一定在」这个前提上。让它自己
//! 兜住「还没有库」，就得把现场变成可空的，而那个可空会渗进五屏每一处判空——为的只是
//! 一个冷启动时出现一次的状态。挡在现场之前，五屏一行都不用改（ADR-0023）。

use crate::app::App;
use crate::site::Locate;

/// 一个参数都不给时说的那句话。**不擅自造一份假的**。
///
/// 合成数据与真库在界面上长得一模一样，于是不带参数打开看见的会是一屏假名字，
/// 第一反应是「我的库怎么了」而不是「我打开的不是我的库」——**不报错的错比报错的错
/// 难查得多**。
///
/// 票 `gui-self-sufficient/03` 会把这句话换成**开场**那一屏：那时这条路不再是死路，
/// 而是**开场中**那一态。**这一张不改它**。
pub const NO_LIBRARY: &str = "说清要开哪份库：\n\
     \x20 romcat-gui <主库根>\n\
     \x20 romcat-gui --library <名字> [--workspace <目录>]\n\
     \x20 romcat-gui --catalog <中立库文件>\n\
     \n\
     只想看看界面长什么样：`cargo run -p romcat-gui --features demo -- --demo`。";

/// 走到哪一态了。
///
/// 摆成一个**私有**的枚举：外面认得 [`Program`]，但拿不到它此刻是哪一态——于是测试
/// 只能验人看得见的东西（标题写着什么、屏上画出了什么），戳不进来。
enum Stage {
    /// **开场中**：还没有现场，开场那一屏摆在窗口里。
    ///
    /// 这一版**只留位、不构造**：一个参数都不给时照旧报 [`NO_LIBRARY`] 并退出
    /// （票 `gui-self-sufficient/03` 才把那条路引到这儿来）。填它的人把开场那一屏挂在
    /// 这一支上，并把底下 `#[expect]` 那一行撤掉——撤晚了编译器会提醒。
    #[expect(
        dead_code,
        reason = "开场那一屏是票 `gui-self-sufficient/03` 的活：这一趟只留位子，不构造它"
    )]
    Opening,
    /// **已开库**：现场开好了，主窗口在画。
    ///
    /// **装箱**：开场那一态几乎不占地方，主窗口那一态是整扇窗（五屏各自的缓存都在
    /// 里头），两态直接并排的话这个枚举永远按大的那一态算尺寸——而窗口本身早已经在
    /// 堆上了（`eframe::run_native` 收的就是一个 `Box`），多这一次装箱不多花一分钱。
    Opened(Box<App>),
}

/// 程序本体：两态——**开场中** / **已开库**。
///
/// `eframe` 拿的是它而不是 [`App`]：换库这件事要在窗口活着的时候完成（开场交出一份
/// 现场、换成主窗口），而那得有个东西同时够得着两态。
pub struct Program {
    stage: Stage,
}

impl Program {
    /// 走完启动那条路：定位要开哪份库、开出现场、进主窗口。
    ///
    /// 工作目录不另外要一个——它从这三种给法里折出来（[`Locate::workspace_dir`]），
    /// 两处各算一遍迟早对不上。
    ///
    /// # Errors
    /// 三种给法一样都没给时返回 [`NO_LIBRARY`]；给了但库不在、或者打不开时，返回核心库
    /// 那句给人看的话。
    pub fn start(locate: &Locate<'_>) -> Result<Self, String> {
        // 三样都不给就如实报错。**票 `gui-self-sufficient/03` 改的是这一句**：那时它
        // 交出的是 `Stage::Opening` 而不是一条错。
        if !locate.given() {
            return Err(NO_LIBRARY.to_string());
        }
        Ok(Self::opened(App::new(
            locate.open()?,
            locate.workspace_dir(),
        )))
    }

    /// 现场已经在手上时直接进主窗口。
    ///
    /// **合成数据那一路专用**：那份现场是造出来的（`demo::site`），压根不走定位那一段，
    /// 而且进窗口之前还要把标题里那个库名换掉（[`App::set_library_label`]）。
    #[must_use]
    pub fn opened(app: App) -> Self {
        Self {
            stage: Stage::Opened(Box::new(app)),
        }
    }

    /// 窗口标题。
    ///
    /// **开窗到第一帧之间**那一小会儿靠它：第一帧一画，[`App`] 就把带屏名的那个标题发
    /// 下来了。开场那一态还没有库也还没有屏，于是只剩程序自己的名字。
    #[must_use]
    pub fn window_title(&self) -> String {
        match &self.stage {
            Stage::Opening => "romcat".to_string(),
            Stage::Opened(app) => app.window_title(),
        }
    }

    /// 画一帧。`eframe` 与不开窗跑帧的那条路走的是同一个函数。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        match &mut self.stage {
            // 开场那一屏还没有人画（票 `gui-self-sufficient/03`）。
            Stage::Opening => {}
            Stage::Opened(app) => app.ui(ui),
        }
    }
}

impl eframe::App for Program {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Program::ui(self, ui);
    }
}
