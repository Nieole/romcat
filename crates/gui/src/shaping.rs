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
use romcat_core::scan::aggregate::Limits;
use romcat_core::shape::{self, Doubt, DoubtKind, fix};
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

/// 合成的预览里最多逐条列几个**附属文件**，其余写「另有 N 个」。
///
/// 多碟合成的成员数按张数算（一张碟一个 `.cue` 一个 `.bin`），十条够摆得下十碟；
/// 再多就不是给人一眼核对的了。
const COMPANIONS: usize = 10;

/// 眼下开着的那一层「调整成型」。
#[derive(Debug, Clone)]
struct Spot {
    /// 哪一种存疑。
    kind: DoubtKind,
    /// 那一处，给人看的完整路径（标题上那一截取它的末级）。
    at: String,
    /// 核心库那一句「凭什么说这里存疑」。
    reason: String,
    /// 同一种存疑**全库**一共几处（「同样的另有 N 处」说的就是它）；说不出是 `None`。
    same_kind: Option<u64>,
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

impl Spot {
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
    open: Option<Spot>,
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

    /// 把这一层连同那句回话一起关掉：画它的那一屏自己走了（作品详情页关了）时收拾这一下。
    pub fn close(&mut self) {
        self.open = None;
        self.said = None;
    }

    /// **刚落过一笔人工纠正**：拿着它的那一屏据此排一趟重新成型。问过就清掉。
    pub fn take_applied(&mut self) -> bool {
        std::mem::take(&mut self.applied)
    }

    /// 对着一处**成型存疑**开这一层。读的是中立库里那几个变体与它们的成员——**只读几个键**，
    /// 摊在画帧这条线程上没问题。
    ///
    /// 吃的是核心库那一份 [`shape::Doubt`]（装的是**中立库的键**），不是报告里折成展示路径的那一份
    /// （[`ShapingDoubt`](romcat_core::scan::aggregate::ShapingDoubt)）——人工纠正记的是键。
    /// 从库体检那一格进来的人把报告那一份的
    /// `at_key` / `item_keys` 折回这一份。
    ///
    /// `same_kind` 是**同一种存疑全库一共几处**，用来说「同样的另有 N 处」；**说不出那个数就交
    /// `None`**（作品详情那一面手上只有这一个作品跟前那几处，数得出的是另一个数），那时屏上不写数。
    ///
    /// 读不出来就把话说清楚，不开一层空的。
    pub fn open_doubt(&mut self, catalog: &Catalog, doubt: &Doubt, same_kind: Option<u64>) {
        self.said = None;
        match read_spot(catalog, doubt, same_kind) {
            Ok(spot) => self.open = Some(spot),
            Err(why) => self.said = Some(Err(why)),
        }
    }

