//! **主列表**：一个游戏一行，以及它背后那扇**窗**。
//!
//! ## 一行是一个作品，不是一个变体
//!
//! 真库上 46,444 个变体收敛成一万出头的行——六成作品下面挂着不止一个变体，那几个
//! 在详情面板里挑。这一层不做收敛，收敛在中立库里
//! （[`romcat_core::catalog::browse`] 的 `GROUP BY`）：把变体表在界面里聚合，
//! 等于先要看见全部行才数得出「这个作品有几个变体」，而那正是这一层从头到尾在躲的事。
//!
//! ## 两层各管一件事
//!
//! - [`Window`] 管**内存里装多少**。它只留当前视口附近的一段，行数由 [`Window::span`]
//!   定死，与库里有多少行无关。要哪一行就问它，不在窗里就去中立库取一段回来。
//! - [`Table`] 管**画出来什么**。`egui_extras::TableBuilder` 的 `body.rows()` 只调用
//!   视口里那几十行的闭包，开销与总行数无关（官方 demo 的行数滑杆上限就是 100,000）。
//!
//! 排序尤其要说一句——`egui_extras` 完全没有排序能力（源码里 `sort` 零出现），
//! 于是它只能由别人做；做在 `ORDER BY` 里而不是内存的 `Vec` 上，正是因为后者要求
//! 先把全库读进来。
//!
//! ## 行高等高，绝不用 `heterogeneous_rows`
//!
//! 后者的开销是 O(总行数)——它要把每一行的高度都过一遍才知道视口从哪开始。
//! 等高行下 `rows()` 是一次除法。
//!
//! ## 选中是「筛选加例外」，不是一万个键
//!
//! [`Picked`] 的两支照 ADR-0016 那条「规则加手动例外」来：全选不把一万行的身份抓进
//! 内存，它就是**当前这个筛选**本身，再减去人点掉的那几行。批量操作要动哪些变体，
//! 由核心库照这两样展开（[`Catalog::scoped_variants`]）。

use egui::{Align, Layout};
use egui_extras::{Column, TableBuilder};
use romcat_core::catalog::Catalog;
use romcat_core::catalog::browse::{
    NON_GAME_ASSET_LABEL, Scope, SearchHit, WorkAnchor, WorkOrder, WorkQuery, WorkRow,
};
use romcat_core::filename::Rules;
use romcat_core::report::{capacity, thousands};
use romcat_core::scrape::Priorities;

use crate::font;
use crate::look;
use crate::media::Shelf;
use crate::tokens::Tokens;

/// 一行多高，点。**待确认屏、子库屏那几张一行字的表**用它。
///
/// 十万行乘以它约 2.1×10⁶ 点，而 `f32` 在那个量级上的最小间隔是 0.25 点——距离滚动抖动
/// 的阈值还有一个数量级以上的余量（egui#1391）。
///
/// 浏览屏的主列表不用它：那一张照稿是两行字一行（[`row_height`]）。
pub const ROW_HEIGHT: f32 = 21.0;

/// 浏览屏**主列表**一行多高，点：照令牌 `table-row`，摆得下名字一行、副行一行。
///
/// 十万行乘以它约 4.6×10⁶ 点，`f32` 在那个量级上的最小间隔是 0.5 点，离滚动抖动的阈值
/// 照旧差得远（见 [`ROW_HEIGHT`] 那一条）。
#[must_use]
pub fn row_height() -> f32 {
    Tokens::builtin().layout.table_row
}

/// 认不出作品的那一行挂的**标签**。
///
/// 那一行是不是认不出作品，由核心库答（[`WorkAnchor::Loose`]）；这几个字是屏上怎么称呼
/// 那种行——卡片视图与作品详情页挂的是同一个标签，所以摆在这儿一处。
pub const UNLINKED_LABEL: &str = "未关联作品";

/// 窗口默认一次取多少行。
///
/// 视口撑死几十行，取 512 是给上下滚动留预取余量：往下翻过 3/4 个窗口才需要再查一次库。
/// 它同时是**内存占用的上界**——不管库里是一万行还是一百万行，这里只有 512 个
/// [`WorkRow`]。
pub const SPAN: u64 = 512;

/// 表格背后那扇窗：内存里只装当前视口那一段。
///
/// 换排序或换筛选**不是**在窗里重排，是把窗整个作废，重新问中立库要——那才是
/// 「排序下推」这句话的实际含义。
pub struct Window {
    query: WorkQuery,
    /// 满足筛选条件的总行数。滚动条的长度由它来，与窗里装了几行无关。
    total: u64,
    /// 窗里第一行在全序里的下标。
    first: u64,
    rows: Vec<WorkRow>,
    span: u64,
    /// 一共查了几次库。测试拿它证明**不是每帧都查**。
    reads: u64,
    /// 上一次读库的错误；`None` 表示一切正常。
    ///
    /// 它同时是**闸门**：有错时不再读库。表格每帧都在问行，读不动的时候一帧一次地重试
    /// 只会把同一个错误刷满状态栏，而下一帧成功的理由并不存在——换筛选或换排序才是。
    error: Option<String>,
    /// 查询换过了，总数与内容都得重取。
    stale: bool,
    /// 挑**显示标题**用的那份优先级表（[`Catalog::work_page_with_titles`]）。与详情面板、导出交的是
    /// 同一份（[`crate::browse::Screen::set_priorities`]），不然屏上这一行与详情头上是两个名字。
    priorities: Priorities,
}

