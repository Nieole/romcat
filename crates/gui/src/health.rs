//! 库屏底下的**库体检**那一块（票 `gui-looks-like-the-design/27`）：把从**中立库**折出来的那份体检报告
//! 摆成一块概要，每一格点进去看明细。体检那一趟**一个字节都不读主库**——盘早在扫描那一趟走过了，
//! 体检只折中立库（`Section::check` 排的那一趟跑的是 `check_run`）。
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
use romcat_core::report::{
    CorrectionGroup, CorrectionGroups, DuplicateDetails, Finding, HealthReport, human_bytes,
    thousands,
};
use romcat_core::scan::ScanOutcome;
use romcat_core::scan::aggregate::Limits;
use romcat_core::site::Site;
use romcat_core::task::{Cutoff, Ending, Finished, Handle};
use romcat_core::verdict::PlatformDecision;

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

/// **平台纠正**那一层的标题（设计稿 `DLG.platfix` 原话，票 `gui-looks-like-the-design/28`）。
pub const PLATFIX: &str = "平台纠正";

/// 平台纠正那一层页脚上那颗按钮（设计稿原话）。
pub const PLATFIX_DONE: &str = "完成";

/// 平台纠正那一层末尾那一句。
///
/// **前半句照设计稿**；后半句是这一票**照实补的**：稿上那句副标题写着「导出和同步时按纠正后的
/// 平台放置」，可导出目录名取的是**键里那一级目录**（`path::platform_of_key`），同步读的是
/// 中立库里**目录声明的那一列**——两处眼下都不读纠正。**屏上不许许一句做不到的话**
/// （票 28 收尾审查 Spec 轴第 1 条），所以这里如实说，那件事记在挂单 `Q1033` 上。
pub const PLATFIX_FOOTNOTE: &str =
    "改了平台的变体会在下次识别时按新平台重新匹配；导出与同步眼下仍按目录放置。";

/// 平台纠正那一层里定过之后那颗「撤销」（设计稿原话）。
pub const PLATFIX_UNDO: &str = "撤销";

/// 一条都不剩时那一层里说的这一句。
pub const PLATFIX_NONE: &str = "目录与内容都对得上，没有要纠正的。";

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
    /// **平台纠正**那一层开着没有（票 `gui-looks-like-the-design/28`）。
    platfix: bool,
    /// 报告里那几组加上人定过的决定，**核心库合出来的那一份**（[`CorrectionGroups`]）；
    /// 还没合过是 `None`。界面一个数都不自己算（ADR-0005、ADR-0024）。
    ///
    /// **缓着而不是每帧现读**：它要读沉淀库，而这一层画在画帧那条线程上。报告换了、
    /// 或者人刚定过一条，就扔掉重合（[`Section::forget_corrections`]）。
    corrections: Option<CorrectionGroups>,
}

/// 明细弹层里「在文件系统中打开」那颗按钮上的字（设计稿原话）。
pub const OPEN_IN_FILES: &str = "在文件系统中打开";

/// 那块盘不在位时那颗按钮按不下去，悬停说的这一句（词表：盘没插上写「未连接」）。
const NOT_MOUNTED: &str = "未连接：那块盘眼下不在位，打不开";

/// 重复拷贝明细那张表最多摆几组；其余的在重复拷贝明细全文里（导出去的那一份列全部）。
const DUPLICATE_ROWS: usize = 50;

/// **平台纠正**里一组摆几条样例（设计稿 `DLG.platfix` 的 `g.ex` 两条，底下写「另有 N 条」）。
const PLATFIX_EXAMPLES: usize = 2;

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

