//! 十万行的变体表，以及它背后那扇**窗**。
//!
//! ## 两层各管一件事
//!
//! - [`Window`] 管**内存里装多少**。它只留当前视口附近的一段，行数由 [`Window::span`]
//!   定死，与库里有多少变体无关。要哪一行就问它，不在窗里就去中立库取一段回来。
//! - [`Table`] 管**画出来什么**。`egui_extras::TableBuilder` 的 `body.rows()` 只调用
//!   视口里那几十行的闭包，开销与总行数无关（官方 demo 的行数滑杆上限就是 100,000）。
//!
//! 两层合起来是这张票的第二条验收：**排序、筛选、分页在数据库层完成，内存占用与总行数
//! 无关**。排序尤其要说一句——`egui_extras` 完全没有排序能力（源码里 `sort` 零出现），
//! 于是它只能由别人做；做在 `ORDER BY` 里而不是内存的 `Vec` 上，正是因为后者要求先把
//! 全库读进来。
//!
//! ## 行高等高，绝不用 `heterogeneous_rows`
//!
//! 后者的开销是 O(总行数)——它要把每一行的高度都过一遍才知道视口从哪开始。
//! 等高行下 `rows()` 是一次除法。

use egui::{Align, Layout};
use egui_extras::{Column, TableBuilder};
use romcat_core::catalog::browse::VariantOrder;
use romcat_core::catalog::{Catalog, VariantQuery, VariantRow};

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
    /// 上一次读库的错误；`None` 表示一切正常。界面把它显示在状态栏而不是弹窗——
    /// 表格每帧都在读，弹窗会弹到关不掉。
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
    /// 越界或读库失败都返回 `None`——表格照样画得下去，只是那一行是空的，
    /// 出错的原因在 [`Window::error`] 里。
    pub fn row(&mut self, catalog: &Catalog, index: u64) -> Option<&VariantRow> {
        if index >= self.total {
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

/// 表头点了一下的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sorted {
    /// 按哪一列排。
    pub order: VariantOrder,
    /// 倒着排。
    pub descending: bool,
}

/// 画一张变体表，返回这一帧里被点选的那一行（若有）。
pub struct Table<'a> {
    /// 数据从哪来。
    pub catalog: &'a Catalog,
    /// 窗口。
    pub window: &'a mut Window,
    /// 选中的是哪一行（全序下标）。
    pub selected: &'a mut Option<u64>,
    /// 把滚动位置强按到这个像素偏移。**只有量帧率时才用**，界面上是 `None`。
    pub scroll_to: Option<f32>,
}

/// 一帧画下来发生了什么。
#[derive(Debug, Default)]
pub struct Painted {
    /// 表头被点了，要换排序。
    pub sort: Option<Sorted>,
    /// 有一行被点选了，附它的内容——**拷一份**而不是留个下标：详情面板要在那一行滚出
    /// 视口之后照样显示得出来。
    pub picked: Option<VariantRow>,
}

impl Table<'_> {
    /// 画出来。
    pub fn show(&mut self, ui: &mut egui::Ui) -> Painted {
        let mut painted = Painted::default();
        let total = usize::try_from(self.window.total()).unwrap_or(usize::MAX);
        let current = self.window.query().clone();

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            // 列宽给定值而不是 `Column::auto()`：自动列宽是按**当前可见的那几行**量出来的，
            // 滚动时可见行一直在换，列宽就会随滚动跳。
            .column(Column::initial(360.0).at_least(160.0).clip(true))
            .column(Column::initial(90.0).at_least(60.0).clip(true))
            .column(Column::initial(110.0).at_least(70.0).clip(true))
            .column(Column::initial(70.0).at_least(50.0))
            .column(Column::remainder().at_least(90.0));
        if let Some(offset) = self.scroll_to {
            builder = builder.vertical_scroll_offset(offset);
        }

        builder
            .header(24.0, |mut header| {
                for order in VariantOrder::ALL {
                    header.col(|ui| {
                        let mark = if current.order != order {
                            ""
                        } else if current.descending {
                            " ▼"
                        } else {
                            " ▲"
                        };
                        if ui
                            .selectable_label(
                                current.order == order,
                                format!("{}{mark}", order.label()),
                            )
                            .clicked()
                        {
                            painted.sort = Some(Sorted {
                                order,
                                // 再点一次同一列就翻方向。
                                descending: current.order == order && !current.descending,
                            });
                        }
                    });
                }
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, total, |mut row| {
                    let index = row.index() as u64;
                    row.set_selected(*self.selected == Some(index));
                    let Some(variant) = self.window.row(self.catalog, index) else {
                        // 读不到就留空行：滚动条的长度已经由总数定死，
                        // 这里少画一行不会让下面的行位移。
                        for _ in 0..5 {
                            row.col(|_ui| {});
                        }
                        return;
                    };
                    let picked = variant.clone();
                    row.col(|ui| {
                        ui.label(&picked.key);
                    });
                    row.col(|ui| {
                        ui.label(picked.platform.as_deref().unwrap_or("—"));
                    });
                    row.col(|ui| {
                        ui.label(&picked.rule);
                    });
                    row.col(|ui| {
                        ui.label(picked.files.to_string());
                    });
                    row.col(|ui| {
                        ui.label(bytes_text(picked.bytes));
                    });
                    if row.response().clicked() {
                        *self.selected = Some(index);
                        painted.picked = Some(picked);
                    }
                });
            });
        painted
    }
}

/// 把字节数说成人看得懂的样子。
#[must_use]
pub fn bytes_text(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit + 1 < UNITS.len() {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}
