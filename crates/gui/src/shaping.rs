//! **成型纠正**那一层（票 `gui-looks-like-the-design/29`，设计稿 `DLG.shape`）：维护者纠正**成型规则**
//! 出错的地方——几张碟没合在一起，或者一个目录被当成了一个变体。
//!
//! **人工纠正聚合结果是一等公民功能**（词表**成型规则**）：不是补丁，是这套模型承认规则会出错之后留的
//! 正门。两处进得去——库体检「成型存疑」那一格的明细里一行一颗「处理…」，作品详情**变体**那一面一张卡
//! 一颗「调整成型…」；撤销也在那张卡上。
//!
//! ## 这一层不判任何事（ADR-0024）
//!
//! 「哪一处存疑」由核心库判（`shape::shaping_doubts`，票 27 立的那一处）；「这一下落成哪几行人工纠正」
//! 「合成之后主文件是谁、附属文件是哪几条」「撤销要清掉哪几条」由核心库答（[`fix`]）。这一层只做三件事：
//! 把那份预览画出来、把人勾中的那几条转发过去、把落库与重新成型排上任务台。
//!
//! ## 盘上一个字节都不动（ADR-0004）
//!
//! 按下去只往**沉淀库**写几行，再把**中立库**里的变体重算一遍。主库里的文件不移动、不改名、不生成
//! 播放列表——屏上那句话因此也是这么写的。
//!
//! ## 记为人工纠正，不是记为裁决
//!
//! 设计稿那句「纠正记为裁决」不照抄（同票 28 的挂单 `Q1032`）：落下来的是沉淀库里 `shaping_override`
//! 那张表，撤销**不走裁决记录**，说成裁决是对用户说了另一样东西的名字。

use std::collections::BTreeMap;
use std::path::Path;

use romcat_core::catalog::{Catalog, CatalogError};
use romcat_core::platform::Manifest;
use romcat_core::report::{DuplicateDetails, HealthReport, human_bytes, thousands};
use romcat_core::scan::aggregate::{Limits, ShapingDoubt};
use romcat_core::shape::{self, DoubtKind, fix};
use romcat_core::site::Site;
use romcat_core::task::{Cutoff, Handle};

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::font;
use crate::look;
use crate::table;
use crate::task::{Product, Tasks};
use crate::tokens::Tokens;

/// 这一层的标题头一截（设计稿 `DLG.shape` 的 `title` 原话）。
pub const FIX: &str = "调整成型";

/// 标题底下那句说明。
///
/// **前两句照设计稿**；第三句是这一票**照实改的**：稿上写的是「纠正记为裁决，按文件内容永久保留」，
/// 而落下来的那张表（沉淀库的 `shaping_override`）键是**主库标识加中立库的键**，也就是**路径**——
/// 删掉中立库重扫它还在（票 `one-criterion-per-thing/07` 钉着），但文件改了名、挪了目录就对不上了。
/// **屏上不许许一句做不到的话**（同票 28 收尾审查 Spec 轴第 1 条）；锚该不该换成内容锚记在挂单
/// `Q1041`。
pub const FIX_NOTE: &str = "成型规则把磁盘上的文件聚成变体。规则会出错，这里可以手动纠正；\
     记为人工纠正，按路径永久记住，删掉中立库重扫也还在。";

/// 那一层末尾那一句：盘上动没动、撤销在哪儿。
pub const FIX_FOOTNOTE: &str =
    "盘上的文件不移动、不改名；撤销在作品详情的「变体」那一面，撤掉就回到成型规则原本的结果。";

/// 「取消」。
pub const CANCEL: &str = "取消";

/// 合成那一颗（设计稿原话）。
pub const MERGE: &str = "合成一个变体";

/// 勾中的不够两个时说的那一句（设计稿 `DLG.shape` 原话）。
pub const NEED_TWO: &str = "至少选择两个变体才能合成。";

/// 库体检明细里一行右头那颗（设计稿 `DLG.shapes` 原话）。
pub const HANDLE: &str = "处理…";

/// 作品详情变体卡上那颗（设计稿变体卡头一行原话）。
pub const ADJUST: &str = "调整成型…";

/// 作品详情变体卡上撤销那一颗。
pub const UNDO: &str = "撤销成型纠正";

/// 重新成型那一趟在任务台上叫什么。
pub const RESHAPE_TASK: &str = "重新成型 · 全库";

