//! 库屏底下的**库体检**那一块（票 `gui-looks-like-the-design/27`）：对主库跑一遍只读扫描得到的报告，
//! 摆成一块概要，每一格点进去看明细。
//!
//! ## 只报告、不处理
//!
//! 体检是只读检查（词表**库体检**）：重复拷贝只发现不删除，主库一个字节都不写（ADR-0004）。这一块上
//! 没有任何改动主库的入口，要清理请人自己去文件系统里做。
//!
//! ## 数不在这里算
//!
//! 每一格的数、明细与判据都在核心库的体检报告里（`romcat_core::report::HealthReport`、`Finding`），这一层只画
//! （ADR-0005、ADR-0024）。疑似同一作品那一格的判断在票 17，这之前一个数都不画。
//!
//! ## 报告从哪来（拿主意的人 2026-09-15 答岔路口 8）
//!
//! - **扫完一个根**，就用扫描交回的那一份：它本来就是从中立库折出来的全库那一份。
//! - **开窗后头一次进库屏、且有扫过的根时**，自动排一趟只读体检（`Section::auto_check`）；「重新体检」排的是同一趟
//!   （[`Section::check`]）。**一次只跑一趟**：台上有体检时不再排。
//! - 那一趟**整条只读**：后台那条线程读的是同一个库文件的第二份只读连接，只活在内存里的库就地跑完——与子库屏「算一遍容量」、
//!   工序段「算一遍要铺多少媒体」同一条路。
//!
//! 标题栏（「库体检」、说明、「重新体检」、折叠标）与面板的样子由库屏摆（`roots::Screen`，与根、数据源、导出设置
//! 三块同一个画法），这里画正文。

use std::path::{Path, PathBuf};

use romcat_core::catalog::{Catalog, CatalogError, Roots};
use romcat_core::platform::Manifest;
use romcat_core::report::{DuplicateDetails, Finding, HealthReport, human_bytes, thousands};
use romcat_core::scan::ScanOutcome;
use romcat_core::scan::aggregate::Limits;
use romcat_core::site::Site;
use romcat_core::task::{Cutoff, Ending, Finished, Handle};

use crate::clock::Clock;
use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;
use crate::look::{self, Tone};
use crate::table;
use crate::task::{Product, Tasks};
use crate::tokens::Tokens;

/// 标题栏里那句说明（设计稿 `#health-sub` 原话）。
pub const READ_ONLY: &str = "只读检查，只报告、不处理";

/// 标题栏右边那颗按钮上的字（设计稿 `data-act="task:health"`）。
pub const RECHECK: &str = "重新体检";

/// 体检那一趟在台上时，标题栏那句说明换成的这一句（设计稿 `renderHealth` 原话）。
pub const CHECKING: &str = "正在体检…";

/// 体检那一趟在任务台上叫什么（设计稿 `TASKS.health` 原话）。测试按它在台上找那一趟。
pub const CHECK_TASK: &str = "库体检 · 全部根";

/// 还没扫描时这一块画的那一句（设计稿 `renderHealth` 空态原话）。
pub const BEFORE_SCAN: &str = "扫描完成后生成体检报告。";

/// 疑似同一作品那一格的标题（设计稿原话）。
pub const SAME_WORK: &str = "疑似同一作品";

/// 疑似同一作品那一格在**识别还没做完**时底下那句（票 27 原话）。
pub const SAME_WORK_BEFORE_IDENTIFY: &str = "识别完成后才有";

/// 疑似同一作品那一格在**识别做完之后**底下那句实话：判断还没接上，界面不自己算一个（拿主意的人 2026-09-15 答岔路口 9）。
pub const SAME_WORK_NOT_YET: &str = "暂时还给不出这一项";

/// 识别还没做完时点疑似同一作品那一格，八格底下说的那一句（设计稿点那一格时那句提示，照票改成「识别完成后才有」）。
pub const SAME_WORK_CLICKED_BEFORE_IDENTIFY: &str = "识别完成后才有疑似同一作品的建议。";

/// 识别做完之后点疑似同一作品那一格，八格底下说的那一句实话：这一项的判断还没接上，不跳到浏览屏去（挂单 `Q957`）。
pub const SAME_WORK_CLICKED_NOT_YET: &str = "暂时还给不出疑似同一作品的建议。";

/// 一份体检的结果。
#[derive(Debug)]
pub struct Checked {
    /// 体检报告（核心库折的全库那一份）。
    pub report: HealthReport,
    /// 同一份统计折出来的重复拷贝完整明细：每一组、记下的每一份（[`DuplicateDetails`]）。
    pub duplicates: DuplicateDetails,
    /// 什么时候出的，UNIX 纪元起的秒：「上次体检」画的就是它。
    pub at: i64,
}