/// 明细弹层标题底下那句说明：**核心库的判据**（[`Finding::criterion`]，与导出的清单抬头同一句）
/// 接上这一层自己的那句政策话。
///
/// 判据不在这儿另写一套（ADR-0024；拿主意的人 2026-09-20 定「判据文案统一到核心库一处」）——
/// 原先界面手写了七句，与核心库那七句已经开始漂：弹层说「判据不读内容」，导出的清单说
/// 「不读内容，也不算哈希」（票 27 收尾审查 Standards 轴第 1 条）。设计稿 `HD` 那几句里
/// 判据那一半由核心库出，剩下的「只发现并报告」「这里不改动任何文件」这类话是界面自己的。
fn note_of(finding: Finding) -> String {
    let 政策 = match finding {
        Finding::Duplicates => {
            "只发现并报告，绝不自动删除；主库只读，要清理请在文件系统中手动处理。同一个作品的不同转储是不同的变体，不算重复拷贝。"
        }
        Finding::PlatformConflicts | Finding::ShapingDoubts => "这里只报告，不改动任何文件。",
        Finding::Unreadable => "它们如实记为「不可读」，既不算已变，也不算已删。",
        Finding::UnmappedDirs => "列在这里，作为整理的依据。",
        Finding::StrandedCompanions => "它们没有归入任何变体；这里只报告，不改动任何文件。",
        Finding::NonGameAssets => "在「浏览」中打开「显示非游戏资产」可以查看。",
    };
    format!("{}。{政策}", finding.criterion())
}

impl Section {
    /// 扫完一个根：拿扫描交回的那份报告，连同一份统计折出重复拷贝的完整明细。`at` 是认领那一刻。
    pub fn take_scan(&mut self, outcome: &ScanOutcome, at: i64) {
        self.error = None;
        self.forget_corrections();
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

    /// 把缓着的那份**平台纠正**扔掉，下一帧重合：报告换了、或者人刚定过一条。
    fn forget_corrections(&mut self) {
        self.corrections = None;
    }

    /// 合一份**平台纠正**：报告里那几组 + 沉淀库里人定过的决定，合的是核心库
    /// （[`CorrectionGroups::build`]）。读沉淀库没成就当一条都没定过——那时屏上会把
    /// 处理过的组重新问一遍，而**不会**把人定过的东西说成没定过。
    pub(crate) fn ensure_corrections(&mut self, site: &Site) {
        if self.corrections.is_some() {
            return;
        }
        let Some(checked) = &self.checked else {
            return;
        };
        let decided = site
            .store
            .platform_corrections(&site.library_identity)
            .unwrap_or_default();
        self.corrections = Some(CorrectionGroups::build(
            &checked.report,
            &Manifest::builtin(),
            &decided,
        ));
    }

    /// 眼下那一份**平台纠正**（测试拿它核对）。
    #[must_use]
    pub fn corrections(&self) -> Option<&CorrectionGroups> {
        self.corrections.as_ref()
    }

    /// **平台纠正**那一层开着没有（测试拿它核对）。
    #[must_use]
    pub fn platfix_open(&self) -> bool {
        self.platfix
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
                self.forget_corrections();
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
        let 点了 = tiles_ui(ui, &checked.report, self.corrections.as_ref(), identified);
        match 点了 {
            // **目录与内容平台不符**那一格点进去不是明细，是**平台纠正**那一层（设计稿
            // `HEALTH` 里这一格的 `data-dg="open:platfix"`，票 `gui-looks-like-the-design/28`）：
            // 那一层既列得出明细，又按得下决定。
            Some(Tile::Finding(Finding::PlatformConflicts)) => {
                self.platfix = true;
                self.said = None;
                self.notice = None;
            }
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

    /// 画开着的那一层**平台纠正**（设计稿 `DLG.platfix`，票 `gui-looks-like-the-design/28`）：
    /// 按组列出「从哪个平台 → 到哪个平台、多少条、凭什么、两条样例」，每组两条出路——
    /// **按内容改**与**保持目录的说法**，定过的那一组摊在那儿说「已改为 X」/「已保持 X」，
    /// 旁边一颗「撤销」。
    ///
    /// **一个数都不在这儿算**：组、条数、判据、「改不改都行」那一句全由核心库交出来
    /// （[`CorrectionGroups`]，ADR-0005、ADR-0024）。**盘上一个字节都不动**（ADR-0004）：
    /// 按下去只往**沉淀库**写一条决定，文件不移动、不改名。
    pub(crate) fn platfix_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        if !self.platfix {
            return;
        }
        self.ensure_corrections(site);
        let Some(fixes) = &self.corrections else {
            return;
        };
        // 副标题：**判据那一句出自核心库**（`Finding::criterion`，与导出的清单抬头同一句），
        // 后面接的是这一层自己的政策话（同明细弹层 `note_of` 那条先例）。
        let note = format!(
            "{}。共 {} 条目录与内容不符，按组处理；纠正记为裁决，不移动任何文件。",
            Finding::PlatformConflicts.criterion(),
            thousands(fixes.remaining()),
        );
        let mut 按了: Option<(String, String, Option<PlatformDecision>)> = None;
        let said = self.said.clone();
        let footer = Footer::new(Button::new(PLATFIX_DONE, Pressed::Close)).dismiss_on_right();
        let shown = Dialog::new(PLATFIX, PLATFIX.to_string(), footer)
            .note(note)
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
                if fixes.is_empty() {
                    look::help(ui, PLATFIX_NONE);
                    return;
                }
                for group in fixes.groups() {
                    if let Some(按下的) = platfix_group_ui(ui, group) {
                        按了 = Some((
                            group.group.declared.clone(),
                            group.group.implied.clone(),
                            按下的,
                        ));
                    }
                    ui.add_space(look::step(2));
                }
                look::help(ui, PLATFIX_FOOTNOTE);
            });
        if shown.pressed == Some(Pressed::Close) {
            self.platfix = false;
            self.said = None;
        }
        if let Some((declared, implied, 决定)) = 按了 {
            self.said = Some(self.decide(site, &declared, &implied, 决定));
            self.forget_corrections();
        }
    }