    /// 画开着的那一层。交回 `true` 表示这一帧落过一笔（拿着它的那一屏据此排重新成型）。
    ///
    /// **每一帧都画**：弹层开没开着记在这一层上，不跟着底下那一屏的面板收起。
    pub fn ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(spot) = self.open.clone() else {
            return;
        };
        // **按不动只有一种情形**：多碟那一支勾中的不够两个——屏上常驻着那句理由（`NEED_TWO`，
        // 画在 `merge_body` 里；ADR-0005 修订段要的那两条）。拆那一支开得出来就一定够两份
        // （`read_spot` 那道闸），没有第二种按不动的态。
        let 够了 = spot.kind != DoubtKind::UnmergedDiscs || spot.picked().len() >= 2;
        let 往前 = match spot.kind {
            DoubtKind::UnmergedDiscs => MERGE.to_string(),
            DoubtKind::CrowdedTree => format!("拆成 {} 个变体", thousands(数(spot.contents.len()))),
        };
        let footer = Footer::new(Button::new(CANCEL, Pressed::Cancel)).button(
            Button::new(往前, Pressed::Go)
                .primary()
                .enabled(够了)
                .hover("只往沉淀库记下这一处该怎么聚；盘上的文件不移动、不改名"),
        );
        let mut 勾了: Option<usize> = None;
        let shown = Dialog::new(FIX, format!("{FIX} · {}", spot.headline()), footer)
            .note(FIX_NOTE)
            .width(Width::Wide)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing.y = look::step(1);
                match spot.kind {
                    DoubtKind::UnmergedDiscs => 勾了 = merge_body(ui, &spot),
                    DoubtKind::CrowdedTree => split_body(ui, &spot),
                }
                look::help(ui, FIX_FOOTNOTE);
            });
        if let Some(at) = 勾了
            && let Some(one) = self
                .open
                .as_mut()
                .and_then(|spot| spot.variants.get_mut(at))
        {
            one.picked = !one.picked;
        }
        match shown.pressed {
            Some(Pressed::Cancel) => {
                self.open = None;
            }
            Some(Pressed::Go) => {
                self.said = Some(self.apply(site, &spot));
                self.open = None;
            }
            None => {}
        }
    }

    /// 往**沉淀库**落这一处的人工纠正。交回画在底下那一屏上的那句回话。
    fn apply(&mut self, site: &mut Site, spot: &Spot) -> Result<String, String> {
        let (overrides, 说的) = match spot.kind {
            DoubtKind::UnmergedDiscs => {
                let picked = spot.picked();
                let (overrides, merged) =
                    fix::merge(&picked).ok_or_else(|| NEED_TWO.to_string())?;
                let 话 = format!(
                    "已把 {} 个变体合成一个（记为人工纠正）：主文件是 {}，附属文件 {} 个；盘上的文件一个字节都没动",
                    thousands(数(picked.len())),
                    name_of(&merged.main),
                    thousands(数(merged.companions.len())),
                );
                (overrides, 话)
            }
            DoubtKind::CrowdedTree => {
                // `read_spot` 那道闸保证这里至少两份，`fix::split` 因此交不回 `None`。
                let overrides = fix::split(&spot.contents)
                    .ok_or_else(|| "这一处只剩一份内容了，没什么可拆的。".to_string())?;
                let 话 = format!(
                    "已拆成 {} 个变体（记为人工纠正）：它们各自参与下一趟识别；盘上的文件一个字节都没动",
                    thousands(数(spot.contents.len())),
                );
                (overrides, 话)
            }
        };
        record(site, &overrides, &[])?;
        self.applied = true;
        Ok(说的)
    }

    /// **撤销这一处**的人工纠正：清掉[同一处一起纠正出来的那几个变体](fix::undo)全部成员那几行，
    /// 交回那句回话。
    ///
    /// `spot` 由核心库折（`Catalog::shaping_fix_group`）：**拆开**那一处落的是那几份内容各一行，
    /// 只撤其中一份回不到成型规则原本的结果。
    ///
    /// 清完要重新成型，这几条才回到规则算出来的地方——所以它也把[该重新成型了](Self::take_applied)
    /// 那个记号放下。
    pub fn undo(&mut self, site: &mut Site, spot: &[fix::Members]) {
        let keys = fix::undo(spot);
        let 几份 = spot.len();
        self.said = Some(record(site, &BTreeMap::new(), &keys).map(|()| {
            if 几份 > 1 {
                format!(
                    "已撤销这一处的人工纠正（同一下拆出来的 {} 份一起撤）：重新成型之后回到成型规则原本的结果",
                    thousands(数(几份))
                )
            } else {
                "已撤销这一处的人工纠正：重新成型之后回到成型规则原本的结果".to_string()
            }
        }));
        if self.said.as_ref().is_some_and(Result::is_ok) {
            self.applied = true;
        }
    }
}