/// 库屏上的**库体检**那一块。
#[derive(Debug, Default)]
pub struct Section {
    /// 眼下画着的那一份；还没体检过是 `None`。
    checked: Option<Checked>,
    /// 台上那一趟体检的任务号（排着队也算）。**一次只跑一趟**，按它挡。
    running: Option<u64>,
    /// 开窗之后问没问过「要不要自动体检一趟」（[`Self::auto_check`]）：只在头一次进库屏时问。
    asked: bool,
    /// 上一趟体检没成的那句话。
    error: Option<String>,
    /// 眼下开着的那一层明细弹层；没开是 `None`。
    detail: Option<Detail>,
    /// 明细弹层里按下去之后那句回话，画在弹层里：`Ok` 是办成了（清单写到了哪儿），`Err` 是没办成（打不开那个目录、
    /// 落点在主库里……）。
    said: Option<Result<String, String>>,
    /// 点了疑似同一作品那一格之后，画在八格底下的那一句（不跳屏、不开明细，岔路口 9）。
    notice: Option<String>,
}

/// 明细弹层里「在文件系统中打开」那颗按钮上的字（设计稿原话）。
pub const OPEN_IN_FILES: &str = "在文件系统中打开";

/// 那块盘不在位时那颗按钮按不下去，悬停说的这一句（词表：盘没插上写「未连接」）。
const NOT_MOUNTED: &str = "未连接：那块盘眼下不在位，打不开";

/// 重复拷贝明细那张表最多摆几组；其余的在重复拷贝明细全文里（导出去的那一份列全部）。
const DUPLICATE_ROWS: usize = 50;

/// 眼下开着的那一层明细弹层。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Detail {
    /// 哪一格。
    finding: Finding,
    /// 重复拷贝那张表里摊开着的是第几组。
    expanded: Option<usize>,
}

/// 明细弹层页脚上按下去的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    /// 关掉这一层。
    Close,
    /// 「导出清单…」：弹保存对话框，把这一格的明细写成纯文本。
    Export,
}

/// 明细弹层页脚上那颗按钮上的字，与保存对话框的标题（设计稿原话「导出清单…」，拿主意的人 2026-09-15 照稿定岔路口 5：
/// 这里的「清单」指体检明细写出去的那份文本，不是子库的清单——词表「清单」条写着这一处）。
pub const EXPORT: &str = "导出清单…";

/// 明细弹层标题底下那句说明（设计稿 `HD` 那几句，与词表对不上的照实改）。
fn note_of(finding: Finding) -> &'static str {
    match finding {
        Finding::Duplicates => {
            "同名且同大小的文件存在多份——判据不读内容，动手前请自己核一眼。只发现并报告，绝不自动删除；主库只读，\
             要清理请在文件系统中手动处理。同一个作品的不同转储是不同的变体，不算重复拷贝。"
        }
        Finding::PlatformConflicts => {
            "目录说的平台与文件内容说的不一致。目录只是强先验，内容可以推翻它；这里只报告，不改动任何文件。"
        }
        Finding::ShapingDoubts => {
            "成型规则把文件聚成变体时拿不准的地方。不处理也不影响使用；这里只报告，不改动任何文件。"
        }
        Finding::Unreadable => {
            "文件名拿得到，但元数据读不出来。它们如实记为「不可读」，既不算已变，也不算已删。"
        }
        Finding::UnmappedDirs => {
            "这些目录不在任何平台目录下：扫描照常读它们，库体检照样列出来，只是不进识别与刮削。列在这里，作为整理的依据。"
        }
        Finding::StrandedCompanions => {
            "存档、补丁这类附属文件在自己那个目录里找不到同名的主文件，没有归入任何变体。这里只报告，不改动任何文件。"
        }
        Finding::NonGameAssets => {
            "模拟器需要、但本身不是游戏的文件。入库但永不导出；在「浏览」中打开「显示非游戏资产」可以查看。"
        }
    }
}

impl Section {
    /// 扫完一个根：拿扫描交回的那份报告，连同一份统计折出重复拷贝的完整明细。`at` 是认领那一刻。
    pub fn take_scan(&mut self, outcome: &ScanOutcome, at: i64) {
        self.error = None;
        self.checked = Some(Checked {
            report: outcome.report.clone(),
            duplicates: DuplicateDetails::build(&outcome.aggregate, &outcome.report),
            at,
        });
    }

    /// 眼下画着的那一份；还没体检过是 `None`。测试拿它核对。
    #[must_use]
    pub fn checked(&self) -> Option<&Checked> {
        self.checked.as_ref()
    }

    /// 台上有没有一趟体检（排着队也算）。
    #[must_use]
    pub fn running(&self) -> bool {
        self.running.is_some()
    }

    /// 标题栏里那句说明：台上有体检时说正在体检；体检过就带上上次体检的时刻（本地短格式，[`Clock::short`]）。
    #[must_use]
    pub fn subtitle(&self, clock: Clock) -> String {
        if self.running.is_some() {
            return CHECKING.to_string();
        }
        match &self.checked {
            Some(checked) => format!("上次体检 {} · {READ_ONLY}", clock.short(checked.at)),
            None => READ_ONLY.to_string(),
        }
    }

