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
//! 两态之间**来回都在这儿换**：开场交出一份现场（[`opening::Chosen`]）就换成主窗口；
//! 主窗口上按下那颗「换一份库」（[`App::switching`]）就换回开场。
//!
//! ## 通常见不到开场
//!
//! 开进一份现场的那一下，把那份中立库的完整路径记进[**上次开的那份**](crate::recent)，
//! 下一趟启动直接开它。**开场只在两种时候出现**：记的那份打不开了（挪走、删了、
//! 结构版本对不上），或者人主动要换一份（ADR-0023，词表**开场**那一条）。
//!
//! **参数优先于记忆**：三种给法任说一样（主库根 / `--library <名字>` /
//! `--catalog <文件>`），就走那一条，记着的那份看都不看——人当场说的话比几天前那一次
//! 更清楚。
//!
//! ## 为什么两态在这儿，而不在 [`App`] 里
//!
//! [`App`] 拿的是一份**已经开好的**现场，五屏全建立在「库一定在」这个前提上。让它自己
//! 兜住「还没有库」，就得把现场变成可空的，而那个可空会渗进五屏每一处判空——为的只是
//! 一个冷启动时出现一次的状态。挡在现场之前，五屏一行都不用改（ADR-0023）。

use std::path::PathBuf;

use romcat_core::site::Site;

use crate::app::App;
use crate::opening;
use crate::recent::Recent;
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
    /// 眼下这一趟落在哪个**工作目录**上。
    ///
    /// **换回开场时要它**：开场那一屏列的是「这个工作目录里有哪些库」，而主窗口那一态
    /// 自己说不出这个数——[`App`] 把工作目录发给了三屏各自留着，没有一处再交得回来。
    workspace: PathBuf,
    /// [**上次开的那份**](crate::recent)记在哪儿。
    ///
    /// **整趟拿着不放**：启动时读一次，往后每换一份库写一次（换库这件事在窗口活着的
    /// 时候发生），两处走的必须是同一份文件。
    recent: Recent,
}

impl Program {
    /// 走完启动那条路：定位要开哪份库、开出现场、进主窗口。
    ///
    /// 工作目录不另外要一个——它从这三种给法里折出来（[`Locate::workspace_dir`]），
    /// 两处各算一遍迟早对不上。
    ///
    /// 三种给法一样都没给时——双击图标那一下——先看[**上次开的那份**](crate::recent)：
    /// 记着的那一份直接开进主窗口，没记过或者记的那份打不开才走**开场**（这个工作目录
    /// 里有哪些库，挑一份开进去。ADR-0023）。**不擅自造一份假的**：合成数据与真库在
    /// 界面上长得一模一样，看见一屏假名字的第一反应是「我的库怎么了」而不是「我打开的
    /// 不是我的库」，而**不报错的错比报错的错难查得多**。
    ///
    /// # Errors
    /// 给了要开哪一份、但库不在或者打不开时，返回核心库那句给人看的话。**一样都没给
    /// 不是错**——那是开场。
    pub fn start(locate: &Locate<'_>) -> Result<Self, String> {
        Self::start_with(locate, Recent::here())
    }

    /// 同 [`Self::start`]，只是[**上次开的那份**](crate::recent)记在哪儿由调用方说。
    ///
    /// **测试要它**：[`Recent::here`] 折出来的那一处是维护者机器上真的那一份
    /// （默认工作目录底下），一条测试都不许读它、更不许写它。
    ///
    /// # Errors
    /// 同 [`Self::start`]。**记的那份打不开不是错**——那是退回开场，并在那一屏上说清
    /// 是哪一条原因。
    pub fn start_with(locate: &Locate<'_>, recent: Recent) -> Result<Self, String> {
        // **参数优先于记忆**（验收第 7 条）：人当场说了要开哪一份，就开哪一份。
        if locate.given() {
            let workspace = locate.workspace_dir();
            return Ok(Self::opened_with(locate.open()?, workspace, recent));
        }
        // **对不上他说的那个工作目录就不算数**（[`Locate::covers`]）：`--workspace` 说的
        // 是「这一趟在这儿干活」，而记着的那份可能落在别处。
        let Some(记的那份) = recent.read().filter(|catalog| locate.covers(catalog)) else {
            return Ok(Self::opening_with(locate.workspace_dir(), recent));
        };
        // **走 [`Locate`] 那条现成的路**：`--catalog` 独有的那段推断（从
        // `工作目录/catalog/某某.sqlite3` 反推工作目录）就在它上面，而记下一条完整路径
        // 正是为了白拿这个（验收第 2 条）。另写一条推断只会与它分家。
        let 上次那条 = Locate::at_catalog(&记的那份);
        let workspace = 上次那条.workspace_dir();
        match 上次那条.open() {
            Ok(site) => Ok(Self::opened_with(site, workspace, recent)),
            // **退回开场，并说清是哪一条原因**（验收第 4 条）：挪走了、删了、结构版本
            // 对不上、还是别的打不开——那句话由核心库一处出，这一层一个字都不改写
            // （ADR-0005）。人期待的是直接进主窗口，突然看见开场就得当场说得出为什么。
            Err(说的) => {
                let mut screen = opening::Screen::new(workspace.clone());
                screen.explain(format!("上次开的那份现在开不了：{说的}"));
                Ok(Self {
                    stage: Stage::Opening(screen),
                    workspace,
                    recent,
                })
            }
        }
    }