    /// 往**沉淀库**落一条平台纠正，或者撤掉说了算的那一条。交回画在那一层里的那句回话。
    ///
    /// **只写沉淀库**：盘上的文件一个字节都不动（ADR-0004），中立库也不动——
    /// 下一趟识别读这一条重新匹配（`identify::platform_of`）。
    fn decide(
        &self,
        site: &mut Site,
        declared: &str,
        implied: &str,
        决定: Option<PlatformDecision>,
    ) -> Result<String, String> {
        let library = site.library_identity.clone();
        let 条数 = self
            .corrections
            .as_ref()
            .and_then(|那一层| {
                那一层
                    .groups()
                    .iter()
                    .find(|one| one.group.declared == declared && one.group.implied == implied)
            })
            .map_or(0, |one| one.group.count);
        match 决定 {
            Some(PlatformDecision::ByContent) => site
                .store
                .set_platform_correction(&library, declared, implied, PlatformDecision::ByContent)
                .map(|()| {
                    format!(
                        "已把 {} 条改为 {implied}（记为裁决）；下次识别按新平台重新匹配，盘上的文件一个字节都没动",
                        thousands(条数)
                    )
                }),
            Some(PlatformDecision::KeepDeclared) => site
                .store
                .set_platform_correction(&library, declared, implied, PlatformDecision::KeepDeclared)
                .map(|()| {
                    format!(
                        "已确认保持 {declared}，这 {} 条不再提示",
                        thousands(条数)
                    )
                }),
            // **交回值不许丢**：本来就没有说了算的那一条时说「已撤销」是骗人的
            // （票 28 收尾审查 Standards 轴第 7 条）。
            None => site
                .store
                .undo_platform_correction(&library, declared, implied)
                .map(|撤掉了| {
                    if 撤掉了 {
                        "已撤销平台纠正，这一组恢复为目录给出的平台".to_string()
                    } else {
                        "这一组上没有说了算的纠正，什么都没撤".to_string()
                    }
                }),
        }
        .map_err(|why| format!("这一下没记进沉淀库：{why}"))
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
    /// - **重复拷贝**写的是 [`DuplicateDetails::render_text`]。⚠️ **「与 `romcat report --dump-duplicates` 同一份字节」
    ///   只在点过「重新体检」之后成立**：那一趟每组路径不设上限
    ///   （[`Limits::FULL_DUPLICATE_PATHS_PER_GROUP`](romcat_core::scan::aggregate::Limits::FULL_DUPLICATE_PATHS_PER_GROUP)，
    ///   见 `check_run`），每一组、组内每一份路径都在。而**扫完一个根交回的那一份**用的是扫描的默认上限
    ///   （每组 10 条），一组超过 10 份时导出的清单照实写「另有 N 份没记下路径」，字节与命令行不同
    ///   （拿主意的人 2026-09-20 定：扫描那一趟不放开，挂单 `Q959` 记着这条边界）。
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

/// **平台纠正**里的一组（设计稿 `.pgrp`）：头一行是两枚平台标、这一组叫什么、多少条，定过的那一组
/// 右边摆一枚标与一颗「撤销」；正文是判据那一句、两条样例、「另有 N 条」，还没定过的底下摆两颗按钮。
///
/// 交回**按下去要做什么**：`Some(Some(档))` 是定成那一档，`Some(None)` 是撤销，`None` 是这一帧什么都没按。
fn platfix_group_ui(
    ui: &mut egui::Ui,
    group: &CorrectionGroup,
) -> Option<Option<PlatformDecision>> {
    let tokens = Tokens::builtin();
    let [头上下, 头左右] = tokens.space.table_head_padding;
    let [正上下, 正左右] = tokens.space.cell_padding;
    let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
    let mut 按了 = None;
    egui::Frame::new()
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // ── 头一行（设计稿 `.pgrp .gh`）：凹一格的底，底下一条线。
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .inner_margin(egui::Margin::from(egui::vec2(头左右, 头上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        platform_badge(ui, &group.group.declared);
                        ui.label("→");
                        platform_badge(ui, &group.group.implied);
                        ui.label(font::strong(group.headline()));
                        look::help(ui, &format!("{} 条", thousands(group.group.count)));
                        if let Some(settled) = group.settled() {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let 撤销 = look::small_buttons(ui, |ui| {
                                        ui.scope(|ui| {
                                            look::ghost_button(ui.visuals_mut());
                                            ui.button(PLATFIX_UNDO)
                                                .on_hover_text(
                                                    "撤掉这一组的决定，它回到还没处理，库体检那一格重新数它",
                                                )
                                                .clicked()
                                        })
                                        .inner
                                    });
                                    if 撤销 {
                                        按了 = Some(None);
                                    }
                                    look::chip(ui, Tone::Good, &settled);
                                },
                            );
                        }
                    });
                });
            look::divider(ui);
            // ── 正文（设计稿 `.pgrp .gb`）：判据、样例、两颗按钮。
            egui::Frame::new()
                .inner_margin(egui::Margin::from(egui::vec2(正左右, 正上下)))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing.y = look::step(1);
                    look::help(ui, &group.reason());
                    // 样例照票 09 写「根名 · 相对路径」（`table::root_and_path`，同明细弹层那一行）：
                    // 核心库交的是**中立库的键**，盘在哪儿是这一层拼的事，基线图里也就不会有
                    // 某一台机器上那条临时目录。
                    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
                    let 字体 = egui::FontId::new(字号, egui::FontFamily::Monospace);
                    for 样例 in group.group.examples.iter().take(PLATFIX_EXAMPLES) {
                        let 宽 = ui.available_width();
                        let 画的 = table::root_and_path(ui, 样例, &字体, 宽);
                        ui.label(font::mono(画的).size(字号));
                    }
                    let 少了 = group
                        .group
                        .count
                        .saturating_sub(u64::try_from(PLATFIX_EXAMPLES).unwrap_or(0));
                    if 少了 > 0 {
                        look::help(ui, &format!("另有 {} 条", thousands(少了)));
                    }
                    if group.pending() {
                        ui.horizontal_wrapped(|ui| {
                            let 改 = look::small_buttons(ui, |ui| {
                                ui.scope(|ui| {
                                    look::primary_button(ui.visuals_mut());
                                    ui.button(format!(
                                        "按内容改为 {}（{} 条）",
                                        group.group.implied,
                                        thousands(group.group.count)
                                    ))
                                    .on_hover_text(
                                        "只往沉淀库记一条决定：盘上的文件不移动、不改名，下次识别按新平台重新匹配",
                                    )
                                    .clicked()
                                })
                                .inner
                            });
                            if 改 {
                                按了 = Some(Some(PlatformDecision::ByContent));
                            }
                            let 保持 = look::small_buttons(ui, |ui| {
                                ui.button(format!("保持 {}", group.group.declared))
                                    .on_hover_text("这一组按目录说的算，下一趟体检不再问它")
                                    .clicked()
                            });
                            if 保持 {
                                按了 = Some(Some(PlatformDecision::KeepDeclared));
                            }
                            if let Some(那一句) = group.interchangeable_note() {
                                look::help(ui, &那一句);
                            }
                        });
                    }
                });
        });
    按了
}