    /// **排一趟体检上任务台**：「重新体检」按的就是它。台上已经有一趟就什么都不做——一次只跑一趟。
    ///
    /// 后台那条线程读的是同一个库文件的第二份只读连接（[`Catalog::read_only`]）；只活在内存里的库分不出第二份，就地跑完。
    pub fn check(&mut self, site: &Site, tasks: &mut Tasks) {
        if self.running.is_some() {
            return;
        }
        let id = match site.catalog.read_only() {
            Ok(reader) => tasks.queue(CHECK_TASK, move |task| check_run(&reader, task)),
            Err(CatalogError::NotOnDisk { .. }) => {
                tasks.run_here(CHECK_TASK, |task| check_run(&site.catalog, task))
            }
            // 别的原因是**意外**：直说，不退到画帧这条线程上偷偷算一遍（同工序段算要铺多少媒体那一处）。
            Err(why) => {
                self.error = Some(format!(
                    "体检没排上：读中立库要另开一份只读连接，这一下没开出来（{why}）"
                ));
                return;
            }
        };
        self.error = None;
        self.running = Some(id);
    }

    /// **开窗后头一次进库屏**时问一次：有扫过的根、还没有报告、台上也没有体检，就自动排一趟（拿主意的人 2026-09-15
    /// 答岔路口 8）。只问一次——之后的报告由扫描交回、或者人按「重新体检」。
    pub(crate) fn auto_check(&mut self, site: &Site, tasks: &mut Tasks, scanned: bool) {
        if std::mem::replace(&mut self.asked, true) {
            return;
        }
        if scanned && self.checked.is_none() {
            self.check(site, tasks);
        }
    }

    /// 任务台交回来的是不是体检那一趟：是就认领、交回 `None`，不是就原样交回去。`now` 是认领那一刻——「上次体检」
    /// 画的就是它。**整条只读**，认领了不等于库变了（窗口因此先问这一句，`App::poll_tasks`）。
    pub fn settle(&mut self, done: Finished<Product>, now: i64) -> Option<Finished<Product>> {
        if self.running != Some(done.id) {
            return Some(done);
        }
        self.running = None;
        match done.ended {
            Ending::Done(Product::Checked { report, duplicates }) => {
                self.error = None;
                self.checked = Some(Checked {
                    report: *report,
                    duplicates: *duplicates,
                    at: now,
                });
            }
            // **不静默结束**：没成就说哪一步、为什么（那一档的词从收场渲染出）。被撤掉的那一趟什么都没留下，不另说。
            Ending::Failed { .. } => {
                self.error = Some(format!("{} {}", done.name, done.ended.render()));
            }
            _ => {}
        }
        None
    }