impl Window {
    /// 开一扇窗，一次取 `span` 行。`span` 会被夹进 `[1, MAX_PAGE]`。
    #[must_use]
    pub fn new(span: u64) -> Self {
        Self {
            query: WorkQuery::default(),
            total: 0,
            first: 0,
            rows: Vec::new(),
            span: span.clamp(1, romcat_core::catalog::MAX_PAGE),
            reads: 0,
            error: None,
            stale: true,
            priorities: Priorities::builtin(),
        }
    }

    /// 换一份优先级表。**换了就作废**：窗里那几行的显示标题是照旧那一份挑的。
    pub fn set_priorities(&mut self, priorities: Priorities) {
        if self.priorities != priorities {
            self.priorities = priorities;
            self.invalidate();
        }
    }

    /// 当前的筛选与排序。
    #[must_use]
    pub fn query(&self) -> &WorkQuery {
        &self.query
    }

    /// 换一套筛选与排序。和现在这套一样就什么都不做——界面每帧都会调它。
    pub fn set_query(&mut self, query: WorkQuery) {
        if self.query != query {
            self.query = query;
            self.rows.clear();
            self.first = 0;
            // 换了条件就重新试一次：上一次读不动的可能正是这条件本身。
            self.error = None;
            self.stale = true;
        }
    }

    /// **库变了**：窗里缓着的那一段整个作废，下一帧重新问中立库要。
    ///
    /// 与 [`Window::set_query`] 是两件事：那一个说「要的不是这一批了」，这一个说
    /// 「要的还是这一批，但库底下已经不是刚才那份了」。扫完一个根、裁完一批、刮削跑完
    /// 都走这条——查询一个字没改，所以 `set_query` 一律是空操作，而窗里那 512 行连同
    /// 总数、连同**筛选面板上那几档**全是旧的（[`crate::browse::Screen::invalidate`]）；
    /// 刮削那一趟刚写进去的正是行上那几列（元数据齐不齐、年份），不作废的话人要滚出
    /// 视口再滚回来才看得见。
    pub fn invalidate(&mut self) {
        self.rows.clear();
        self.first = 0;
        // 上一次读不动的理由可能已经不在了（那一趟扫描正是去补它的）。
        self.error = None;
        self.stale = true;
    }

    /// 满足筛选条件的总行数。
    #[must_use]
    pub fn total(&self) -> u64 {
        self.total
    }

    /// 此刻内存里装着几行。**这个数与 `total` 无关**，上界是 `span`。
    #[must_use]
    pub fn retained(&self) -> usize {
        self.rows.len()
    }

    /// 窗口一次取多少行。
    #[must_use]
    pub fn span(&self) -> u64 {
        self.span
    }

    /// 一共读了几次中立库。
    #[must_use]
    pub fn reads(&self) -> u64 {
        self.reads
    }

    /// 上一次读库出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 换过查询之后重新数一遍总行数。每帧开头调一次，没换过就是空操作。
    pub fn sync(&mut self, catalog: &Catalog) {
        if !self.stale {
            return;
        }
        self.stale = false;
        match catalog.work_total(&self.query) {
            Ok(total) => {
                self.total = total;
                self.error = None;
            }
            Err(error) => {
                self.total = 0;
                self.error = Some(error.to_string());
            }
        }
    }

    /// 取全序里第 `index` 行；不在窗里就去库里取一段回来。
    ///
    /// 越界、读库出过错、这一行取不到，都返回 `None`——表格照样画得下去，只是那一行是
    /// 空的，出错的原因在 [`Window::error`] 里。
    pub fn row(&mut self, catalog: &Catalog, index: u64) -> Option<&WorkRow> {
        if index >= self.total || self.error.is_some() {
            return None;
        }
        if !self.holds(index) {
            self.fill(catalog, index);
        }
        let at = usize::try_from(index.checked_sub(self.first)?).ok()?;
        self.rows.get(at)
    }

    fn holds(&self, index: u64) -> bool {
        index >= self.first && index < self.first + self.rows.len() as u64
    }

    /// 取一段把 `index` 包住的行。
    ///
    /// 起点往前退四分之一个窗口：往回滚一点点不该触发重取，而往下滚是主要方向，
    /// 剩下的四分之三留给它。
    fn fill(&mut self, catalog: &Catalog, index: u64) {
        let first = index.saturating_sub(self.span / 4);
        // 连显示标题一起取：认出作品的那几行主栏印它（`name_cell`）。
        match catalog.work_page_with_titles(&self.query, first, self.span, &self.priorities) {
            Ok(rows) => {
                self.first = first;
                // 整段换掉而不是追加：窗口的行数因此恒等于一页的大小，
                // 滚一整趟十万行也不会攒出第二段来。
                self.rows = rows;
                self.reads += 1;
                self.error = None;
            }
            Err(error) => {
                self.rows.clear();
                self.error = Some(error.to_string());
            }
        }
    }
}