/// 屏上要印的一个计数从 `usize` 折成 `u64`：这几处数的都是屏上摆着的那几行，溢不出去。
fn 数(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

/// 往沉淀库记这一批人工纠正：`set` 是要落的几行，`clear` 是要清掉的几条键。
fn record(site: &mut Site, set: &BTreeMap<String, String>, clear: &[String]) -> Result<(), String> {
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
fn read_spot(catalog: &Catalog, doubt: &Doubt, same_kind: Option<u64>) -> Result<Spot, String> {
    let mut spot = Spot {
        kind: doubt.kind,
        at: doubt.at.clone(),
        reason: doubt.reason(),
        same_kind,
        variants: Vec::new(),
        contents: Vec::new(),
    };
    match doubt.kind {
        DoubtKind::UnmergedDiscs => {
            let keys: Vec<&str> = doubt.items.iter().map(String::as_str).collect();
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
                spot.variants.push(Candidate {
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
            if spot.variants.len() < 2 {
                return Err("这一处的变体在中立库里已经不在了，重新体检一趟再看。".to_string());
            }
        }
        DoubtKind::CrowdedTree => {
            // **少于两份不开这一层**：一份内容的目录本来就该是一个变体，没什么可拆的
            // （`fix::split` 那一条同样的闸）。开出来的话页脚那颗按不动，而屏上说不出为什么。
            if doubt.items.len() < 2 {
                return Err("这一处在中立库里只剩一份内容了，重新体检一趟再看。".to_string());
            }
            spot.contents.clone_from(&doubt.items);
        }
    }
    Ok(spot)
}

/// 多碟那一支的正文（设计稿 `DLG.shape` 的 `disc` 那一支）：勾选、线索、纠正后的预览、会怎样。
///
/// 交回这一帧点了第几行的勾。
fn merge_body(ui: &mut egui::Ui, spot: &Spot) -> Option<usize> {
    let tokens = Tokens::builtin();
    let 字号 = look::font_size(ui.ctx(), tokens.font.size_caption_plus);
    let 字体 = egui::FontId::new(字号, egui::FontFamily::Monospace);
    let mut 勾了 = None;
    look::section(
        ui,
        &format!(
            "现在：这 {} 个变体各自独立",
            thousands(数(spot.variants.len()))
        ),
    );
    for (at, one) in spot.variants.iter().enumerate() {
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
    look::note(ui, format!("线索：{}", spot.reason));
    look::section(ui, "纠正后");
    let picked = spot.picked();
    match fix::merge(&picked) {
        Some((_, merged)) => {
            look::impact(
                ui,
                &[
                    ("主文件：", false),
                    (&name_of(&merged.main), true),
                    (
                        &format!("（一共 {} 个文件成员）", thousands(数(merged.files()))),
                        false,
                    ),
                ],
            );
            // **附属文件逐条列出来**（票面第 2 条「预览合成后的主文件与**附属文件**」）：
            // 只说个数的话，人看不出合进来的是不是他勾的那几份。长了就说「另有 N 个」。
            look::impact(
                ui,
                &[(
                    &format!("附属文件 {} 个：", thousands(数(merged.companions.len()))),
                    false,
                )],
            );
            // **缩进到与上面那一条「会怎样」的字同一条线上**：`look::impact` 把圆点摆进
            // `impact-column` 那一列，再由 `horizontal_top` 隔一个 `item_spacing.x` 才画字
            // ——少算那一格的话，这几行会落在圆点与字之间，屏上与谁都对不齐（实测差 9 点）。
            let 缩 = tokens.layout.impact_column + ui.spacing().item_spacing.x;
            for 一条 in merged.companions.iter().take(COMPANIONS) {
                ui.horizontal(|ui| {
                    ui.add_space(缩);
                    ui.label(font::mono(name_of(一条)).size(字号));
                });
            }
            let 少了 = merged.companions.len().saturating_sub(COMPANIONS);
            if 少了 > 0 {
                ui.horizontal(|ui| {
                    ui.add_space(缩);
                    look::help(ui, &format!("另有 {} 个", thousands(数(少了))));
                });
            }
            // **数的是勾中的那几个**，不是这一处全部候选：勾掉一个，这句话得跟着变。
            //
            // **不在这儿再说一遍「记为人工纠正、不移动任何文件」**：那句话标题底下那段说明
            // 与这一层末尾那一句各说过一次了，第三遍只会把这几条「这一下会怎样」冲淡。
            look::impact(
                ui,
                &[(
                    &format!("勾中的 {} 个变体合成 1 个。", thousands(数(picked.len()))),
                    false,
                )],
            );
        }
        None => {
            ui.colored_label(ui.visuals().error_fg_color, NEED_TWO);
        }
    }
    same_kind_note(ui, spot);
    勾了
}

/// 目录那一支的正文（设计稿 `DLG.shape` 的 `dir` 那一支）：里头有哪几份、线索、纠正为、会怎样。
fn split_body(ui: &mut egui::Ui, spot: &Spot) {
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
            for key in &spot.contents {
                ui.label(font::mono(name_of(key)).size(字号));
            }
        });
    look::note(ui, format!("线索：{}", spot.reason));
    look::section(ui, "纠正为");
    look::radio_option(
        ui,
        true,
        &format!(
            "拆成 {} 个变体，每份内容一个",
            thousands(数(spot.contents.len()))
        ),
        "拆开后各自参与下一趟识别：按内容命中的直接归入对应作品，其余进待确认队列",
    );
    // **这一支不再摆一条「会怎样」**：要说的那句（记为人工纠正、不移动任何文件）标题底下那段说明
    // 与末尾那一句已经各说过一次，而摆在这儿它紧挨着上面那一档单选——**两行的字对不齐**
    // （单选的字落在 `radio-diameter` + `radio-gap` 那一列，圆点的字落在 `impact-column` 那一列，
    // 实测差 5 点）。合成那一支的三条「会怎样」说的是这一处独有的事实，那三条留着。
    same_kind_note(ui, spot);
}

/// 「同样结构的其余几处在哪儿」那一句（票面验收第 6 条：**逐处确认，不一次性全改**）。
///
/// **数说不出就不写数**：那个数只有**全库**那一份报告答得出（`ShapingDoubtSummary::by_kind`，
/// 库体检那条入口交得进来），而作品详情那一面手上只有这一个作品跟前那几处——**界面不自己凑一个**
/// （ADR-0024）。两条入口那句话的后半截是同一句，人从哪儿进来读到的规矩都一样。
fn same_kind_note(ui: &mut egui::Ui, spot: &Spot) {
    let 另有 = match spot.same_kind {
        Some(全库) if 全库 > 1 => format!("的另有 {} 处", thousands(全库 - 1)),
        // 全库就这一处：那句「去哪儿逐处确认」就不必说了。
        Some(_) => return,
        None => "的其余几处".to_string(),
    };
    look::help(
        ui,
        &format!(
            "同样是「{}」{另有}，列在库体检的「成型存疑」里——逐处确认，这一下只改这一处。",
            spot.kind.label(),
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