    /// 画正文。`scanned`：这个库有没有一个根完整扫过一趟；`identified`：识别那一道做完没有——两样判据都由库屏交进来
    /// （`LibraryRoot::fully_scanned`、`StageRow::settled`）。
    pub(crate) fn body_ui(&mut self, ui: &mut egui::Ui, scanned: bool, identified: bool) {
        if !scanned {
            centered_weak(ui, BEFORE_SCAN);
            return;
        }
        if let Some(error) = &self.error {
            let [上下, 左右] = Tokens::builtin().space.health_grid_padding;
            egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.colored_label(ui.visuals().error_fg_color, error);
                });
        }
        let Some(checked) = &self.checked else {
            centered_weak(
                ui,
                if self.running.is_some() {
                    "正在体检，完成后在这里列出报告。"
                } else {
                    "还没体检。按「重新体检」出一份报告。"
                },
            );
            return;
        };
        let 点了 = tiles_ui(ui, &checked.report, identified);
        match 点了 {
            Some(Tile::Finding(finding)) => {
                self.detail = Some(Detail {
                    finding,
                    expanded: None,
                });
                self.said = None;
                self.notice = None;
            }
            // 疑似同一作品：**不跳屏、不开明细**，只在八格底下说一句（拿主意的人 2026-09-15 答岔路口 9；跳到浏览屏
            // 「整理建议」由票 17 接上，挂单 `Q957`）。
            Some(Tile::SameWork) => {
                self.notice = Some(
                    if identified {
                        SAME_WORK_CLICKED_NOT_YET
                    } else {
                        SAME_WORK_CLICKED_BEFORE_IDENTIFY
                    }
                    .to_string(),
                );
            }
            None => {}
        }
        // 那一句画在八格**底下**：画在上头的话，它里头也写着「疑似同一作品」，再点一下会点到这一句上。
        if let Some(notice) = &self.notice {
            let [上下, 左右] = Tokens::builtin().space.health_grid_padding;
            egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(左右, 0.0)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.weak(notice);
                });
            ui.add_space(上下);
        }
    }

    /// 画开着的那一层**明细弹层**（设计稿 `DLG.health`）：标题「库体检 · 那一格」、那句说明、明细，页脚「关闭」。
    /// **每一帧都画**，库体检那一块收着时也画——弹层开没开着记在这一块上，不跟着面板收起。
    pub(crate) fn dialog_ui(&mut self, ctx: &egui::Context, site: &Site) {
        let (Some(detail), Some(checked)) = (self.detail, &self.checked) else {
            return;
        };
        let mut 展开 = detail.expanded;
        let mut 要打开: Option<PathBuf> = None;
        let said = self.said.clone();
        // 照稿：「导出清单…」幽灵按钮在左、「关闭」主按钮在右（拿主意的人 2026-09-15 答，挂单 `Q958`）。Esc 照旧等于「关闭」。
        let footer = Footer::new(Button::new("关闭", Pressed::Close))
            .dismiss_on_right()
            .button(
                Button::new(EXPORT, Pressed::Export)
                    .ghost()
                    .hover("把这一格的明细写成一份纯文本，保存到你选的地方；不许写进主库"),
            );
        let shown = Dialog::new(
            "库体检明细",
            format!("库体检 · {}", detail.finding.label()),
            footer,
        )
        .note(note_of(detail.finding))
        .width(Width::Wide)
        .show(ctx, |ui| {
            match &said {
                Some(Ok(said)) => {
                    ui.weak(said);
                }
                Some(Err(said)) => {
                    ui.colored_label(ui.visuals().error_fg_color, said);
                }
                None => {}
            }
            match detail.finding {
                Finding::Duplicates => {
                    duplicates_ui(ui, &checked.duplicates, &mut 展开, &mut 要打开);
                }
                other => findings_ui(ui, &checked.report, other, &mut 要打开),
            }
        });
        match shown.pressed {
            Some(Pressed::Close) => {
                self.detail = None;
                self.said = None;
            }
            Some(Pressed::Export) => {
                self.detail = Some(Detail {
                    expanded: 展开,
                    ..detail
                });
                let picked = crate::pick::save_file(
                    "导出清单",
                    &format!("库体检-{}.txt", detail.finding.label()),
                );
                self.export_picked(site, picked);
            }
            None => {
                self.detail = Some(Detail {
                    expanded: 展开,
                    ..detail
                });
            }
        }
        if let Some(folder) = 要打开 {
            self.said = reveal(&folder).err().map(Err);
        }
    }

    /// 「导出清单…」那个保存对话框交回来的那一个（`crate::pick::save_file`）：把开着的那一格的明细写成**纯文本**。
    ///
    /// - **重复拷贝**写的是 [`DuplicateDetails::render_text`]：每一组、组内每一份路径都在（体检那一趟不设上限，
    ///   [`Limits::FULL_DUPLICATE_PATHS_PER_GROUP`](romcat_core::scan::aggregate::Limits::FULL_DUPLICATE_PATHS_PER_GROUP)），
    ///   与 `romcat report --dump-duplicates` 同一份字节。
    /// - **其余几格**写的是核心库的文本明细（[`HealthReport::render_finding`]）。
    /// - **落点在主库的任何一个根里就拒**，一个字节都不写，那句话画在弹层里（ADR-0004，判据在核心库
    ///   [`Roots::refuse_writing_into`]，命令行问的是同一处）。
    /// - **交回 `None`**：取消了，或者对话框弹不出来（`crate::pick` 的模块文档），弹层里说一句没导出。
    ///
    /// **测试从这儿递路径进去**：系统的保存对话框测试点不了，选好之后那一半照 `tests/pick.rs` 的做法验。
    pub fn export_picked(&mut self, site: &Site, picked: Option<PathBuf>) {
        let (Some(detail), Some(checked)) = (self.detail, &self.checked) else {
            return;
        };
        let Some(path) = picked else {
            self.said = Some(Ok("没有选保存位置，清单没有导出。".to_string()));
            return;
        };
        let refused = Roots::load(&site.catalog)
            .map_err(|error| format!("中立库读不动，清单没有导出：{error}"))
            .and_then(|roots| roots.refuse_writing_into(&path));
        if let Err(why) = refused {
            self.said = Some(Err(why));
            return;
        }
        let text = match detail.finding {
            Finding::Duplicates => checked.duplicates.render_text(),
            other => checked.report.render_finding(other),
        };
        let shown = romcat_core::path::display(&path);
        self.said = Some(match std::fs::write(&path, text) {
            Ok(()) => Ok(format!("清单写到了 {shown}")),
            Err(error) => Err(format!("清单写不进 {shown}：{error}")),
        });
    }
}

/// **在文件系统中打开**这个目录：交给系统的文件管理器（`open` 那个依赖，协调人 2026-09-15 定岔路口 7）。
///
/// 这一层不测，理由同 `crate::pick`：系统的窗口测试驱动不了。测得到的那一半——盘不在位时按钮按不下去、按的是哪个
/// 目录——在画它的那一处。
fn reveal(folder: &Path) -> Result<(), String> {
    open::that_detached(folder)
        .map_err(|error| format!("打不开 {}：{error}", romcat_core::path::display(folder)))
}

/// 表里一格：给定的宽里画一行字，超宽截断（悬停看全），`靠` 是左靠还是右靠。
fn cell(ui: &mut egui::Ui, 宽: f32, 字: egui::WidgetText, 靠: egui::Align) {
    let 高 = ui.text_style_height(&egui::TextStyle::Body);
    ui.allocate_ui_with_layout(egui::vec2(宽, 高), egui::Layout::top_down(靠), |ui| {
        ui.set_min_width(宽);
        ui.set_max_width(宽);
        ui.add(egui::Label::new(字).truncate());
    });
}