/// 眼下开着的那一层「调整成型」。
#[derive(Debug, Clone)]
struct Case {
    /// 哪一种存疑。
    kind: DoubtKind,
    /// 那一处，给人看的完整路径（标题上那一截取它的末级）。
    at: String,
    /// 核心库那一句「凭什么说这里存疑」。
    reason: String,
    /// 同一种存疑一共几处（「另有 N 处」说的就是它）。
    total: u64,
    /// 多碟那一支：牵涉的那几个变体，连它们的成员、文件数与大小。
    variants: Vec<Candidate>,
    /// 目录那一支：那几份各自独立的内容的键。
    contents: Vec<String>,
}

/// 多碟那一支里的一个变体：勾没勾上，加上纠正要的那几样。
#[derive(Debug, Clone)]
struct Candidate {
    /// 勾上没有（默认全勾上——存疑说的就是「这几个多半是一套」）。
    picked: bool,
    /// 变体连它的成员：[`fix::merge`] 吃的就是这一份。
    members: fix::Members,
    /// 文件成员数。
    files: u64,
    /// 字节合计（下界，ADR-0021）。
    bytes: u64,
}

impl Case {
    /// 勾中的那几个变体（多碟那一支）。
    fn picked(&self) -> Vec<fix::Members> {
        self.variants
            .iter()
            .filter(|one| one.picked)
            .map(|one| one.members.clone())
            .collect()
    }

    /// 标题上那一截：那一处的末级名字。
    fn headline(&self) -> String {
        Path::new(&self.at)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&self.at)
            .to_string()
    }
}

/// 页脚上按下去的是哪一颗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pressed {
    /// 关掉这一层，什么都不落。
    Cancel,
    /// 落下去：合成，或者拆开。
    Go,
}

/// **成型纠正**那一层，连它落库与重新成型那条路。
///
/// 开它的那一屏拿着一个（库屏底下的库体检一个、浏览屏的作品详情一个）：弹层开没开着由拿着它的那一屏
/// 记（`crate::dialog` 那条规矩）。
#[derive(Debug, Default)]
pub struct Fixer {
    /// 眼下开着的那一处；没开是 `None`。
    open: Option<Case>,
    /// 按下去之后那句回话：`Ok` 是落成了，`Err` 是没落成。画在开它的那一屏上。
    said: Option<Result<String, String>>,
    /// 落过一笔、该重新成型了：拿着它的那一屏每帧问一次（[`Self::take_applied`]）。
    applied: bool,
}

impl Fixer {
    /// 这一层开着没有（测试拿它核对）。
    #[must_use]
    pub fn open(&self) -> bool {
        self.open.is_some()
    }

    /// 上一下落成没落成那句回话；画在开它的那一屏上。
    #[must_use]
    pub fn said(&self) -> Option<&Result<String, String>> {
        self.said.as_ref()
    }

    /// 把那句回话擦掉（换了一处、关掉那一层时）。
    pub fn forget(&mut self) {
        self.said = None;
    }

    /// **刚落过一笔人工纠正**：拿着它的那一屏据此排一趟重新成型。问过就清掉。
    pub fn take_applied(&mut self) -> bool {
        std::mem::take(&mut self.applied)
    }

    /// 对着一处**成型存疑**开这一层。读的是中立库里那几个变体与它们的成员——**只读几个键**，
    /// 摊在画帧这条线程上没问题。
    ///
    /// 读不出来就把话说清楚，不开一层空的。
    pub fn open_doubt(&mut self, catalog: &Catalog, doubt: &ShapingDoubt, total: u64) {
        self.said = None;
        let case = match build(catalog, doubt, total) {
            Ok(case) => case,
            Err(why) => {
                self.said = Some(Err(why));
                return;
            }
        };
        self.open = Some(case);
    }

