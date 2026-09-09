//! **开场**：五屏之外的那一屏。
//!
//! 挑一份**现场**开进去，或者认领一个新主库。它交出现场之后自己退场——五屏都建立在
//! 「库一定在」这个前提上，开场就是守住那个前提的地方（`CONTEXT.md` 词表、ADR-0023）。
//!
//! ## 这一屏一条领域判断都没有
//!
//! 列一个**工作目录**里有哪些**中立库**、比对结构版本、读出主库名，三样全在核心库
//! （[`workspace::catalogs`]）。这儿只把要来的画出来（ADR-0005 的红线）。
//! **结构版本对不上的那一份照列不误**，而那句「版本 X，本程序认得的是 Y」也是核心库
//! 交出来的原话，界面一个字都不改写——两处各写一套措辞，人在终端里看见的与在界面上
//! 看见的迟早分家。
//!
//! ## 那三个数为什么盘没挂上也在
//!
//! 主库名、变体数、上次扫描时刻全住在**中立库**里，不在主库上（ADR-0009）。外置盘
//! 不常挂载（ADR-0018 的双机工作流），而开场正是盘不在位时最常看见的那一屏。

use std::path::{Path, PathBuf};

use romcat_core::path;
use romcat_core::report::{human_time, thousands};
use romcat_core::site::Site;
use romcat_core::workspace::{self, CatalogEntry};

/// 这个工作目录里一份中立库都没有时说的那句话。
///
/// **它指向下一步**，不是一句「空」：认领一个主库才是这时候唯一做得下去的事
/// （那个入口是票 `gui-self-sufficient/05` 的活）。
pub const NO_CATALOG: &str = "还没有库，认领一个主库开始";

/// 换工作目录那个框里的提示字。
const WORKSPACE_HINT: &str = "换一个工作目录：把路径贴在这儿";

/// 开场交出来的东西：一份开好的**现场**，连它住在哪个**工作目录**里。
///
/// 两样一起交，是因为 [`App::new`](crate::app::App::new) 两样都要，而工作目录**不能
/// 由拿到它的人再算一遍**——开场上可以换工作目录，再算一遍算出来的会是换之前那个。
pub struct Chosen {
    /// 开好的现场：中立库、沉淀库、这份主库在**路径锚**里叫什么名字。
    pub site: Site,
    /// 它住在哪个工作目录里。
    pub workspace: PathBuf,
}

/// 开场那一屏。
pub struct Screen {
    /// 眼下在看哪个**工作目录**。
    workspace: PathBuf,
    /// 那个目录里的中立库，**列一次记下来**。
    ///
    /// 不每帧现列：列一遍要把目录里每一份库都开一趟（[`workspace::catalogs`]），
    /// 那是几次到几十次磁盘往返，一秒钟做六十遍是拿开场那一屏当磁盘压力测试。
    /// 换了工作目录才重列——而那正是列表内容会变的唯一时机。
    catalogs: Vec<CatalogEntry>,
    /// 换工作目录那个框里正打着的字。
    draft: String,
    /// 上一次开库没开成时说的那句话。
    ///
    /// 与「这一份读不开」不是一回事：那一条跟着行走（[`CatalogEntry::facts`]），
    /// 这一条是**按下「打开」之后**才知道的（文件在列出来之后被挪走了、被别人占着）。
    error: Option<String>,
    /// **退回这一屏的原因**：上次开的那份打不开了，这是那句原话。
    ///
    /// 与[按下「打开」之后那一句](Self::error)分开摆，因为它们**知道的时机**不同：
    /// 这一条在这一屏画出来之前就已经知道了，画在表**上头**人第一眼就看得见；那一条
    /// 要等人按下去才知道，画在上头的话人得多等一帧才看见它。
    fallback_reason: Option<String>,
}