/// 展开标（设计稿表格最后那一列的 `▸` / `▾`）：打包的字形子集里没有这两个字，画一个小三角（同库屏面板的折叠标），
/// 宽取令牌 `fold-mark`，弱色。
fn paint_chevron(ui: &egui::Ui, 格: egui::Rect, 摊开: bool) {
    let 半 = Tokens::builtin().layout.fold_mark / 2.0;
    let 中 = 格.center();
    let 三点 = if 摊开 {
        vec![
            中 + egui::vec2(-半, -半 / 2.0),
            中 + egui::vec2(半, -半 / 2.0),
            中 + egui::vec2(0.0, 半 / 2.0),
        ]
    } else {
        vec![
            中 + egui::vec2(-半 / 2.0, -半),
            中 + egui::vec2(-半 / 2.0, 半),
            中 + egui::vec2(半 / 2.0, 0.0),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        三点,
        ui.visuals().weak_text_color(),
        egui::Stroke::NONE,
    ));
}

/// 明细里一条路径（设计稿 `.lst` 那一行）：等宽小一号的路径，右边一颗弱化的「在文件系统中打开」——打开 `folder` 那个目录；
/// 那块盘不在位（目录不在）时按不下去。交回按了之后要打开的那个目录。
///
/// 给了中立库的**键**就照票 09 写「根名 · 相对路径」，放不下才省根名（`table::root_and_path`，拿主意的人 2026-09-15 答
/// 岔路口 4）；没有键就画那条路径，超宽截断。
fn path_row(
    ui: &mut egui::Ui,
    路径: &str,
    键: Option<&str>,
    folder: Option<&Path>,
) -> Option<PathBuf> {
    let tokens = Tokens::builtin();
    let 按钮宽 = look::small_button_width(ui, OPEN_IN_FILES);
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let mut 要打开 = None;
    ui.horizontal(|ui| {
        let 路径宽 = (ui.available_width() - 按钮宽 - ui.spacing().item_spacing.x).max(0.0);
        let 画的 = 键.map_or_else(
            || 路径.to_string(),
            |键| {
                table::root_and_path(
                    ui,
                    键,
                    &egui::FontId::new(字号, egui::FontFamily::Monospace),
                    路径宽,
                )
            },
        );
        cell(
            ui,
            路径宽,
            font::mono(画的).size(字号).into(),
            egui::Align::Min,
        );
        let 在位 = folder.is_some_and(Path::is_dir);
        let 按了 = look::small_buttons(ui, |ui| {
            ui.scope(|ui| {
                look::ghost_button(ui.visuals_mut());
                ui.add_enabled(在位, egui::Button::new(OPEN_IN_FILES))
                    .on_hover_text("交给系统的文件管理器，打开它所在的目录")
                    .on_disabled_hover_text(NOT_MOUNTED)
                    .clicked()
            })
            .inner
        });
        if 按了 {
            要打开 = folder.map(Path::to_path_buf);
        }
    });
    要打开
}

/// 其余几格的明细（设计稿 `DLG.health` 的列表那一支，`.lst`）：一行一条，路径在上、原因在下（弱字），牵涉的那几条并成一行跟在
/// 底下；有路径的一行右边一颗「在文件系统中打开」。行与原因出自核心库（[`HealthReport::finding_rows`]）；报告里样例截断时底下
/// 照实说另有几个。**未纳入管理的目录不画「映射到平台」下拉**（拿主意的人 2026-09-15 答岔路口 10）：只报告、不处理。
///
/// **虚拟化列表**（拿主意的人 2026-09-15 答，挂单 `Q959`）：「重新体检」那一趟列全之后一格能有几千行，列表最多高到令牌
/// `health-list-max-height`，在列表里滚、**只画看得见的那几行**。于是一格里每一行长得一样高——路径一行、有原因的格再加一行原因、
/// 有牵涉的格再加一行（顿号并列、超宽截断）。列表里只有按钮，没有输入框（`crate::dialog` 模块文档那条回收的禁令管的是输入框）。
fn findings_ui(
    ui: &mut egui::Ui,
    report: &HealthReport,
    finding: Finding,
    要打开: &mut Option<PathBuf>,
) {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.health_list_padding;
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let rows = report.finding_rows(finding);
    let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
    if rows.is_empty() {
        look::help(ui, &format!("没有{}。", finding.label()));
        return;
    }
    let 行缝 = look::step(0);
    let 正文高 = ui.text_style_height(&egui::TextStyle::Body);
    let 小字高 = ui.text_style_height(&egui::TextStyle::Small);
    // 有「在文件系统中打开」的那一行是横排，横排最矮也有可点控件那么高。
    let 路径高 = if rows.iter().any(|row| row.folder.is_some()) {
        ui.spacing().interact_size.y.max(正文高)
    } else {
        正文高
    };
    let 有原因 = rows.iter().any(|row| row.reason.is_some());
    let 有牵涉 = rows.iter().any(|row| !row.items.is_empty());
    let 行高 = 2.0 * 上下
        + 路径高
        + if 有原因 { 行缝 + 小字高 } else { 0.0 }
        + if 有牵涉 { 行缝 + 正文高 } else { 0.0 };
    egui::Frame::new()
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::ScrollArea::vertical()
                .id_salt(("库体检明细", finding.label()))
                .max_height(tokens.layout.health_list_max_height)
                .auto_shrink([false, true])
                .show_rows(ui, 行高, rows.len(), |ui, 看得见的| {
                    for 第几行 in 看得见的 {
                        let row = &rows[第几行];
                        let (格, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 行高),
                            egui::Sense::hover(),
                        );
                        if 第几行 > 0 {
                            ui.painter().hline(格.x_range(), 格.top(), 线);
                        }
                        let mut 里头 = ui.new_child(
                            egui::UiBuilder::new()
                                .max_rect(格.shrink2(egui::vec2(左右, 上下)))
                                .layout(egui::Layout::top_down(egui::Align::Min)),
                        );
                        里头.spacing_mut().item_spacing.y = 行缝;
                        let 宽 = 里头.available_width();
                        match &row.folder {
                            Some(folder) => {
                                if let Some(目录) =
                                    path_row(&mut 里头, &row.path, None, Some(folder))
                                {
                                    *要打开 = Some(目录);
                                }
                            }
                            None => cell(
                                &mut 里头,
                                宽,
                                font::mono(row.path.as_str()).size(字号).into(),
                                egui::Align::Min,
                            ),
                        }
                        if let Some(reason) = &row.reason {
                            look::help(&mut 里头, reason);
                        }
                        if !row.items.is_empty() {
                            cell(
                                &mut 里头,
                                宽,
                                font::mono(row.items.join("、")).size(字号).weak().into(),
                                egui::Align::Min,
                            );
                        }
                    }
                });
        });
    let 列出 = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    let 一共 = report.finding_count(finding);
    if 一共 > 列出 {
        ui.add_space(look::step(1));
        look::help(
            ui,
            &format!("另有 {} {}", thousands(一共 - 列出), finding.unit()),
        );
    }
}