    /// 画开着的那一层。交回 `true` 表示这一帧落过一笔（拿着它的那一屏据此排重新成型）。
    ///
    /// **每一帧都画**：弹层开没开着记在这一层上，不跟着底下那一屏的面板收起。
    pub fn ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(case) = self.open.clone() else {
            return;
        };
        let 够了 = match case.kind {
            DoubtKind::UnmergedDiscs => case.picked().len() >= 2,
            DoubtKind::CrowdedTree => case.contents.len() >= 2,
        };
        let 往前 = match case.kind {
            DoubtKind::UnmergedDiscs => MERGE.to_string(),
            DoubtKind::CrowdedTree => {
                format!("拆成 {} 个变体", thousands(case.contents.len() as u64))
            }
        };
        let footer = Footer::new(Button::new(CANCEL, Pressed::Cancel)).button(
            Button::new(往前, Pressed::Go)
                .primary()
                .enabled(够了)
                .hover("只往沉淀库记下这一处该怎么聚；盘上的文件不移动、不改名"),
        );
        let mut 勾了: Option<usize> = None;
        let shown = Dialog::new(FIX, format!("{FIX} · {}", case.headline()), footer)
            .note(FIX_NOTE)
            .width(Width::Wide)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = look::step(1);
                match case.kind {
                    DoubtKind::UnmergedDiscs => 勾了 = merge_body(ui, &case),
                    DoubtKind::CrowdedTree => split_body(ui, &case),
                }
                look::help(ui, FIX_FOOTNOTE);
            });
        if let Some(at) = 勾了
            && let Some(one) = self
                .open
                .as_mut()
                .and_then(|case| case.variants.get_mut(at))
        {
            one.picked = !one.picked;
        }
        match shown.pressed {
            Some(Pressed::Cancel) => {
                self.open = None;
            }
            Some(Pressed::Go) => {
                self.said = Some(self.apply(site, &case));
                self.open = None;
            }
            None => {}
        }
    }

    /// 往**沉淀库**落这一处的人工纠正。交回画在底下那一屏上的那句回话。
    fn apply(&mut self, site: &mut Site, case: &Case) -> Result<String, String> {
        let (overrides, 说的) = match case.kind {
            DoubtKind::UnmergedDiscs => {
                let picked = case.picked();
                let (overrides, merged) =
                    fix::merge(&picked).ok_or_else(|| NEED_TWO.to_string())?;
                let 话 = format!(
                    "已把 {} 个变体合成一个（记为人工纠正）：主文件是 {}，附属文件 {} 个；盘上的文件一个字节都没动",
                    thousands(picked.len() as u64),
                    name_of(&merged.main),
                    thousands(merged.companions.len() as u64),
                );
                (overrides, 话)
            }
            DoubtKind::CrowdedTree => {
                let overrides = fix::split(&case.contents)
                    .ok_or_else(|| "至少要有两份各自独立的内容才拆得开。".to_string())?;
                let 话 = format!(
                    "已拆成 {} 个变体（记为人工纠正）：它们各自参与下一趟识别；盘上的文件一个字节都没动",
                    thousands(case.contents.len() as u64),
                );
                (overrides, 话)
            }
        };
        write(site, &overrides, &[])?;
        self.applied = true;
        Ok(说的)
    }

    /// **撤销**一个变体上的人工纠正：清掉它全部成员那几行（[`fix::undo`]），交回那句回话。
    ///
    /// 清完要重新成型，这几条才回到规则算出来的地方——所以它也把[该重新成型了](Self::take_applied)
    /// 那个记号放下。
    pub fn undo(&mut self, site: &mut Site, variant: &fix::Members) {
        let keys = fix::undo(variant);
        self.said =
            Some(write(site, &BTreeMap::new(), &keys).map(|()| {
                "已撤销这一处的人工纠正：重新成型之后回到成型规则原本的结果".to_string()
            }));
        if self.said.as_ref().is_some_and(Result::is_ok) {
            self.applied = true;
        }
    }
}

/// 往沉淀库写这一批人工纠正：`set` 是要落的几行，`clear` 是要清掉的几条键。
fn write(site: &mut Site, set: &BTreeMap<String, String>, clear: &[String]) -> Result<(), String> {
    let library = site.library_identity.clone();
    for key in clear {
        site.store
            .clear_shaping_override(&library, key)
            .map_err(|why| format!("这一下没记进沉淀库：{why}"))?;
    }
    for (key, to) in set {
        site.store
            .set_shaping_override(&library, key, to)
            .map_err(|why| format!("这一下没记进沉淀库：{why}"))?;
    }
    Ok(())
}