/// **选中了哪些行**。
///
/// 两支照 ADR-0016 那条「规则加手动例外」来：
///
/// - 没全选时，`rows` 是**点选的**那几行。
/// - 全选时，`rows` 是**点掉的**那几行——全选本身不是一万个身份，
///   它就是当前这个筛选。
///
/// **换筛选就得清空**（[`Screen`](crate::browse::Screen) 每帧比一次）：全选说的是
/// 「当前筛出来的这一批」，条件一改那批就不是同一批了，留着上一批的选中会让批量操作
/// 作用到人根本没看见的行上。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Picked {
    all: bool,
    /// 点选（或全选时点掉）的那几行，**排好序**，`contains` 走二分。
    rows: Vec<WorkAnchor>,
}

impl Picked {
    /// 这一行选中了吗。
    #[must_use]
    pub fn contains(&self, anchor: &WorkAnchor) -> bool {
        self.all != self.rows.binary_search(anchor).is_ok()
    }

    /// 点一下这一行的选中框。
    pub fn toggle(&mut self, anchor: &WorkAnchor) {
        match self.rows.binary_search(anchor) {
            Ok(at) => {
                self.rows.remove(at);
            }
            Err(at) => self.rows.insert(at, anchor.clone()),
        }
    }

    /// **全选**：当前筛选下的每一行。
    pub fn select_all(&mut self) {
        self.all = true;
        self.rows.clear();
    }

    /// 一行都不选。
    pub fn clear(&mut self) {
        self.all = false;
        self.rows.clear();
    }

    /// 全选着吗。
    #[must_use]
    pub fn is_all(&self) -> bool {
        self.all
    }

    /// 选中了多少行。`total` 是当前筛选下的总行数——全选那一支只有靠它才数得出来。
    #[must_use]
    pub fn count(&self, total: u64) -> u64 {
        if self.all {
            total.saturating_sub(self.rows.len() as u64)
        } else {
            self.rows.len() as u64
        }
    }

    /// 一行都没选中吗。
    #[must_use]
    pub fn is_empty(&self, total: u64) -> bool {
        self.count(total) == 0
    }

    /// 交给核心库去展开的那份**作用范围**。
    #[must_use]
    pub fn scope(&self) -> Scope<'_> {
        if self.all {
            Scope::AllExcept(&self.rows)
        } else {
            Scope::Rows(&self.rows)
        }
    }
}

/// 一张主列表。
///
/// 它**直接改 `query`**：点表头就是换排序，而排序是中立库那一层的事，界面这边只是把
/// 用户点的那一下写进查询里。多存一份「点了哪个表头」的状态只会与查询漂开。
pub struct Table<'a> {
    /// 数据从哪来。
    pub catalog: &'a Catalog,
    /// 窗口。
    pub window: &'a mut Window,
    /// 筛选与排序。**界面那一份的引用，不是副本。**
    pub query: &'a mut WorkQuery,
    /// 高亮的是哪一行（全序下标）——**详情面板摆的就是它**。
    ///
    /// 与[选中](Self::picked)不是一回事：高亮一行是「我要看它」，选中一批是
    /// 「我要对它们动手」。混成一件事的话，翻着看就会把批量操作的范围改掉。
    pub focused: &'a mut Option<u64>,
    /// 选中了哪几行（批量操作的作用范围）。
    pub picked: &'a mut Picked,
    /// 把滚动位置强按到这个像素偏移。**只有量帧率时才用**，界面上是 `None`。
    pub scroll_to: Option<f32>,
    /// **剥离规则**：认不出作品的那一行拿它剥正题（[`WorkRow::title`]）。
    ///
    /// 与刮削撞中文离线源用的是同一份（工作目录里那份，`sources::rules`）——
    /// 屏上这一行写的正题，就是刮削依据里写的那一个。
    pub rules: &'a Rules,
    /// **行首那一小格封面**：「在每行开头显示封面」开着时是 `Some`，关着是 `None`
    /// ——那时行首什么都不摆，行也照令牌矮回 `table-row`。
    pub shelf: Option<&'a mut Shelf>,
}