/// 重复拷贝明细（设计稿 `DLG.health` 的 `dups` 那一支）：一张表，四列照稿——内容、平台、份数、单份大小——加一列展开标，
/// **按可腾出的空间从大到小**（核心库排好的次序，[`DuplicateDetails`]）。点一行摊开那一组记下的每一份路径，每一份旁边一颗
/// 「在文件系统中打开」。平台横跨几个时用顿号并列（挂单 `Q953`）。
fn duplicates_ui(
    ui: &mut egui::Ui,
    details: &DuplicateDetails,
    展开: &mut Option<usize>,
    要打开: &mut Option<PathBuf>,
) {
    let tokens = Tokens::builtin();
    let [平台宽, 份数宽, 大小宽, 标宽] = tokens.layout.health_dup_columns;
    let [头上下, 头左右] = tokens.space.table_head_padding;
    let [行上下, 行左右] = tokens.space.cell_padding;
    let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
    let 弱色 = ui.visuals().weak_text_color();
    let 表头字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 格缝 = 2.0 * 行左右;
    let 内容宽 = |ui: &egui::Ui| {
        (ui.available_width() - 平台宽 - 份数宽 - 大小宽 - 标宽 - 4.0 * 格缝).max(0.0)
    };
    egui::Frame::new()
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing = egui::vec2(格缝, 0.0);
            // **行高照稿**（设计稿 `.tbl td` 一行约 36 点）：egui 的横排最矮也有可点控件那么高，这张表里局部把它收成零，
            // 行高只由字高与格子内边距撑；全窗口那一格不动（拿主意的人 2026-09-15 定）。
            ui.spacing_mut().interact_size.y = 0.0;
            // 表头（设计稿 `.tbl th`）。
            egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(头左右, 头上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let 名宽 = 内容宽(ui);
                    let 头 = |字: &str| {
                        egui::WidgetText::from(font::strong(字).size(表头字号).color(弱色))
                    };
                    ui.horizontal(|ui| {
                        cell(ui, 名宽, 头("内容"), egui::Align::Min);
                        cell(ui, 平台宽, 头("平台"), egui::Align::Min);
                        cell(ui, 份数宽, 头("份数"), egui::Align::Max);
                        cell(ui, 大小宽, 头("单份大小"), egui::Align::Max);
                        cell(ui, 标宽, egui::WidgetText::default(), egui::Align::Min);
                    });
                });
            for (第几组, 组) in details.groups.iter().take(DUPLICATE_ROWS).enumerate() {
                look::divider(ui);
                let 摊开 = *展开 == Some(第几组);
                let 这一行 = egui::Frame::new()
                    .inner_margin(egui::Margin::from(egui::vec2(行左右, 行上下)))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let 名宽 = 内容宽(ui);
                        ui.horizontal(|ui| {
                            cell(
                                ui,
                                名宽,
                                font::strong(组.name.as_str()).into(),
                                egui::Align::Min,
                            );
                            cell(ui, 平台宽, 组.platforms.join("、").into(), egui::Align::Min);
                            cell(
                                ui,
                                份数宽,
                                egui::RichText::new(thousands(组.count)).into(),
                                egui::Align::Max,
                            );
                            cell(
                                ui,
                                大小宽,
                                egui::RichText::new(human_bytes(组.size)).into(),
                                egui::Align::Max,
                            );
                            let 高 = ui.text_style_height(&egui::TextStyle::Body);
                            let (标, _) =
                                ui.allocate_exact_size(egui::vec2(标宽, 高), egui::Sense::hover());
                            paint_chevron(ui, 标, 摊开);
                        });
                    })
                    .response
                    .interact(egui::Sense::click());
                if 这一行.clicked() {
                    *展开 = if 摊开 { None } else { Some(第几组) };
                }
                if 摊开 {
                    look::divider(ui);
                    egui::Frame::new()
                        .fill(ui.visuals().faint_bg_color)
                        .inner_margin(egui::Margin::from(egui::vec2(行左右, 行上下)))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.spacing_mut().item_spacing.y = look::step(1);
                            for (第几份, 路径) in 组.paths.iter().enumerate() {
                                let 键 = 组.keys.get(第几份).map(String::as_str);
                                if let Some(目录) = path_row(ui, 路径, 键, Path::new(路径).parent())
                                {
                                    *要打开 = Some(目录);
                                }
                            }
                            let 少了 = 组.paths_missing();
                            if 少了 > 0 {
                                look::help(ui, &format!("另有 {} 份没记下路径", thousands(少了)));
                            }
                        });
                }
            }
        });
    ui.add_space(look::step(1));
    let 其余 = details.groups.len().saturating_sub(DUPLICATE_ROWS);
    let 那一句 = if 其余 > 0 {
        format!(
            "另有 {} 组 · 按可腾出的空间从大到小排列",
            thousands(u64::try_from(其余).unwrap_or(u64::MAX))
        )
    } else {
        "按可腾出的空间从大到小排列".to_string()
    };
    look::help(ui, &那一句);
}