impl Screen {
    /// 进开场：把这个工作目录里的库列出来。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        let catalogs = workspace::catalogs(&workspace);
        Self {
            workspace,
            catalogs,
            draft: String::new(),
            error: None,
            fallback_reason: None,
        }
    }

    /// 摆一句「**为什么你会看见这一屏**」在抬头底下。
    ///
    /// [`Program`](crate::program::Program) 拿它交进来那句核心库的原话：上次开的那份
    /// 打不开时，人期待的是直接进主窗口，突然看见开场就得当场说得出是哪一条原因——
    /// 挪走了、删了、还是结构版本对不上（票 `gui-self-sufficient/04` 验收第 4 条）。
    pub fn explain(&mut self, 说的: String) {
        self.fallback_reason = Some(说的);
    }

    /// 换一个工作目录，**立刻**列出那个目录里的库。
    ///
    /// 不换的话，那些不在默认位置的库一份都开不出来——默认工作目录那条链
    /// （[`workspace::default_dir`]）认的是环境变量，而盘在两台机器之间来回接时
    /// 它常常不是人想开的那一个。
    pub fn look_at(&mut self, workspace: PathBuf) {
        self.workspace = workspace;
        self.error = None;
        // **换了目录那句话就作废**：它说的是上次开的那份在**原来那个**目录里出了什么事。
        self.fallback_reason = None;
        self.catalogs = workspace::catalogs(&self.workspace);
    }

    /// 画一帧。挑中了一份并且开得出来时，交出那份**现场**——开场到此退场。
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<Chosen> {
        ui.heading("开场");
        ui.weak("挑一份库开进去。底下这几个数住在中立库里，外置盘不在位照样看得见");
        // **它画在表上头**：这一句回答的是「我明明开过库了，怎么又看见这一屏」，
        // 那个问题在人找那份库之前就问出口了。
        if let Some(说的) = &self.fallback_reason {
            ui.colored_label(ui.visuals().warn_fg_color, 说的);
        }
        ui.separator();

        self.workspace_ui(ui);
        ui.add_space(12.0);

        let 要开的 = self.catalogs_ui(ui);
        let 开出来的 = 要开的.and_then(|catalog| self.open(&catalog));
        // **那句话画在表底下**，不是画在表上头：它是**按下「打开」之后**才知道的，
        // 而这一帧的按下发生在表画完的那一刻——画在上头的话，人要等下一帧才看见它。
        if let Some(error) = &self.error {
            ui.add_space(6.0);
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        开出来的
    }

    /// 抬头那一行：现在看的是哪个工作目录，以及换一个。
    ///
    /// **票 `gui-self-sufficient/05` 的「认领新主库」挂在这一行**：开场不是一个只能
    /// 选中已有库的列表，它同时是「这个工作目录里从零开始」的入口。
    fn workspace_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("工作目录");
            ui.label(path::display(&self.workspace));
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.draft)
                    .hint_text(WORKSPACE_HINT)
                    .desired_width(420.0),
            );
            // 空着按下去什么都不该发生：那不是「换到根目录」，是还没打完字。
            let 打了字 = !self.draft.trim().is_empty();
            if ui
                .add_enabled(打了字, egui::Button::new("换过去"))
                .clicked()
            {
                let 换到 = PathBuf::from(self.draft.trim());
                self.draft.clear();
                self.look_at(换到);
            }
        });
    }

    /// 那张表：一份库一行。挑中了哪一份就交出它的文件路径。
    fn catalogs_ui(&mut self, ui: &mut egui::Ui) -> Option<PathBuf> {
        if self.catalogs.is_empty() {
            ui.label(NO_CATALOG);
            return None;
        }

        let mut 要开的 = None;
        for 一份 in &self.catalogs {
            ui.horizontal(|ui| {
                // **开不进去的那一份按钮按不下去，但那一行照样在。** 从列表里静静消失
                // 才是最难查的那种错（ADR-0023）。
                //
                // 按的是 `openable` 而**不是**「那几个数读出来了没有」：开进去了、
                // 只是数没读回来的那一份照样开得进去，把两件事并成一件就会把它画成
                // 一份坏库。
                if ui
                    .add_enabled(一份.openable, egui::Button::new("打开"))
                    .clicked()
                {
                    要开的 = Some(一份.path.clone());
                }
                ui.vertical(|ui| {
                    ui.strong(&一份.name);
                    match &一份.facts {
                        Ok(facts) => {
                            ui.weak(format!(
                                "{} 个变体 · {}",
                                thousands(facts.variants),
                                facts.scanned_at.map_or_else(
                                    || "还没扫过".to_string(),
                                    |at| format!("上次扫描 {}", human_time(at)),
                                ),
                            ));
                        }
                        // 核心库那句话原样画出来：它自己就说清了是哪个版本对哪个版本、
                        // 该怎么办。
                        Err(说的) => {
                            ui.colored_label(ui.visuals().warn_fg_color, 说的);
                        }
                    }
                });
            });
            ui.add_space(6.0);
        }
        要开的
    }

    /// 开这一份。开不出来就把那句话摆到屏上，**留在开场**。
    fn open(&mut self, catalog: &Path) -> Option<Chosen> {
        match Site::open_file(&self.workspace, catalog, None) {
            Ok(site) => {
                self.error = None;
                Some(Chosen {
                    site,
                    workspace: self.workspace.clone(),
                })
            }
            Err(error) => {
                self.error = Some(format!("{error}"));
                None
            }
        }
    }
}