    /// 进**开场**：列出这个工作目录里有哪些中立库，挑一份开进去。
    ///
    /// 它是**公开**的，而且开场随时进得来第二次：票 `gui-self-sufficient/05` 的
    /// 「认领新主库」就长在这一屏上。**主窗口里那条回开场的路不走它**——那一条要把
    /// 「上次开的那份记在哪儿」原样带过去（[`Self::ui`]），而这个构造子只认那个固定的
    /// 位置。
    #[must_use]
    pub fn opening(workspace: PathBuf) -> Self {
        Self::opening_with(workspace, Recent::here())
    }

    /// 同 [`Self::opening`]，只是那份记忆记在哪儿由调用方说。
    fn opening_with(workspace: PathBuf, recent: Recent) -> Self {
        Self {
            stage: Stage::Opening(opening::Screen::new(workspace.clone())),
            workspace,
            recent,
        }
    }

    /// 进主窗口，**并把这一份记下来**（验收第 1、6 条）。
    ///
    /// 记的是那份中立库的完整路径。**只活在内存里的那一份不记**（合成数据走的正是
    /// 这条）：它没有文件，记下去下一趟也开不出来。
    fn opened_with(site: Site, workspace: PathBuf, recent: Recent) -> Self {
        if let Some(catalog) = site.catalog.file() {
            recent.remember(catalog);
        }
        Self {
            stage: Stage::Opened(Box::new(App::new(site, workspace.clone()))),
            workspace,
            recent,
        }
    }

    /// 现场已经在手上时直接进主窗口。
    ///
    /// **合成数据那一路专用**：那份现场是造出来的（`demo::site`），压根不走定位那一段，
    /// 而且进窗口之前还要把标题里那个库名换掉（[`App::set_library_label`]）。
    ///
    /// **工作目录要跟着一起给**：那一路的工作目录是个临时目录（`demo::workspace`），
    /// 而这一层拿它去开「换一份库」回到的那个开场——自己折一个默认的出来，人就会在演示
    /// 窗口里按一下、然后对着**别处**那个目录里的库发愣。
    ///
    /// **那份记忆也要跟着一起给**，而不是在这儿悄悄折一个 [`Recent::here`]：这一路自己
    /// 一个字都不记（合成数据没有中立库文件），但从它按「换一份库」开进去的那一份**是真
    /// 的库**，那一下会真的落盘。让调用方明说记在哪儿，写的是哪一份文件就摆在调用处
    /// 看得见——测试也才拦得住它去碰维护者真正的那一份。
    #[must_use]
    pub fn opened(app: App, workspace: PathBuf, recent: Recent) -> Self {
        Self {
            stage: Stage::Opened(Box::new(app)),
            workspace,
            recent,
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
    /// **两态之间那一下换在这儿，来回都是**：开场挑中一份并开出现场，这一帧末尾就换成
    /// 主窗口，开场自己退场；主窗口上按下「换一份库」，这一帧末尾就换回开场——**换回去
    /// 那一下不重开一个程序**，那份记忆原样带着走，不然在主窗口里换的库记不下来。
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let mut 开出来的 = None;
        let mut 要回开场 = false;
        match &mut self.stage {
            Stage::Opening(opening) => 开出来的 = opening.ui(ui),
            Stage::Opened(app) => {
                app.ui(ui);
                要回开场 = app.switching();
            }
        }
        if let Some(chosen) = 开出来的 {
            *self = Self::opened_with(chosen.site, chosen.workspace, self.recent.clone());
        } else if 要回开场 {
            // **回的是这一趟正开着的那个工作目录**：人要换的多半是隔壁那一份，
            // 而不是默认那条链折出来的那个。**那份记忆原样带走**——不然在主窗口里换的库
            // 记不下来。
            *self = Self::opening_with(self.workspace.clone(), self.recent.clone());
        }
    }
}

impl eframe::App for Program {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        Program::ui(self, ui);
    }
}