/// 体检那一趟：从中立库折出全库的统计，出报告，连同重复拷贝的完整明细（**每组记全路径**，明细弹层要列得出全部）。
/// 一个字节都不读主库、不写库。
fn check_run(catalog: &Catalog, task: &Handle) -> Result<Product, Cutoff> {
    task.check()?;
    // **明细列全**（拿主意的人 2026-09-15 答，挂单 `Q959`）：这一趟样例不设上限、重复拷贝每组记全路径，其余几格的明细
    // 与导出的清单一个不少。命令行 `romcat report` 照旧每类只留前几个。
    let limits = Limits {
        max_examples: usize::MAX,
        max_duplicate_paths_per_group: Limits::FULL_DUPLICATE_PATHS_PER_GROUP,
        ..Limits::default()
    };
    let failed = |error: CatalogError| Cutoff::failed(format!("中立库读不出来：{error}"));
    let aggregate = catalog
        .aggregate(&limits, &Manifest::builtin())
        .map_err(failed)?;
    task.check()?;
    let report = HealthReport::build_full(&aggregate, &catalog.report_meta().map_err(failed)?);
    let duplicates = DuplicateDetails::build(&aggregate, &report);
    Ok(Product::Checked {
        report: Box::new(report),
        duplicates: Box::new(duplicates),
    })
}

/// 这一块正文里居中的一句弱字（设计稿 `.empty`），四边留令牌 `health-empty-padding`。
fn centered_weak(ui: &mut egui::Ui, text: &str) {
    let 留白 = Tokens::builtin().space.health_empty_padding;
    egui::Frame::new()
        .inner_margin(egui::Margin::from(egui::vec2(留白, 留白)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| ui.label(egui::RichText::new(text).weak()));
        });
}

/// 概要里的一格。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tile {
    /// 报告里说得出明细的那几格（[`Finding`]）。
    Finding(Finding),
    /// 疑似同一作品：判断在票 17，这之前不画数。
    SameWork,
}

impl Tile {
    /// 八格，照设计稿的次序（`HEALTH`）。
    const ALL: [Self; 8] = [
        Self::Finding(Finding::Duplicates),
        Self::Finding(Finding::PlatformConflicts),
        Self::Finding(Finding::ShapingDoubts),
        Self::Finding(Finding::Unreadable),
        Self::Finding(Finding::UnmappedDirs),
        Self::Finding(Finding::StrandedCompanions),
        Self::Finding(Finding::NonGameAssets),
        Self::SameWork,
    ];
}

/// 一格画出来的样子。
struct Face {
    /// 标题。
    label: &'static str,
    /// 那个数（带单位）。
    value: String,
    /// 底下那句小字。
    sub: String,
    /// 左边那条色条的语气；`None` 不画。
    tone: Option<Tone>,
}

