//! 十万行的变体表，以及它背后那扇**窗**。
//!
//! ## 两层各管一件事
//!
//! - [`Window`] 管**内存里装多少**。它只留当前视口附近的一段，行数由 [`Window::span`]
//!   定死，与库里有多少变体无关。要哪一行就问它，不在窗里就去中立库取一段回来。
//! - [`Table`] 管**画出来什么**。`egui_extras::TableBuilder` 的 `body.rows()` 只调用
//!   视口里那几十行的闭包，开销与总行数无关（官方 demo 的行数滑杆上限就是 100,000）。
//!
//! 两层合起来是这张票的第二条验收：**排序、筛选、分页在中立库那一层完成，内存占用与
//! 总行数无关**。排序尤其要说一句——`egui_extras` 完全没有排序能力（源码里 `sort`
//! 零出现），于是它只能由别人做；做在 `ORDER BY` 里而不是内存的 `Vec` 上，正是因为
//! 后者要求先把全库读进来。
//!
//! ## 行高等高，绝不用 `heterogeneous_rows`
//!
//! 后者的开销是 O(总行数)——它要把每一行的高度都过一遍才知道视口从哪开始。
//! 等高行下 `rows()` 是一次除法。

use egui::{Align, Layout};
use egui_extras::{Column, TableBuilder};
use romcat_core::catalog::browse::VariantOrder;
use romcat_core::catalog::{Catalog, VariantQuery, VariantRow};
use romcat_core::report::capacity;

/// 一行多高，点。
///
/// 十万行乘以它约 2.1×10⁶ 点，而 `f32` 在那个量级上的最小间隔是 0.25 点——距离滚动抖动
/// 的阈值还有一个数量级以上的余量（egui#1391）。
pub const ROW_HEIGHT: f32 = 21.0;

/// 窗口默认一次取多少行。
///
/// 视口撑死几十行，取 512 是给上下滚动留预取余量：往下翻过 3/4 个窗口才需要再查一次库。
/// 它同时是**内存占用的上界**——不管库里是四万行还是一百万行，这里只有 512 个
/// [`VariantRow`]。
pub const SPAN: u64 = 512;

/// 表格背后那扇窗：内存里只装当前视口那一段。
///
/// 换排序或换筛选**不是**在窗里重排，是把窗整个作废，重新问中立库要——那才是
/// 「排序下推」这句话的实际含义。
pub struct Window {
    query: VariantQuery,
    /// 满足筛选条件的总行数。滚动条的长度由它来，与窗里装了几行无关。
    total: u64,
    /// 窗里第一行在全序里的下标。
    first: u64,
    rows: Vec<VariantRow>,
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
}

impl Window {
    /// 开一扇窗，一次取 `span` 行。`span` 会被夹进 `[1, MAX_PAGE]`。
    #[must_use]
    pub fn new(span: u64) -> Self {
        Self {
            query: VariantQuery::default(),
            total: 0,
            first: 0,
            rows: Vec::new(),
            span: span.clamp(1, romcat_core::catalog::MAX_PAGE),
            reads: 0,
            error: None,
            stale: true,
        }
    }

    /// 当前的筛选与排序。
    #[must_use]
    pub fn query(&self) -> &VariantQuery {
        &self.query
    }