/// 一枚**平台标**（设计稿 `.hplat`）：平台色的底、白字、小圆角。高与左右留白跟标签同一对令牌
/// （`chip-height` / `chip-padding`，与稿上那两个数逐字相同），字取 `size-caption-plus` 的粗体。
fn platform_badge(ui: &mut egui::Ui, platform: &str) {
    let tokens = Tokens::builtin();
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let color = tokens.color.platform.of(platform);
    let galley = ui.painter().layout_no_wrap(
        platform.to_owned(),
        egui::FontId::new(字号, font::strong_family()),
        egui::Color32::WHITE,
    );
    let 高 = tokens.layout.chip_height.max(galley.size().y);
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(galley.size().x + 2.0 * tokens.layout.chip_padding, 高),
        egui::Sense::hover(),
    );
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, tokens.radius.small, color);
    painter.galley(
        rect.center() - galley.size() / 2.0,
        galley,
        egui::Color32::WHITE,
    );
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
fn face(
    tile: Tile,
    report: &HealthReport,
    fixes: Option<&CorrectionGroups>,
    identified: bool,
) -> Face {
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
    // **目录与内容平台不符**那一格数的是**还没处理的那几组**（票 `gui-looks-like-the-design/28`）：
    // 处理过的组不再计数，小字改说「已处理 N 组」。两个数都由核心库交出来
    // （`CorrectionGroups::remaining` / `handled_note`），界面一个都不自己算。
    let 纠正 = (finding == Finding::PlatformConflicts)
        .then_some(fixes)
        .flatten();
    let count = 纠正.map_or_else(
        || report.finding_count(finding),
        CorrectionGroups::remaining,
    );
    // 小字出自核心库那一处（`Finding::hint`，判据的短写法）：界面不另写一套（ADR-0024；拿主意的人
    // 2026-09-20 定「判据文案统一到核心库一处，格子小字也从同一处出」）。重复拷贝那一格稿上写的是
    // 「可腾出 N」——那是报告里的一个数，所以 `hint()` 对它交回 `None`。
    let sub = 纠正
        .and_then(CorrectionGroups::handled_note)
        .unwrap_or_else(|| {
            finding.hint().map_or_else(
                || {
                    format!(
                        "可腾出 {}",
                        human_bytes(report.suspects.duplicate_reclaimable_bytes)
                    )
                },
                ToString::to_string,
            )
        });
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
fn tiles_ui(
    ui: &mut egui::Ui,
    report: &HealthReport,
    fixes: Option<&CorrectionGroups>,
    identified: bool,
) -> Option<Tile> {
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
                        if tile_ui(ui, 宽, &face(*tile, report, fixes, identified)).clicked() {
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
