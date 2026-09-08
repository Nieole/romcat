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
use romcat_core::catalog::browse::{Scope, SearchHit, WorkAnchor, WorkOrder, WorkQuery, WorkRow};
use romcat_core::report::{capacity, thousands};

use crate::look;

/// 一行多高，点。
///
/// 十万行乘以它约 2.1×10⁶ 点，而 `f32` 在那个量级上的最小间隔是 0.25 点——距离滚动抖动
/// 的阈值还有一个数量级以上的余量（egui#1391）。
pub const ROW_HEIGHT: f32 = 21.0;

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
        match catalog.work_page(&self.query, first, self.span) {
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
        } = self;
        let mut opened = None;
        // 行画完之后手上没有那一行的 `Ui` 了（列都加完才拿得到 `response`），
        // 而焦点那一圈要画在那时——先把上下文留一份。
        let ctx = ui.ctx().clone();
        let total_rows = window.total();
        let total = usize::try_from(total_rows).unwrap_or(usize::MAX);
        let (sorted_by, descending) = (query.order, query.descending);

        let mut builder = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(Layout::left_to_right(Align::Center))
            // 列宽给定值而不是 `Column::auto()`：自动列宽是按**当前可见的那几行**量出来的，
            // 滚动时可见行一直在换，列宽就会随滚动跳。
            .column(Column::initial(30.0).at_least(26.0))
            .column(Column::initial(320.0).at_least(140.0).clip(true))
            .column(Column::initial(150.0).at_least(80.0).clip(true))
            .column(Column::initial(60.0).at_least(50.0))
            .column(Column::initial(110.0).at_least(70.0).clip(true))
            .column(Column::initial(64.0).at_least(50.0))
            .column(Column::remainder().at_least(110.0));
        if let Some(offset) = scroll_to {
            builder = builder.vertical_scroll_offset(offset);
        }

        builder
            .header(24.0, |mut header| {
                header.col(|ui| {
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
                for order in WorkOrder::ALL {
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
                }
                header.col(|ui| {
                    // **元数据那一列排不了序**，所以它不是个可点的表头：点了没反应
                    // 比灰着更糟。
                    ui.label("元数据").on_hover_text(
                        "这一行最高的那档置信度（ADR-0002），加上作品这一层缺哪几样\
                         元数据。**这一列排不了序**——排序一律下推到中立库，\
                         而它是补在页上的。",
                    );
                });
            })
            .body(|body| {
                body.rows(ROW_HEIGHT, total, |mut row| {
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
                    // 焦点那一圈要夹在滚动视口里，而只有格子里头拿得到那个裁剪矩形。
                    let mut 看得见的 = egui::Rect::NOTHING;
                    row.col(|ui| {
                        看得见的 = ui.clip_rect();
                        let mut on = picked.contains(&work.anchor);
                        if ui.checkbox(&mut on, "").changed() {
                            picked.toggle(&work.anchor);
                        }
                    });
                    row.col(|ui| {
                        // **搜索命中在别处时说清楚**：一行名字里一个搜索词都没有的
                        // 作品冒在前面，不印这一句就是「凭什么排在这儿」看不出答案。
                        // 标题自己命中的不印——那一眼就看得见，多一个记号只是噪音。
                        match work.hit.filter(|hit| *hit > SearchHit::Title) {
                            None => {
                                ui.label(&work.name);
                            }
                            Some(hit) => {
                                // **先把那句话摆到这一格的右头，剩下的宽度才给名字。**
                                // 这一列是定宽加 `clip`，而真库里 DAT 条目名普遍长——
                                // 顺着写的话被截掉的正是那句唯一的答案。反过来摆，
                                // 截掉的是名字，而名字还挂在悬停里。
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.weak(hit.label());
                                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                                        ui.label(&work.name).on_hover_text(&work.name);
                                    });
                                });
                            }
                        }
                    });
                    row.col(|ui| {
                        // **平台是个集合**：一部作品可以横跨好几个平台。
                        ui.label(work.platforms.join(" / "));
                    });
                    row.col(|ui| {
                        ui.label(thousands(work.variants));
                    });
                    row.col(|ui| {
                        ui.label(capacity(work.bytes, work.unreadable_files));
                    });
                    row.col(|ui| {
                        ui.label(work.year.as_deref().unwrap_or("—"));
                    });
                    row.col(|ui| {
                        // 置信度的颜色与词在五屏里同出一处（规格 69、票 `gui-redesign/12`）：
                        // 哪一档由核心库说（`WorkRow::tier`），印哪个词也由核心库说
                        // （`WorkRow::confidence_label`——一条候选都没有时它还要分辨
                        // **没有候选**与**还没识别**，票 `gui-redesign/17`），
                        // 什么颜色由 [`crate::look`] 说，这儿一个 `match` 都不写。
                        // **词一直在**——颜色不是唯一线索。
                        ui.colored_label(
                            look::tier_color(work.tier(), ui.visuals()),
                            format!("{} · {}", work.confidence_label(), work.missing_label()),
                        );
                    });
                    // **焦点落在这一行上要看得见**：行是点得中的，于是 Tab 走得到它
                    // （票 `gui-redesign/12` 验收第 7 条）。行自己画底色，走不了 egui
                    // 按钮那条路，得自己描一圈。
                    let response = row.response();
                    look::focus_ring(&ctx, 看得见的, &response);
                    if response.clicked() {
                        *focused = Some(index);
                        // **交一份拷贝出去而不是下标**：详情面板要在这一行滚出视口
                        // 之后照样摆得出来。只有真点中的那一帧才复制。
                        opened = Some(work.clone());
                    }
                });
            });
        opened
    }
}