/// 从中立库摊开一处存疑：那几个变体连成员（多碟），或者那几份独立内容（目录）。
fn build(catalog: &Catalog, doubt: &ShapingDoubt, total: u64) -> Result<Case, String> {
    let mut case = Case {
        kind: doubt.kind,
        at: doubt.at.clone(),
        reason: doubt.reason(),
        total,
        variants: Vec::new(),
        contents: Vec::new(),
    };
    match doubt.kind {
        DoubtKind::UnmergedDiscs => {
            let keys: Vec<&str> = doubt.item_keys.iter().map(String::as_str).collect();
            let rows = catalog
                .variants_of(&keys)
                .map_err(|why| format!("中立库读不出来：{why}"))?;
            let members = catalog
                .variant_members_of(&keys)
                .map_err(|why| format!("中立库读不出来：{why}"))?;
            let mut rows = rows;
            rows.sort_by(|a, b| a.key.cmp(&b.key));
            for row in rows {
                let member_keys = members
                    .get(&row.key)
                    .map(|list| list.iter().map(|(key, _)| key.clone()).collect())
                    .unwrap_or_else(|| vec![row.main_key.clone()]);
                case.variants.push(Candidate {
                    picked: true,
                    files: row.files,
                    bytes: row.bytes,
                    members: fix::Members {
                        key: row.key.clone(),
                        main_key: row.main_key.clone(),
                        members: member_keys,
                    },
                });
            }
            if case.variants.len() < 2 {
                return Err("这一处的变体在中立库里已经不在了，重新体检一趟再看。".to_string());
            }
        }
        DoubtKind::CrowdedTree => case.contents.clone_from(&doubt.item_keys),
    }
    Ok(case)
}

/// 多碟那一支的正文（设计稿 `DLG.shape` 的 `disc` 那一支）：勾选、线索、纠正后的预览、会怎样。
///
/// 交回这一帧点了第几行的勾。
fn merge_body(ui: &mut egui::Ui, case: &Case) -> Option<usize> {
    let tokens = Tokens::builtin();
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 字体 = egui::FontId::new(字号, egui::FontFamily::Monospace);
    let mut 勾了 = None;
    look::section(
        ui,
        &format!(
            "现在：这 {} 个变体各自独立",
            thousands(case.variants.len() as u64)
        ),
    );
    for (at, one) in case.variants.iter().enumerate() {
        let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
        let [上下, 左右] = tokens.space.cell_padding;
        egui::Frame::new()
            .stroke(线)
            .corner_radius(tokens.radius.large)
            .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let mut 勾着 = one.picked;
                    if ui.checkbox(&mut 勾着, "").changed() {
                        勾了 = Some(at);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        look::help(
                            ui,
                            &format!(
                                "{} 个文件 · {}",
                                thousands(one.files),
                                human_bytes(one.bytes)
                            ),
                        );
                        let 宽 = ui.available_width();
                        let 画的 = table::root_and_path(ui, &one.members.key, &字体, 宽);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(font::mono(画的).size(字号));
                        });
                    });
                });
            });
    }
    look::note(ui, format!("线索：{}", case.reason));
    look::section(ui, "纠正后");
    let picked = case.picked();
    match fix::merge(&picked) {
        Some((_, merged)) => {
            look::impact(
                ui,
                &[
                    ("主文件：", false),
                    (&name_of(&merged.main), true),
                    (
                        &format!(
                            "，附属文件 {} 个（一共 {} 个文件成员）",
                            thousands(merged.companions.len() as u64),
                            thousands(merged.files() as u64),
                        ),
                        false,
                    ),
                ],
            );
            look::impact(
                ui,
                &[(
                    &format!(
                        "这一处的变体从 {} 个变为 1 个；记为人工纠正，不移动或修改任何文件。",
                        thousands(case.variants.len() as u64)
                    ),
                    false,
                )],
            );
        }
        None => {
            ui.colored_label(ui.visuals().error_fg_color, NEED_TWO);
        }
    }
    others(ui, case);
    勾了
}