impl Table<'_> {
    /// 画出来，返回这一帧里被点开的那一行。
    ///
    /// 返回的是**一份拷贝**而不是下标：详情面板要在那一行滚出视口之后照样显示得出来。
    #[allow(clippy::too_many_lines)]
    pub fn show(self, ui: &mut egui::Ui) -> Option<WorkRow> {
        let Self {
            catalog,
            window,
            query,
            focused,
            picked,
            scroll_to,
            rules,
            mut shelf,
        } = self;
        let tokens = Tokens::builtin();
        // 行首摆封面时一行照令牌 `table-row-cover` 高：两行字旁边还得竖得下那一小格封面。
        let height = if shelf.is_some() {
            tokens.layout.table_row_cover
        } else {
            row_height()
        };
        let mut opened = None;
        // 行画完之后手上没有那一行的 `Ui` 了（列都加完才拿得到 `response`），
        // 而焦点那一圈、行底下那条分隔线要画在那时——先把上下文留一份。
        let ctx = ui.ctx().clone();
        let total_rows = window.total();
        let total = usize::try_from(total_rows).unwrap_or(usize::MAX);
        let (sorted_by, descending) = (query.order, query.descending);
        // **格子里**照旧用这一屏的间距；**格与格、行与行之间**一点缝都不留，每一格自己让出左右留白
        // （[`padded`]）——那样定宽那几列正好是稿上写的宽，选中那一行的底色也连成一整条。
        let spacing = ui.spacing().item_spacing;
        let cell_x = tokens.space.table_cell_padding;
        let check = [tokens.layout.check_padding, 0.0];
        // 半号字号走 `look::font_size` 取整（票 `gui-looks-like-the-design/32` 定的统一入口）。
        let head_font =
            egui::FontId::proportional(look::font_size(&ctx, tokens.font.size_caption_plus));
        let header_height = ctx.fonts_mut(|fonts| fonts.row_height(&head_font))
            + 2.0 * tokens.space.table_head_padding[0];
        let line = ui.visuals().widgets.noninteractive.bg_stroke;
        let [平台宽, 变体宽, 容量宽, 年份宽, 元数据宽] = tokens.layout.table_columns;

        ui.scope(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            // 表头与行底下那几条分隔线**只横贯这张表**：量的是摆表之前这一栏的宽，不拿行的外框——
            // 那几条线画在行所在的那一层上，外框一宽出去，线就画到右边那一栏上头了（第二段第三趟截图）。
            let 表宽 = ui.available_rect_before_wrap().x_range();
            let mut builder = TableBuilder::new(ui)
                // **不画斑马纹，行与行之间一条分隔线**（设计稿 `.wtbl td` 的 `border-bottom`）。
                .striped(false)
                // **列宽不给拖**：拖得动的列 egui_extras 在每两列之间常画一道竖线，稿上没有。
                .resizable(false)
                .sense(egui::Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                // 列宽给定值而不是 `Column::auto()`：自动列宽是按**当前可见的那几行**量出来的，
                // 滚动时可见行一直在换，列宽就会随滚动跳。
                //
                // **宽度照稿**（`prototype.html` 的 `.wtbl` 表头，票 `gui-looks-like-the-design/09`）：
                // 作品那一列吃剩下的，其余几列照令牌 `check-column` 与 `table-columns`。从前作品那一列
                // 定死 320、元数据吃剩下的，左右两栏都摊开时 1280 宽的窗口里年份与元数据两列被挤出视口。
                .column(Column::exact(tokens.layout.check_column))
                .column(Column::remainder().at_least(140.0).clip(true))
                .column(Column::exact(平台宽).clip(true))
                .column(Column::exact(变体宽).clip(true))
                .column(Column::exact(容量宽).clip(true))
                .column(Column::exact(年份宽).clip(true))
                .column(Column::exact(元数据宽).clip(true));
            if let Some(offset) = scroll_to {
                builder = builder.vertical_scroll_offset(offset);
            }

            builder
                .header(header_height, |mut header| {
                    let (_, 全选格) = header.col(|ui| {
                        padded(ui, spacing, check, false, |ui| {
                            // 全选那一格。**它选的是「当前这个筛选」**，不是屏上看得见的那几行。
                            let mut all = picked.is_all();
                            if ui
                                .checkbox(&mut all, "")
                                .on_hover_text(
                                    "全选当前筛选下的每一行。它记的是这个筛选本身，\
                                     不是一万行的身份——换了筛选就作废。",
                                )
                                .changed()
                            {
                                if all {
                                    picked.select_all();
                                } else {
                                    picked.clear();
                                }
                            }
                        });
                    });
                    let mut 表头 = 全选格.rect;
                    // **默认那一种排法不画箭头**（照稿；拿主意的人 2026-09-14 定）：人点过表头、
                    // 换了排法才出箭头。
                    let 默认 = WorkQuery::default();
                    let 照默认排 = sorted_by == 默认.order && descending == 默认.descending;
                    for order in WorkOrder::ALL {
                        let (_, 这一格) = header.col(|ui| {
                            let active = sorted_by == order;
                            let 箭头 = (active && !照默认排).then_some(descending);
                            if sort_header(ui, spacing, order, 箭头).clicked() {
                                query.order = order;
                                // 再点一次同一列就翻方向。
                                query.descending = active && !descending;
                            }
                        });
                        表头 = 表头.union(这一格.rect);
                    }
                    let (_, 这一格) = header.col(|ui| {
                        padded(ui, spacing, [cell_x, cell_x], false, |ui| {
                            // **元数据那一列排不了序**，所以它不是个可点的表头：点了没反应
                            // 比灰着更糟。
                            ui.add(egui::Label::new(head_text(ui, "元数据")).selectable(false))
                                .on_hover_text(
                                    "这一行的元数据齐不齐、认没认出来（核心库给的短标签），\
                                     颜色是这一行最高的那档置信度。这一列排不了序。",
                                );
                        });
                    });
                    表头 = 表头.union(这一格.rect);
                    // 表头底下一条分隔线，横贯整张表（设计稿 `.tbl th` 的 `border-bottom`）。
                    egui::Painter::new(
                        ctx.clone(),
                        这一格.layer_id,
                        egui::Rect::from_x_y_ranges(表宽, 表头.expand(line.width).y_range()),
                    )
                    .hline(表宽, 表头.bottom() - line.width / 2.0, line);
                })
                .body(|body| {
                    body.rows(height, total, |mut row| {
                        let index = row.index() as u64;
                        row.set_selected(*focused == Some(index));
                        let Some(work) = window.row(catalog, index) else {
                            // 读不到就留空行：滚动条的长度已经由总数定死，
                            // 这里少画一行不会让下面的行位移。
                            for _ in 0..7 {
                                row.col(|_ui| {});
                            }
                            return;
                        };
                        // 焦点那一圈与行底下那条线要夹在滚动视口里，而只有格子里头拿得到那个裁剪矩形
                        // （勾选那一列不裁，它的裁剪矩形就是整个视口）。
                        let mut 看得见的 = egui::Rect::NOTHING;
                        row.col(|ui| {
                            看得见的 = ui.clip_rect();
                            padded(ui, spacing, check, false, |ui| {
                                let mut on = picked.contains(&work.anchor);
                                if ui.checkbox(&mut on, "").changed() {
                                    picked.toggle(&work.anchor);
                                }
                            });
                        });
                        row.col(|ui| {
                            // 行首那一道**置信度色条**：贴着这一格的左沿、与一行一样高（设计稿
                            // `td.st` 的 `box-shadow:inset 3px 0 0`），颜色与元数据那一枚标签同出一处。
                            let 格 = ui.max_rect();
                            ui.painter().rect_filled(
                                egui::Rect::from_min_size(
                                    格.min,
                                    egui::vec2(tokens.layout.tier_bar, 格.height()),
                                ),
                                0.0,
                                look::tier_color(work.tier(), ui.visuals()),
                            );
                            padded(ui, spacing, [cell_x, cell_x], false, |ui| {
                                name_cell(ui, work, rules, shelf.as_deref_mut());
                            });
                        });
                        row.col(|ui| {
                            padded(ui, spacing, [cell_x, cell_x], false, |ui| {
                                // **平台是个集合**：一部作品可以横跨好几个平台。
                                ui.label(work.platforms.join(" / "));
                            });
                        });
                        // 变体数与容量靠右，一列扫下来位数对得齐（设计稿 `td.r`）。变体数用等宽；
                        // **容量用常规体**，照稿——等宽的「256.00 MiB」在这一列里放不下
                        // （拿主意的人 2026-09-14 定，`font::mono` 那一条写着这个例外）。
                        row.col(|ui| {
                            padded(ui, spacing, [cell_x, cell_x], true, |ui| {
                                ui.label(font::mono(thousands(work.variants)));
                            });
                        });
                        row.col(|ui| {
                            padded(ui, spacing, [cell_x, cell_x], true, |ui| {
                                ui.label(capacity(work.bytes, work.unreadable_files));
                            });
                        });
                        row.col(|ui| {
                            padded(ui, spacing, [cell_x, cell_x], false, |ui| {
                                ui.label(work.year.as_deref().unwrap_or("—"));
                            });
                        });
                        row.col(|ui| {
                            padded(ui, spacing, [cell_x, cell_x], false, |ui| {
                                metadata_chip(ui, work);
                            });
                        });
                        let response = row.response();
                        // 每一行底下一条分隔线，夹在滚动视口里。
                        let 这一行 = response.rect;
                        egui::Painter::new(
                            ctx.clone(),
                            response.layer_id,
                            egui::Rect::from_x_y_ranges(表宽, 看得见的.y_range()),
                        )
                        .hline(
                            表宽,
                            这一行.bottom() - line.width / 2.0,
                            line,
                        );
                        // **焦点落在这一行上要看得见**：行是点得中的，于是 Tab 走得到它
                        // （票 `gui-redesign/12` 验收第 7 条）。行自己画底色，走不了 egui
                        // 按钮那条路，得自己描一圈。
                        look::focus_ring(&ctx, 看得见的, &response);
                        if response.clicked() {
                            *focused = Some(index);
                            // **交一份拷贝出去而不是下标**：详情面板要在这一行滚出视口
                            // 之后照样摆得出来。只有真点中的那一帧才复制。
                            opened = Some(work.clone());
                        }
                    });
                });
        });
        opened
    }
}