/// 这一格画成什么样。数与单位、标题出自核心库（[`HealthReport::finding_count`]、[`Finding`]）；小字照设计稿，
/// 与词表对不上的两句改成实话（不可读说的是元数据；未纳入管理的目录照扫照报，只是不进识别与刮削——岔路口 10）。
fn face(tile: Tile, report: &HealthReport, identified: bool) -> Face {
    let Tile::Finding(finding) = tile else {
        return Face {
            label: SAME_WORK,
            value: "—".to_string(),
            sub: if identified {
                SAME_WORK_NOT_YET
            } else {
                SAME_WORK_BEFORE_IDENTIFY
            }
            .to_string(),
            tone: None,
        };
    };
    let count = report.finding_count(finding);
    let sub = match finding {
        Finding::Duplicates => format!(
            "可腾出 {}",
            human_bytes(report.suspects.duplicate_reclaimable_bytes)
        ),
        Finding::PlatformConflicts => "目录只是强先验，内容可以推翻它".to_string(),
        Finding::ShapingDoubts => "多碟没合在一起、目录拆错".to_string(),
        Finding::Unreadable => "文件名拿得到，元数据读不到".to_string(),
        Finding::UnmappedDirs => "不在任何平台目录下，不进识别与刮削".to_string(),
        Finding::StrandedCompanions => "存档、补丁找不到对应的主文件".to_string(),
        Finding::NonGameAssets => "BIOS 等，入库但不导出".to_string(),
    };
    // 色条照稿配色（`.htile.warn` / `.bad`），**数为零的格不画**（拿主意的人 2026-09-15 答岔路口 11）。
    let tone = match finding {
        Finding::Duplicates | Finding::PlatformConflicts | Finding::ShapingDoubts => {
            Some(Tone::Caution)
        }
        Finding::Unreadable => Some(Tone::Bad),
        Finding::UnmappedDirs | Finding::StrandedCompanions | Finding::NonGameAssets => None,
    }
    .filter(|_| count > 0);
    Face {
        label: finding.label(),
        value: format!("{} {}", thousands(count), finding.unit()),
        sub,
        tone,
    }
}

/// 八格（设计稿 `.health`）：四列等宽，格与格之间、外面一圈取令牌。交回这一帧点了哪一格。
fn tiles_ui(ui: &mut egui::Ui, report: &HealthReport, identified: bool) -> Option<Tile> {
    let tokens = Tokens::builtin();
    let [外上下, 外左右] = tokens.space.health_grid_padding;
    let 缝 = tokens.space.health_grid_gap;
    egui::Frame::new()
        .inner_margin(egui::Margin::from(egui::vec2(外左右, 外上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let 宽 = ((ui.available_width() - 3.0 * 缝) / 4.0).max(0.0);
            let mut 点了 = None;
            for (第几行, 这一行) in Tile::ALL.chunks(4).enumerate() {
                if 第几行 > 0 {
                    ui.add_space(缝);
                }
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 缝;
                    for tile in 这一行 {
                        if tile_ui(ui, 宽, &face(*tile, report, identified)).clicked() {
                            点了 = Some(*tile);
                        }
                    }
                });
            }
            点了
        })
        .inner
}

/// 一格（设计稿 `.htile`）：面板底、分隔线色描边、中圆角，内边距取令牌；标题与小字是半号说明字号的弱字，数是等宽大一号。
/// 指针停上去描边换强调色；有语气时左边一条色条（宽取令牌 `row-stripe`）。**三段各画一行、超宽截断**：一行格子一样高。
fn tile_ui(ui: &mut egui::Ui, 宽: f32, face: &Face) -> egui::Response {
    let tokens = Tokens::builtin();
    let [上下, 左右] = tokens.space.health_tile_padding;
    let 圆角 = tokens.radius.medium;
    let 小字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 弱色 = ui.visuals().weak_text_color();
    let 强色 = ui.visuals().strong_text_color();
    let 画好 = egui::Frame::new()
        .fill(ui.visuals().window_fill)
        .corner_radius(圆角)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            let 内宽 = (宽 - 2.0 * 左右).max(0.0);
            ui.set_min_width(内宽);
            ui.set_max_width(内宽);
            ui.spacing_mut().item_spacing.y = tokens.space.health_tile_gap;
            ui.vertical(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(face.label).size(小字号).color(弱色))
                        .truncate(),
                );
                ui.add(
                    egui::Label::new(
                        font::mono(face.value.as_str())
                            .size(tokens.font.size_health_value)
                            .color(强色),
                    )
                    .truncate(),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(face.sub.as_str())
                            .size(小字号)
                            .color(弱色),
                    )
                    .truncate(),
                );
            });
        });
    let response = 画好.response.interact(egui::Sense::click());
    let rect = response.rect;
    let visuals = ui.visuals();
    let 线 = visuals.widgets.noninteractive.bg_stroke;
    let 描边色 = if response.hovered() {
        visuals.selection.stroke.color
    } else {
        线.color
    };
    ui.painter().rect_stroke(
        rect,
        圆角,
        egui::Stroke::new(线.width, 描边色),
        egui::StrokeKind::Inside,
    );
    if let Some(tone) = face.tone {
        let 条 = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(rect.min.x + tokens.layout.row_stripe, rect.max.y),
        );
        ui.painter().rect_filled(
            条,
            egui::CornerRadius {
                nw: 圆角,
                sw: 圆角,
                ne: 0,
                se: 0,
            },
            look::tone_colors(tone, visuals).0,
        );
    }
    response
}