/// 目录那一支的正文（设计稿 `DLG.shape` 的 `dir` 那一支）：里头有哪几份、线索、纠正为、会怎样。
fn split_body(ui: &mut egui::Ui, case: &Case) {
    let tokens = Tokens::builtin();
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    look::section(ui, "现在：整个目录被当成 1 个变体");
    let 线 = ui.visuals().widgets.noninteractive.bg_stroke;
    let [上下, 左右] = tokens.space.cell_padding;
    egui::Frame::new()
        .stroke(线)
        .corner_radius(tokens.radius.large)
        .inner_margin(egui::Margin::from(egui::vec2(左右, 上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            for key in &case.contents {
                ui.label(font::mono(name_of(key)).size(字号));
            }
        });
    look::note(ui, format!("线索：{}", case.reason));
    look::section(ui, "纠正为");
    look::radio_option(
        ui,
        true,
        &format!(
            "拆成 {} 个变体，每份内容一个",
            thousands(case.contents.len() as u64)
        ),
        "拆开后各自参与下一趟识别：按内容命中的直接归入对应作品，其余进待确认队列",
    );
    look::impact(ui, &[("记为人工纠正，不移动或修改任何文件。", false)]);
    others(ui, case);
}

/// 「同样结构的另有 N 处」那一句（票面验收第 6 条：**逐处确认，不一次性全改**）。
///
/// 数取核心库按种类分的那一格（`ShapingDoubtSummary::by_kind`），**界面不自己数**。
fn others(ui: &mut egui::Ui, case: &Case) {
    let 别处 = case.total.saturating_sub(1);
    if 别处 == 0 {
        return;
    }
    look::help(
        ui,
        &format!(
            "同样是「{}」的另有 {} 处，列在库体检的「成型存疑」里——逐处确认，这一下只改这一处。",
            case.kind.label(),
            thousands(别处),
        ),
    );
}

/// 一条键的末级名字：屏上说「主文件是谁」时印它。
fn name_of(key: &str) -> String {
    romcat_core::path::file_name_of_key(key).to_string()
}

/// **重新成型**那一趟：照沉淀库里的人工纠正把中立库里的变体整批重算，顺手把**库体检**也重跑一趟
/// ——屏上那份「成型存疑」的名单正是它折出来的。
///
/// 交回任务号；台上那条线程自己开一份写得动的中立库（`rusqlite::Connection` 不是 `Sync`，界面这条线程
/// 手里那一份交不过去），与扫描、识别、刮削同一条路。**只活在内存里的库就地跑完**（合成数据那一路）。
///
/// # Errors
/// 沉淀库读不出来、或者中立库里还没有遍历记录时交回那句话。
pub fn reshape(site: &mut Site, tasks: &mut Tasks) -> Result<u64, String> {
    let overrides = site
        .shaping_overrides()
        .map_err(|why| format!("沉淀库读不出来：{why}"))?;
    let scan = match site.catalog.last_traversal() {
        Ok(Some(traversal)) => traversal.scan,
        Ok(None) => {
            return Err("这份库还没扫过，没有可重新成型的东西。先扫一趟。".to_string());
        }
        Err(why) => return Err(format!("中立库读不出来：{why}")),
    };
    match site.catalog.file().map(Path::to_path_buf) {
        Some(file) => Ok(tasks.queue(RESHAPE_TASK, move |task| {
            let mut catalog =
                Catalog::open(&file).map_err(|why| Cutoff::failed(why.to_string()))?;
            reshape_run(&mut catalog, &overrides, scan, task)
        })),
        // 只活在内存里的那一份分不出第二份连接，就地跑完（同库体检那一处）。
        None => Ok(tasks.run_here(RESHAPE_TASK, |task| {
            reshape_run(&mut site.catalog, &overrides, scan, task)
        })),
    }
}

/// 重新成型加一趟体检，交回那份新报告。
fn reshape_run(
    catalog: &mut Catalog,
    overrides: &BTreeMap<String, String>,
    scan: i64,
    task: &Handle,
) -> Result<Product, Cutoff> {
    let manifest = Manifest::builtin();
    task.check()?;
    let failed = |error: CatalogError| Cutoff::failed(format!("重新成型没成：{error}"));
    shape::reshape(catalog, &manifest, overrides, scan).map_err(failed)?;
    task.check()?;
    // **明细列全**：与「重新体检」那一趟同一套上限（`health::check_run`，挂单 `Q959`）。
    let limits = Limits {
        max_examples: usize::MAX,
        max_duplicate_paths_per_group: Limits::FULL_DUPLICATE_PATHS_PER_GROUP,
        ..Limits::default()
    };
    let aggregate = catalog.aggregate(&limits, &manifest).map_err(failed)?;
    let report = HealthReport::build_full(&aggregate, &catalog.report_meta().map_err(failed)?);
    let duplicates = DuplicateDetails::build(&aggregate, &report);
    Ok(Product::Checked {
        report: Box::new(report),
        duplicates: Box::new(duplicates),
    })
}