/// 表里一格的**内容区**：左右各让出 `[左, 右]`，间距换回这一屏平常那一份，靠左或靠右摆、上下居中。
///
/// 整张表的格与格之间一点缝都不留（[`Table::show`]），留白由每一格自己让。
fn padded<R>(
    ui: &mut egui::Ui,
    spacing: egui::Vec2,
    [left, right]: [f32; 2],
    right_aligned: bool,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let mut rect = ui.max_rect();
    rect.min.x = (rect.min.x + left).min(rect.max.x);
    rect.max.x = (rect.max.x - right).max(rect.min.x);
    let layout = if right_aligned {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    };
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(layout), |ui| {
        ui.spacing_mut().item_spacing = spacing;
        // 选中那一行 egui_extras 把整行的字换成强调色；照稿选中只换底色（`accent-soft`），字照旧。
        ui.visuals_mut().override_text_color = None;
        add(ui)
    })
    .inner
}

/// 表头一格：列名后面跟着排序的小箭头（按这一列排、又不是默认那一种排法时才有）。**整格点得中**：点一下按这一列排，
/// 再点一下翻方向。变体与容量两列靠右，与底下的数对齐（设计稿 `th.r`）；悬停时列名换成强调字。
fn sort_header(
    ui: &mut egui::Ui,
    spacing: egui::Vec2,
    order: WorkOrder,
    direction: Option<bool>,
) -> egui::Response {
    let tokens = Tokens::builtin();
    let cell_x = tokens.space.table_cell_padding;
    let response = ui.interact(ui.max_rect(), ui.id().with("排序"), egui::Sense::click());
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, order.label()));
    let 靠右 = matches!(order, WorkOrder::Variants | WorkOrder::Bytes);
    let 悬停 = response.hovered();
    padded(ui, spacing, [cell_x, cell_x], 靠右, |ui| {
        ui.spacing_mut().item_spacing.x = tokens.space.sort_arrow_gap;
        let mut 列名 = head_text(ui, order.label());
        if 悬停 {
            列名 = 列名.color(ui.visuals().strong_text_color());
        }
        let 箭头 = direction.map(|down| {
            egui::Label::new(
                egui::RichText::new(if down { "▼" } else { "▲" })
                    .size(tokens.font.size_arrow)
                    .color(ui.visuals().hyperlink_color),
            )
            .selectable(false)
        });
        let 列名 = egui::Label::new(列名).selectable(false);
        // 靠右的那两列从右往左摆：先摆的在最右，所以箭头先摆。
        if 靠右 {
            if let Some(箭头) = 箭头 {
                ui.add(箭头);
            }
            ui.add(列名);
        } else {
            ui.add(列名);
            if let Some(箭头) = 箭头 {
                ui.add(箭头);
            }
        }
    });
    response
}