    /// 换一套筛选与排序。和现在这套一样就什么都不做——界面每帧都会调它。
    pub fn set_query(&mut self, query: VariantQuery) {
        if self.query != query {
            self.query = query;
            self.rows.clear();
            self.first = 0;
            // 换了条件就重新试一次：上一次读不动的可能正是这条件本身。
            self.error = None;
            self.stale = true;
        }
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
        match catalog.variant_total(&self.query) {
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
    pub fn row(&mut self, catalog: &Catalog, index: u64) -> Option<&VariantRow> {
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
        match catalog.variant_page(&self.query, first, self.span) {
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

/// 一张变体表。
///
/// 它**直接改 `query`**：点表头就是换排序，而排序是中立库那一层的事，界面这边只是把
/// 用户点的那一下写进查询里。多存一份「点了哪个表头」的状态只会与查询漂开。
pub struct Table<'a> {
    /// 数据从哪来。
    pub catalog: &'a Catalog,
    /// 作品 id → 作品名。
    ///
    /// **翻库时人认的是作品，不是文件名**——库里那些名字是各路来源攒出来的，
    /// 有的还是乱码（票 11）。所以表里多一列作品名。
    ///
    /// 它是**整份带进来的一张小表**（真库 9,226 个作品，几百 KiB），不是每行查一次库：
    /// 那样会把「一行一次查询」重新引回这张四万行的表上。代价是这一列**排不了序、
    /// 也筛不了**——排序与筛选一律下推到 `ORDER BY`/`WHERE`，而这一列不在那儿（挂账 D160）。
    pub works: &'a std::collections::BTreeMap<i64, String>,
    /// 窗口。
    pub window: &'a mut Window,
    /// 筛选与排序。**界面那一份的引用，不是副本。**
    pub query: &'a mut VariantQuery,
    /// 选中的是哪一行（全序下标）。
    pub selected: &'a mut Option<u64>,
    /// 把滚动位置强按到这个像素偏移。**只有量帧率时才用**，界面上是 `None`。
    pub scroll_to: Option<f32>,
}

impl Table<'_> {
    /// 画出来，返回这一帧里被点选的那一行。
    ///
    /// 返回的是**一份拷贝**而不是下标：详情面板要在那一行滚出视口之后照样显示得出来。
    pub fn show(self, ui: &mut egui::Ui) -> Option<VariantRow> {
        let Self {
            catalog,
            works,
            window,
            query,
            selected,
            scroll_to,
        } = self;
        let mut picked = None;
        let total = usize::try_from(window.total()).unwrap_or(usize::MAX);
        let (sorted_by, descending) = (query.order, query.descending);

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            // 列宽给定值而不是 `Column::auto()`：自动列宽是按**当前可见的那几行**量出来的，
            // 滚动时可见行一直在换，列宽就会随滚动跳。
            .column(Column::initial(300.0).at_least(140.0).clip(true))
            .column(Column::initial(180.0).at_least(90.0).clip(true))
            .column(Column::initial(90.0).at_least(60.0).clip(true))
            .column(Column::initial(110.0).at_least(70.0).clip(true))
            .column(Column::initial(70.0).at_least(50.0))
            .column(Column::remainder().at_least(90.0));
        if let Some(offset) = scroll_to {
            builder = builder.vertical_scroll_offset(offset);
        }

        builder
            .header(24.0, |mut header| {
                for order in VariantOrder::ALL {
                    header.col(|ui| {
                        let active = sorted_by == order;
                        let mark = match (active, descending) {
                            (false, _) => "",
                            (true, true) => " ▼",
                            (true, false) => " ▲",
                        };
                        if ui
                            .selectable_label(active, format!("{}{mark}", order.label()))
                            .clicked()
                        {
                            query.order = order;
                            // 再点一次同一列就翻方向。
                            query.descending = active && !descending;
                        }
                    });
                    // **作品名那一列排不了序**，所以它不是个可点的表头：点了没反应
                    // 比灰着更糟。它插在「变体」右边——那是人扫这张表时先看的位置。
                    if order == VariantOrder::Key {
                        header.col(|ui| {
                            ui.label("作品").on_hover_text(
                                "识别认出来的那个作品。**这一列排不了序也筛不了**——\
                                 排序与筛选一律下推到中立库，而它是另一张表上的名字。",
                            );
                        });
                    }
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, total, |mut row| {
                    let index = row.index() as u64;
                    row.set_selected(*selected == Some(index));
                    let Some(variant) = window.row(catalog, index) else {
                        // 读不到就留空行：滚动条的长度已经由总数定死，
                        // 这里少画一行不会让下面的行位移。
                        for _ in 0..6 {
                            row.col(|_ui| {});
                        }
                        return;
                    };
                    row.col(|ui| {
                        ui.label(&variant.key);
                    });
                    row.col(|ui| {
                        ui.label(
                            variant
                                .work_id
                                .and_then(|id| works.get(&id))
                                .map_or("—", String::as_str),
                        );
                    });
                    row.col(|ui| {
                        ui.label(variant.platform.as_deref().unwrap_or("—"));
                    });
                    row.col(|ui| {
                        ui.label(&variant.rule);
                    });
                    row.col(|ui| {
                        ui.label(variant.files.to_string());
                    });
                    row.col(|ui| {
                        ui.label(capacity(variant.bytes, variant.unreadable_files));
                    });
                    if row.response().clicked() {
                        *selected = Some(index);
                        picked = Some(variant.clone());
                    }
                });
            });
        picked
    }
}
