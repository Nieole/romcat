//! **启动那条路**，与它交出来的那两态。
//!
//! 程序入口只剩两件事：解析参数、开窗。中间那一段——定位要开哪份库、开出**现场**、
//! 进主窗口——住在这儿。抬上来是为了**它够得着测试**：从前那段摊在 `main.rs` 里，
//! 「三种给法（主库根 / `--library <名字>` / `--catalog <文件>`）行为一致」只能靠人手
//! 敲三遍命令，而这几条正是维护者第一次打开这个工具时走的路。
//!
//! ## 两态
//!
//! - **开场中**：还没有现场，[**开场**](crate::opening)那一屏摆在窗口里——列出这个
//!   **工作目录**里已有的中立库，挑一份开进去，或者换一个工作目录。一个参数都不给
//!   （双击图标）走的就是这条路：**界面自足，从第一步起不必开终端**（ADR-0023）。
//! - **已开库**：现场开好了，[`App`] 在画那五屏。
//!
//! 两态之间只有一条路：开场交出一份现场（[`opening::Chosen`]），这儿把它换成主窗口。
//! **换回去也走同一个构造子**（[`Program::opening`]）——票 `gui-self-sufficient/04`
//! 的「回开场换一份库」接的就是它。
//!
//! ## 为什么两态在这儿，而不在 [`App`] 里
//!
//! [`App`] 拿的是一份**已经开好的**现场，五屏全建立在「库一定在」这个前提上。让它自己
//! 兜住「还没有库」，就得把现场变成可空的，而那个可空会渗进五屏每一处判空——为的只是
//! 一个冷启动时出现一次的状态。挡在现场之前，五屏一行都不用改（ADR-0023）。

use std::path::PathBuf;

use crate::app::App;
use crate::opening;
use crate::site::Locate;

/// 走到哪一态了。
///
/// 摆成一个**私有**的枚举：外面认得 [`Program`]，但拿不到它此刻是哪一态——于是测试
/// 只能验人看得见的东西（标题写着什么、屏上画出了什么），戳不进来。
enum Stage {
    /// **开场中**：还没有现场，[开场](crate::opening)那一屏摆在窗口里。
    ///
    /// **不装箱**：它就是一个工作目录、一小批列出来的库行和一个输入框，与旁边那一态
    /// （整扇窗）差着两三个数量级，装不装箱这个枚举的尺寸都由 `Opened` 那一支说了算。
    Opening(opening::Screen),
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
    /// 三种给法一样都没给时——双击图标那一下——走的是**开场**：这个工作目录里有哪些
    /// 库，挑一份开进去（ADR-0023）。**不擅自造一份假的**：合成数据与真库在界面上长得
    /// 一模一样，看见一屏假名字的第一反应是「我的库怎么了」而不是「我打开的不是我的
    /// 库」，而**不报错的错比报错的错难查得多**。
    ///
    /// # Errors
    /// 给了要开哪一份、但库不在或者打不开时，返回核心库那句给人看的话。**一样都没给
    /// 不是错**——那是开场。
    pub fn start(locate: &Locate<'_>) -> Result<Self, String> {
        if !locate.given() {
            return Ok(Self::opening(locate.workspace_dir()));
        }
        Ok(Self::opened(App::new(
            locate.open()?,
            locate.workspace_dir(),
        )))
    }

    /// 进**开场**：列出这个工作目录里有哪些中立库，挑一份开进去。
    ///
    /// 它是**公开**的，而且开场随时进得来第二次：票 `gui-self-sufficient/04` 的
    /// 「在主窗口里主动回到开场换一份库」接的就是这个构造子。
    #[must_use]
    pub fn opening(workspace: PathBuf) -> Self {
        Self {
            stage: Stage::Opening(opening::Screen::new(workspace)),
        }
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
    /// 下来了。开场那一态还没有库，标题里于是只剩它自己那一屏的名字。
    #[must_use]
    pub fn window_title(&self) -> String {
        match &self.stage {
            Stage::Opening(_) => "romcat — 开场".to_string(),
            Stage::Opened(app) => app.window_title(),
        }
    }

    /// 画一帧。`eframe` 与不开窗跑帧的那条路走的是同一个函数。
    ///
    /// **两态之间那一下换在这儿**：开场挑中一份并开出现场，这一帧末尾就换成主窗口，
    /// 开场自己退场。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let 开出来的 = match &mut self.stage {
            Stage::Opening(opening) => opening.ui(ui),
            Stage::Opened(app) => {
                app.ui(ui);
                None
            }
        };
        if let Some(chosen) = 开出来的 {
            self.stage = Stage::Opened(Box::new(App::new(chosen.site, chosen.workspace)));
        }
    }
}

impl eframe::App for Program {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Program::ui(self, ui);
    }
}