/// 表头上的字：表头字号（令牌 `size-caption-plus`）、弱字色，拉丁与数字加粗（设计稿 `.tbl th`）。
fn head_text(ui: &egui::Ui, text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(look::font_size(
            ui.ctx(),
            Tokens::builtin().font.size_caption_plus,
        ))
        .family(font::strong_family())
        .color(ui.visuals().weak_text_color())
}

/// 「元数据」那一格：照稿一枚标签（[`look::chip`]）。字是核心库给的短标签（[`WorkRow::meta_label`]：
/// 完整 / 缺某一项 / 缺 N 项 / 缺全部 / 仅文件名 / 待确认 / 还没识别），颜色是这一行那一档置信度
/// （[`WorkRow::tier`] → [`look::tier_tone`]）。这儿一个 `match` 都不写。
///
/// **置信度的词不在表上**：那一档由行首色条与右栏变体卡片上的标签说——票 `gui-redesign/12` 那条
/// 「上了色的地方都跟着那个词」，2026-09-14 拿主意的人看图后撤掉，照稿（挂单 `Q872`）。
fn metadata_chip(ui: &mut egui::Ui, work: &WorkRow) {
    look::chip(ui, look::tier_tone(work.tier()), &work.meta_label());
}

/// 根名与相对路径之间那个分隔：认不出作品那一行的副行、侧边详情变体卡片底下那一行都照稿写
/// 「根名 · 相对路径」（设计稿 `主库 · GBC/汉化/…`，挂单 `Q809`）。
pub const ROOT_SEPARATOR: &str = " · ";

/// 一条键画成「**根名 · 相对路径**」，宽不过 `max_width`：画不下时只从相对路径的左边删字补「…」，
/// 根名留着——一眼看得出是哪个根。**拆键由核心库做**（[`romcat_core::path::split_root`]），
/// 这里只接起来、量宽度。
///
/// **一个例外**（拿主意的人 2026-09-14 定，挂单 `Q809`）：写上根名之后，留得下的那一截连**文件名**
/// （相对路径的最后一段）都摆不全，就省掉根名，只写「…」接相对路径的尾巴——根名那一截宽让给文件名。
/// 表格里带「未关联作品」标签的那几行，1280 宽的窗口里扣掉标签只剩六十来点，带着根名只留得下一个字。
/// 判的是**量出来的宽度**，不数字数；省掉根名时开头一律是「…」，人看得出前头还有东西。
#[must_use]
pub(crate) fn root_and_path(
    ui: &egui::Ui,
    key: &str,
    font: &egui::FontId,
    max_width: f32,
) -> String {
    let (root, relative) = romcat_core::path::split_root(key);
    if relative.is_empty() {
        return tail_fit(ui, root, font, max_width);
    }
    let head = format!("{root}{ROOT_SEPARATOR}");
    let room = max_width - text_width(ui, &head, font);
    let with_root = tail_fit(ui, relative, font, room.max(0.0));
    let file_name = std::path::Path::new(relative)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(relative);
    // 带着根名时留下的那一截：整条相对路径画得下，或者截过之后（去掉开头那个「…」）还装得下整个文件名。
    let kept = with_root.strip_prefix('…').unwrap_or(&with_root);
    if with_root == relative || kept.len() >= file_name.len() {
        return format!("{head}{with_root}");
    }
    let marked = format!("…{relative}");
    if text_width(ui, &marked, font) <= max_width {
        marked
    } else {
        tail_fit(ui, relative, font, max_width)
    }
}

/// 「作品」那一格。
///
/// **认不出作品的那一行两行字**（票 `gui-looks-like-the-design/09`）：主栏是**正题**，单独占一行；
/// 副行开头是「未关联作品」标签，跟着那份内容在主库里的「**根名 · 相对路径**」，**从尾部截断**
/// （[`root_and_path`]）。标签挪到第二行是拿主意的人 2026-09-14 定的（挂单 `Q878`）：左边多了导航之后，
/// 1280 宽的窗口里作品那一列只剩一百六十来点，标签跟在正题后头时正题被挤得只剩一个「…」。从前那一格直接印变体的键——真库里一万六千多行都是一长串路径，
/// 而路径的信息在尾巴上，被列宽截掉的正是文件名那一截。认不认得出、正题是什么，都是核心库答的
/// （[`WorkRow::title`]）。
///
/// **认出作品、带着显示标题的那一行也两行字**：主栏是显示标题（中文名多半在这儿），副行小字是作品名
/// （原名）。显示标题由核心库挑（[`WorkRow::display`]）；取不到时主栏印作品名，第二行不写。
///
/// 「在每行开头显示封面」开着时（`shelf` 是 `Some`），这一格最左边先摆一小格封面或平台色块。
fn name_cell(ui: &mut egui::Ui, work: &WorkRow, rules: &Rules, shelf: Option<&mut Shelf>) {
    let tokens = Tokens::builtin();
    if let Some(shelf) = shelf {
        shelf.thumb(ui, work);
        // 封面格与字之间（设计稿 `.wcell` 的 `gap`）。
        ui.add_space((tokens.space.cell_gap - ui.spacing().item_spacing.x).max(0.0));
    }
    // **搜索命中在别处时说清楚**：一行名字里一个搜索词都没有的
    // 作品冒在前面，不印这一句就是「凭什么排在这儿」看不出答案。
    // 标题自己命中的不印——那一眼就看得见，多一个记号只是噪音。
    //
    // **非游戏资产也标在右头**（票 `gui-looks-like-the-design/08`）：
    // 是不是由核心库答（`WorkRow::non_game_asset`），这里照着标，
    // 不自己判（ADR-0024）。
    let hit = work.hit.filter(|hit| *hit > SearchHit::Title);
    if let Some(title) = work.title(rules) {
        two_lines(ui, work, hit, &title, Some(UNLINKED_LABEL), Second::Path);
        return;
    }
    if let Some(display) = work.display.as_deref() {
        two_lines(ui, work, hit, display, None, Second::WorkName);
        return;
    }
    // 取不到显示标题的那一行一行字：作品名，拉丁与数字加粗（设计稿 `.w1`）。
    if hit.is_none() && !work.non_game_asset {
        ui.add(egui::Label::new(font::strong(&work.name)).truncate());
    } else {
        // **先把那句话摆到这一格的右头，剩下的宽度才给名字。**
        // 这一列是定宽加 `clip`，而真库里 DAT 条目名普遍长——
        // 顺着写的话被截掉的正是那句唯一的答案。反过来摆，
        // 截掉的是名字，而名字还挂在悬停里。
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            right_marks(ui, work, hit);
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.add(egui::Label::new(font::strong(&work.name)).truncate())
                    .on_hover_text(&work.name);
            });
        });
    }
}

/// 两行字那一格的第二行印什么。
#[derive(Debug, Clone, Copy)]
enum Second {
    /// 认不出作品的那一行：「根名 · 相对路径」，画不下只删相对路径的左边（[`root_and_path`]）。
    Path,
    /// 带着显示标题的那一行：作品名（原名），画不下截尾巴。
    WorkName,
}

/// 两行字的那一格（设计稿 `.wtxt`）：主栏拉丁与数字加粗，右头照旧挂着搜索命中与非游戏资产那两个
/// 记号；底下一行小字，等宽、弱字色（设计稿 `.w2`），整条挂在悬停里。`label` 给了就摆在**第二行开头**，
/// 主栏整行都给正题（挂单 `Q878`）。
fn two_lines(
    ui: &mut egui::Ui,
    work: &WorkRow,
    hit: Option<SearchHit>,
    main: &str,
    label: Option<&str>,
    second: Second,
) {
    let tokens = Tokens::builtin();
    let width = ui.available_width();
    let small = egui::FontId::monospace(tokens.font.size_path);
    ui.vertical(|ui| {
        // 两行字之间不另留缝（设计稿 `.wtxt`）。
        ui.spacing_mut().item_spacing.y = 0.0;
        ui.allocate_ui_with_layout(
            egui::vec2(width, tokens.layout.tag_height),
            Layout::right_to_left(Align::Center),
            |ui| {
                right_marks(ui, work, hit);
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(egui::Label::new(font::strong(main)).truncate());
                });
            },
        );
        let weak = ui.visuals().weak_text_color();
        match second {
            Second::Path => {
                ui.allocate_ui_with_layout(
                    egui::vec2(width, tokens.layout.tag_height),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        // **标签的宽度先让出来，剩下的才给路径**：路径长了截的是路径的左边，
                        // 「未关联作品」那几个字总在。
                        let mut room = width;
                        if let Some(text) = label {
                            tag(ui, text);
                            room -= tag_width(ui, text) + ui.spacing().item_spacing.x;
                        }
                        // **路径从尾部截断**：截到画得下为止，文件名那一截留着。
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(root_and_path(
                                    ui,
                                    &work.name,
                                    &small,
                                    room.max(0.0),
                                ))
                                .font(small)
                                .color(weak),
                            )
                            .extend(),
                        )
                        .on_hover_text(&work.name);
                    },
                );
            }
            Second::WorkName => {
                ui.add(
                    egui::Label::new(egui::RichText::new(&work.name).font(small).color(weak))
                        .truncate(),
                )
                .on_hover_text(&work.name);
            }
        }
    });
}

/// 这一格右头那两个记号：命中在哪儿、是不是非游戏资产。**右往左摆**，先摆的在最右。
fn right_marks(ui: &mut egui::Ui, work: &WorkRow, hit: Option<SearchHit>) {
    if let Some(hit) = hit {
        ui.weak(hit.label());
    }
    if work.non_game_asset {
        ui.weak(NON_GAME_ASSET_LABEL);
    }
}

/// 一段字**从左边删字**、补一个「…」，删到量出来的宽度摆得进 `max_width` 为止；
/// 本来就摆得下就原样交回。
///
/// 给路径用：路径的信息在尾巴上（文件名），从右边截掉的正是人要认的那一截。
/// **宽度是拿这个字体真量出来的**，不是数字数——一个汉字比一个拉丁字母宽一倍，
/// 按字数截要么截多了、要么画出格。
///
/// 删几个字是二分找的：删得越多越窄，找「删最少、摆得下」的那一处，一格量十几次。
///
/// 侧边详情里变体那一行、文件那一行印的也是这串键，用的是同一个。
#[must_use]
pub(crate) fn tail_fit(ui: &egui::Ui, text: &str, font: &egui::FontId, max_width: f32) -> String {
    let width = |candidate: String| text_width(ui, &candidate, font);
    if width(text.to_string()) <= max_width {
        return text.to_string();
    }
    // 删掉头 `n + 1` 个字之后，剩下那一截从哪个字节起；最后一格是「全删了」。
    let starts: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .skip(1)
        .chain(std::iter::once(text.len()))
        .collect();
    let fits = |n: usize| width(format!("…{}", &text[starts[n]..])) <= max_width;
    let (mut low, mut high) = (0, starts.len() - 1);
    while low < high {
        let middle = (low + high) / 2;
        if fits(middle) {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    format!("…{}", &text[starts[low]..])
}

/// 这段字用这个字体排成一行**有多宽**（真量出来的，不是数字数）。
#[must_use]
pub(crate) fn text_width(ui: &egui::Ui, text: &str, font: &egui::FontId) -> f32 {
    ui.ctx().fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(text.to_owned(), font.clone(), egui::Color32::PLACEHOLDER)
            .size()
            .x
    })
}

/// 一枚行内**标签**（[`tag`]）画出来多宽：字宽加左右各一份令牌 `tag-padding`。要先替它留出地方时用。
#[must_use]
pub(crate) fn tag_width(ui: &egui::Ui, text: &str) -> f32 {
    let tokens = Tokens::builtin();
    text_width(
        ui,
        text,
        &egui::FontId::proportional(look::font_size(ui.ctx(), tokens.font.size_caption_plus)),
    ) + 2.0 * tokens.layout.tag_padding
}

/// 画一枚行内**标签**，照稿 `.tag`：凹陷底（令牌 `sunken`）、小圆角、正文次一级的字色（`ink-2`），
/// 高取 `tag-height`、左右留白取 `tag-padding`、字是 `size-caption-plus`。
///
/// 表上认不出作品那一行的「未关联作品」、侧边详情变体卡片上的「首选变体」「非游戏资产」都是它。
pub(crate) fn tag(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let tokens = Tokens::builtin();
    let color = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(
        text.to_owned(),
        egui::FontId::proportional(look::font_size(ui.ctx(), tokens.font.size_caption_plus)),
        color,
    );
    let size = egui::vec2(
        galley.size().x + 2.0 * tokens.layout.tag_padding,
        tokens.layout.tag_height,
    );
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, tokens.radius.small, ui.visuals().extreme_bg_color);
    painter.galley(rect.center() - galley.size() / 2.0, galley, color);
    response
}
