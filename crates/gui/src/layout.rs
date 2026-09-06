//! **面板边界**：拖得动、记得住、挤不塌。
//!
//! 三屏上一共七条边界（[`Boundary::ALL`]），每一条都是一句声明：靠哪一边、默认多宽、
//! 最少多宽、最多占整个窗口那一维的几成。**画那一屏的代码不自己写这四个数**
//! ——写了就会有人只改一处，于是同一条边界在两个地方是两个下限。
//!
//! ## 上限两道一起夹
//!
//! `min_size` 拦得住「拖没了」，拦不住「把别人挤没」：`egui::Panel` 的上限默认是
//! **整块可用地方**，一路往里拖就能让正中那张表只剩零宽。所以每条边界的上限自己算
//! （[`Boundary::cap`]），两道一起夹：**整个窗口的几成**（让上限是个定数，先摆的不会
//! 挤后摆的），加上**「眼下还剩多少」减去 [`FLOOR`]**（兜底：不管前面吃掉多少，
//! 后面一定还剩得下这么多）。于是正中那张表在任何窗口尺寸下都留得住。
//!
//! ## 记的是「人拖到哪儿」，不是「这一帧画成多宽」
//!
//! [`Layout::harvest`] 只在**把手被松开**的那一帧记。照单全收的话，窗口一变小、面板被
//! 上限夹了一刀，那一刀就会写进文件——而窗口尺寸本身不持久化，每次开窗都要挨一遍。
//!
//! ## 位置存**工作目录**，不存中立库
//!
//! [`romcat_core::workspace::gui_layout_path`]。中立库整份可再生，界面偏好放进去会被
//! 某一次重扫抹掉（票 `gui-redesign/12` 点名的一条）。那个文件是几行
//! 「名字 = 数值」的纯文本，随手删得掉：删了就回到默认版式，库里一个字节都不动。
//!
//! ## 拖的时候不写盘
//!
//! [`Layout::flush`] 只在**手松开之后**落盘（`pointer.any_down()` 为假的那一帧），
//! 而且只在数真变了的时候写。拖一次是一次写，不是六十次。
//!
//! ## 这一层为什么不算领域逻辑
//!
//! 面板拖到哪儿是**这块屏**的事，与库里有什么无关（ADR-0005 拦的是领域判断长在界面里，
//! 不是拦界面自己的偏好）。工作目录里那条路径归核心库说了算（那是「工作目录里有什么」
//! 的唯一一份清单），怎么读写那几行字归这里。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::app::View;

/// 一条边界靠在哪一边。
///
/// **没有 `Top`**：顶栏是那排屏名，高度由内容定死，拖它没有意义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// 左栏，拖的是宽。
    Left,
    /// 右栏，拖的是宽。
    Right,
    /// 底栏，拖的是高。
    Bottom,
}

impl Side {
    /// 这条边界拖的是这个尺寸的哪一维。
    #[must_use]
    pub fn of(self, size: egui::Vec2) -> f32 {
        match self {
            Self::Left | Self::Right => size.x,
            Self::Bottom => size.y,
        }
    }

    /// 反过来：一个长度摊成这一维上的尺寸。另一维填 0——面板只从这个尺寸里读它那一维。
    #[must_use]
    pub fn size(self, along: f32) -> egui::Vec2 {
        match self {
            Self::Left | Self::Right => egui::vec2(along, 0.0),
            Self::Bottom => egui::vec2(0.0, along),
        }
    }
}

/// 一条**面板边界**。
///
/// [`Self::id`] 同时是三样东西：`egui::Panel` 的 id、面板尺寸在 `egui` 那张表里的键、
/// 以及落盘时那一行的名字。**三样是同一串字不是巧合**——分开写就会有一处漏改，
/// 而漏改的症状是「拖了但不记得」，看起来像存盘坏了。
#[derive(Debug, Clone, Copy)]
pub struct Boundary {
    /// 面板 id，也是存盘时那一行的名字。
    pub id: &'static str,
    /// 哪一屏上的。
    pub screen: View,
    /// 靠哪一边。
    pub side: Side,
    /// 头一次打开时多宽（多高）。
    pub default: f32,
    /// 最少多宽。**拖到底也塌不下去。**
    pub min: f32,
    /// 最多占**整个窗口**在这一维上的几成。同一屏同一维上几条加起来不超过八成。
    pub share: f32,
}

/// 浏览屏左边那栏：五个一按就有的档 ＋ 条件组 ＋ 搜索 ＋ 收藏合集 ＋ 存成子库。
pub const FILTER: Boundary = Boundary {
    id: "筛选",
    screen: View::Browse,
    side: Side::Left,
    default: 230.0,
    min: 150.0,
    share: 0.35,
};

/// 浏览屏右边那块：作品 → 变体 → 文件 → 媒体。
pub const DETAIL: Boundary = Boundary {
    id: "浏览详情",
    screen: View::Browse,
    side: Side::Right,
    default: 360.0,
    min: 200.0,
    share: 0.45,
};

/// 浏览屏底下那块：看与改选中那个变体的元数据。**输入法全在这一块上。**
pub const EDIT: Boundary = Boundary {
    id: "浏览编辑",
    screen: View::Browse,
    side: Side::Bottom,
    default: 260.0,
    min: 110.0,
    share: 0.40,
};

/// 浏览屏底下那块**刮削面板**。摊开才有，收起来这条边界就不在屏上。
pub const SCRAPE: Boundary = Boundary {
    id: "刮削面板",
    screen: View::Browse,
    side: Side::Bottom,
    default: 300.0,
    min: 160.0,
    share: 0.40,
};

/// 待确认屏逐条那一路左边那栏：选择器与整批操作。
pub const BATCHES: Boundary = Boundary {
    id: "批量",
    screen: View::Queue,
    side: Side::Left,
    default: 320.0,
    min: 180.0,
    share: 0.40,
};

/// 待确认屏逐条那一路底下那块：这一条是什么、候选、裁决表单。
pub const DECIDE: Boundary = Boundary {
    id: "裁决面板",
    screen: View::Queue,
    side: Side::Bottom,
    default: 268.0,
    min: 120.0,
    share: 0.45,
};

/// 子库屏底下那块：配目标——名字、路径、前端格式、容量上限。
pub const TARGET: Boundary = Boundary {
    id: "配目标",
    screen: View::Sublibraries,
    side: Side::Bottom,
    default: 190.0,
    min: 110.0,
    share: 0.40,
};

impl Boundary {
    /// 全部七条，**照各屏真正摆它们的次序**。不在这儿的边界不落盘。
    ///
    /// 次序不是随手排的：面板是**一块接一块**吃地方的（先摆的把地方吃掉一截，后摆的
    /// 看见的是剩下的），而 [`Self::cap`] 第二道正是按「眼下还剩多少」算的。
    /// 这张表照实排，「一块接一块吃下去正中那块还剩得下 [`FLOOR`]」那条才算得准
    /// ——排错了，算出来的是另一套摆法的账。
    // 三屏各自照 `Screen::ui` 里 `show` 的先后排；`rustfmt` 会把它挤成一行，
    // 而这张表的次序**是有意义的**，所以不让它挤。
    #[rustfmt::skip]
    pub const ALL: [Self; 7] = [
        // 浏览屏（`browse::Screen::ui`）
        SCRAPE,
        EDIT,
        FILTER,
        DETAIL,
        // 待确认屏逐条那一路（`queue::Screen::ui`）
        DECIDE,
        BATCHES,
        // 子库屏（`sublibrary::Screen::ui`）
        TARGET,
    ];

    /// 画这块面板。
    ///
    /// **内容头一件事是把这块地方占满**（`take_available_*`），不占满的话拖不动：
    /// `egui::Panel` 存下来的尺寸是**内容量出来的那个**，而不是它请求的那个——
    /// 于是拖宽了，下一帧内容又把它缩回去。这一条是 `egui::Panel::resizable` 的文档
    /// 点名写着的前提，收之前那几块底栏正是漏了它（「浏览编辑」不管拖到多高，
    /// 下一帧都缩回下限 110）。
    pub fn show<R>(self, ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
        self.panel(ui)
            .show(ui, |ui| {
                match self.side {
                    Side::Left | Side::Right => ui.take_available_width(),
                    Side::Bottom => ui.take_available_height(),
                }
                add_contents(ui)
            })
            .inner
    }

    /// 这条边界这一帧最多拖到多宽。
    ///
    /// **两道一起夹**，各管一头：
    ///
    /// - **按整个窗口的几成**（[`Self::share`]）。按窗口算而不是按「摆到它那一刻还剩多少」
    ///   算，是为了让上限是个**定数**——后者会让先摆的那块挤后摆的：摊开刮削面板之后，
    ///   底下那块元数据面板的上限跟着缩水，人明明没碰它，它却自己矮了一截。
    /// - **「眼下还剩多少」减去 [`FLOOR`]**。上一道按的是整个窗口，可面板是摆在顶栏底下、
    ///   并且一块接一块地吃地方的——窗口小到 720×480、刮削面板又摊开时，光按几成算会让
    ///   正中那张表只剩七十来点（表头 24 加两行）。这一道是**兜底**：不管前面吃掉多少，
    ///   它后面**一定还剩得下 [`FLOOR`] 点**。
    ///
    /// **下限赢**：窗口小到连下限都摆不开时，宁可让它超出，也不塌成一条缝。
    #[must_use]
    pub fn cap(self, viewport: egui::Vec2, room: f32) -> f32 {
        (self.side.of(viewport) * self.share)
            .min(room - FLOOR)
            .max(self.min)
    }

    /// 照这条边界起一个面板，**先不画**。[`Self::show`] 用它；单拎出来是为了
    /// 「怎么拖」这件事只在一处说。
    pub fn panel(self, ui: &egui::Ui) -> egui::Panel {
        let room = self.side.of(ui.available_rect_before_wrap().size());
        let max = self.cap(ui.ctx().viewport_rect().size(), room);
        let panel = match self.side {
            Side::Left => egui::Panel::left(self.id),
            Side::Right => egui::Panel::right(self.id),
            Side::Bottom => egui::Panel::bottom(self.id),
        };
        panel
            .resizable(true)
            .default_size(self.default)
            .min_size(self.min)
            .max_size(max)
    }

    /// 一个存下来的数**收进这条边界认的范围**。
    ///
    /// 上限这儿只用一个够宽的绝对值（`SANE`）——真正的上限要等到画那一帧才知道
    /// 还剩多少地方，`egui::Panel` 会再夹一次。这一道只拦「文件被人改成 1e9」那种。
    #[must_use]
    pub fn clamp(self, size: f32) -> f32 {
        if !size.is_finite() {
            return self.default;
        }
        size.clamp(self.min, SANE)
    }
}

/// 存下来的数最大认到这儿，点。比任何一块屏都宽，只用来拦离谱的值。
const SANE: f32 = 4000.0;

/// 正中那块**至少**留这么多，点。
///
/// 拖到极限时正中那张表还得摆得下表头加几行（表头 24 点、一行 21 点）。
/// 它是每条边界上限里的第二道（[`Boundary::cap`]），也是
/// `tests/layout.rs::拖到极限时不塌陷也不把正中那块挤没` 断的那个数。
pub const FLOOR: f32 = 120.0;

/// 那份文件开头写着的几句话。**它是给人看的**：一个不认得的文件躺在工作目录里，
/// 第一个问题永远是「删了会怎样」。
const HEADER: &str = "\
# romcat 界面的版式偏好：几条面板边界各自拖到哪儿了，单位是点。
# 这一份**随时可以删**：删了就回到默认版式，库里一个字节都不动。
# 它**不在中立库里**——中立库整份可再生，界面偏好放进去会被某一次重扫抹掉。
";

/// 七条边界各自拖到哪儿了，以及它落在哪个文件上。
#[derive(Debug)]
pub struct Layout {
    /// 那份文件在哪。**在工作目录里**（[`romcat_core::workspace::gui_layout_path`]）。
    path: PathBuf,
    /// 眼下各是多少。没记过的那几条不在这张表里，那时用默认值。
    sizes: BTreeMap<&'static str, f32>,
    /// **上一次真写进文件的是哪几个数。** 拿它与 [`Self::sizes`] 比，才知道要不要写盘。
    saved: BTreeMap<&'static str, f32>,
    /// 上一次写盘出的错。**不静默吞掉**：吞了的话人只看见「拖了半天，下次全忘」。
    error: Option<String>,
}

impl Layout {
    /// 从工作目录里读一份。**读不到、读不懂都不算错**——那时就是默认版式。
    ///
    /// 界面偏好没有「必须在」这回事：文件不在是第一次打开，文件坏了是人手改坏的。
    /// 两种情形下把窗口开起来都比报一句错有用。
    #[must_use]
    pub fn load(workspace: &Path) -> Self {
        let path = romcat_core::workspace::gui_layout_path(workspace);
        let sizes = std::fs::read_to_string(&path)
            .map(|text| parse(&text))
            .unwrap_or_default();
        Self {
            path,
            saved: sizes.clone(),
            sizes,
            error: None,
        }
    }

    /// 那份文件在哪。测试拿它核对「存在工作目录里、不在中立库里」。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 这条边界记着多少。没记过就是 `None`（那时用这条边界的 `default`）。
    #[must_use]
    pub fn size(&self, boundary: Boundary) -> Option<f32> {
        self.sizes.get(boundary.id).copied()
    }

    /// 上一次写盘出的错。顶栏上照它画一句。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 把记着的几个数**塞进 `egui` 那张面板尺寸表**，赶在这一帧画面板之前。
    ///
    /// `egui::Panel` 只在那张表里**没有**这一条时才用 `default_size`，所以「记住上次
    /// 拖到哪儿」这件事，落到实处就是开窗第一帧先把那张表填好。**只在开窗那一帧做一次**
    /// ——每帧都塞的话，人正拖着的那一下会被上一次存的值按回去。
    pub fn seed(&self, ctx: &egui::Context) {
        for boundary in Boundary::ALL {
            let Some(size) = self.size(boundary) else {
                continue;
            };
            let outer_rect = egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                boundary.side.size(boundary.clamp(size)),
            );
            ctx.data_mut(|data| {
                data.insert_persisted(egui::Id::new(boundary.id), egui::PanelState { outer_rect });
            });
        }
    }

    /// 画完一帧之后问一遍：这一帧**有没有人松开某条边界的把手**，松开在哪儿。
    ///
    /// **只记人拖出来的那个数，不记「这一帧画成多宽」。** 这不是省事，是必须的：
    /// `egui::Panel` 存下来的尺寸是**夹过上限之后**的，而上限跟着窗口大小走。照单全收的话
    /// ——最大化窗口把「浏览详情」拖到 700，还原窗口那一帧它被夹到 576，于是 576 盖掉 700，
    /// 再最大化也回不来了。而窗口尺寸本身**不持久化**（`eframe` 的 `persistence` 没开），
    /// 每次开窗都回到 1280×800，所以那一刀每次启动都要挨一遍。
    ///
    /// 于是判据换成「**把手被松开了**」：那一帧 `egui` 正好也把释放位置存进了 `PanelState`
    /// （它拖动过程中不存，松手那一帧才存），两件事对得上。
    ///
    /// 没被拖过的边界一个字都不记——那时它用的是默认值，写进文件只是把默认值抄一遍。
    pub fn harvest(&mut self, ctx: &egui::Context) {
        for boundary in Boundary::ALL {
            let 松开了 = ctx
                .read_response(resize_id(boundary))
                .is_some_and(|handle| handle.drag_stopped());
            if !松开了 {
                continue;
            }
            let Some(state) = egui::PanelState::load(ctx, egui::Id::new(boundary.id)) else {
                continue;
            };
            // **收成整点**：面板尺寸是浮点算出来的，写进文件的那一行不该带着尾数。
            let size = boundary.side.of(state.size()).round();
            if size > 0.0 {
                self.sizes.insert(boundary.id, size);
            }
        }
    }

    /// 数变了就落盘。**没变就一个字节都不写。**
    ///
    /// 调它的地方要先确认手已经松开（`App::ui` 那一句）：拖的过程中每帧
    /// 写一次是六十次写盘，而那六十次里有五十九次的值是过路的。
    pub fn flush(&mut self) {
        if self.sizes == self.saved {
            return;
        }
        // 不管写成没写成，都记成「写过了」：写不成时每帧再试一次只是把同一个错刷六十遍。
        // 下一次拖动会再试一次——那时人正等着它记住，重试才有意义。
        self.saved = self.sizes.clone();
        self.error = write(&self.path, &self.render()).err();
    }

    /// 摊成要写进文件的那几行。
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::from(HEADER);
        // **照 `Boundary::ALL` 的次序写**，不照哈希表的次序：那样两次写出来的文件
        // 逐字一样，`git diff` 与人眼都看得出到底哪一条变了。
        for boundary in Boundary::ALL {
            if let Some(size) = self.size(boundary) {
                out.push_str(&format!("{} = {:.0}\n", boundary.id, size));
            }
        }
        out
    }
}

/// `egui` 给一条边界那个**把手**起的 id。
///
/// 那个后缀在 `egui` 里是私有的（`containers::panel` 里的 `resize_widget_id`：
/// `id_source.with("__resize")`），这儿只能照抄一份。**抄错了不会静默**——
/// `tests/layout.rs` 里「拖完落盘」与「关掉再打开还是那个样子」两条走的是真的
/// 按下、拖过去、松手，找不着这个把手它们当场就红。
fn resize_id(boundary: Boundary) -> egui::Id {
    egui::Id::new(boundary.id).with("__resize")
}

/// 读那几行。**读不懂的行、不认得的名字，一律跳过。**
///
/// 跳过而不是报错：这个文件的全部内容是「面板拖到哪儿」，一行坏掉的代价是那一条回到
/// 默认宽度。为它把窗口关掉，或者为它把另外六条一起丢掉，都不成比例。
fn parse(text: &str) -> BTreeMap<&'static str, f32> {
    let mut sizes = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let (name, value) = (name.trim(), value.trim());
        let Some(boundary) = Boundary::ALL.into_iter().find(|it| it.id == name) else {
            continue;
        };
        let Ok(size) = value.parse::<f32>() else {
            continue;
        };
        sizes.insert(boundary.id, boundary.clamp(size));
    }
    sizes
}

/// 写下去，连目录一起建。
fn write(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("建不出 {}：{error}", romcat_core::path::display(parent)))?;
    }
    std::fs::write(path, text)
        .map_err(|error| format!("写不进 {}：{error}", romcat_core::path::display(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 七条边界的名字互不相同() {
        // 名字同时是 `egui` 的面板 id：撞了的话两块面板会共用一个尺寸，
        // 拖左栏右栏跟着动。
        for at in 0..Boundary::ALL.len() {
            for other in (at + 1)..Boundary::ALL.len() {
                assert_ne!(
                    Boundary::ALL[at].id,
                    Boundary::ALL[other].id,
                    "两条边界重名了"
                );
            }
        }
    }

    #[test]
    fn 同一屏同一维上的几条加起来挤不没正中那块() {
        // 头一道：份额之和。它不是硬保证（真正兜底的是 `cap` 里那道 `room - FLOOR`），
        // 但它保证**常态下那道兜底不必出手**——出手就意味着先摆的那块在挤后摆的，
        // 而那正是这份声明想避开的手感。
        for screen in [View::Browse, View::Queue, View::Sublibraries] {
            for 横着 in [true, false] {
                let 加起来: f32 = Boundary::ALL
                    .iter()
                    .filter(|it| it.screen == screen && (it.side != Side::Bottom) == 横着)
                    .map(|it| it.share)
                    .sum();
                assert!(
                    加起来 <= 0.8,
                    "{screen:?} 这一维上几条加起来吃掉 {:.0}%，正中那块留不住两成",
                    加起来 * 100.0,
                );
            }
        }
    }

    #[test]
    fn 一块接一块吃下去正中那块还剩得下地板那么多() {
        // 第二道，也是真正的那道保证：每条边界最多吃到「眼下还剩多少减去 `FLOOR`」，
        // 于是一块接一块摆完，剩下的还有 `FLOOR`。这儿按**最小窗口**（720×480，
        // `main.rs` 的 `with_min_inner_size`）连着算一遍，顶栏那 26 点先扣掉。
        let 最小窗口 = egui::vec2(720.0, 480.0);
        for screen in [View::Browse, View::Queue, View::Sublibraries] {
            for 横着 in [true, false] {
                // 顶栏只吃高，不吃宽。
                let mut room = if 横着 {
                    最小窗口.x
                } else {
                    最小窗口.y - 26.0
                };
                for boundary in Boundary::ALL
                    .iter()
                    .filter(|it| it.screen == screen && (it.side != Side::Bottom) == 横着)
                {
                    room -= boundary.cap(最小窗口, room);
                }
                assert!(
                    room >= FLOOR - 1.0,
                    "{screen:?} 这一维上全拖到头之后，正中那块只剩 {room} 点",
                );
            }
        }
    }

    #[test]
    fn 默认那个宽度落在下限与上限之间() {
        // 上限比默认还小的话，头一次打开看见的就已经是被夹过的样子——
        // 那时人以为「默认就这么窄」，其实是这张表自相矛盾。视口按窗口最小尺寸算
        // （`main.rs` 的 `with_min_inner_size`）。
        let 最小窗口 = egui::vec2(720.0, 480.0);
        for boundary in Boundary::ALL {
            assert!(
                boundary.min <= boundary.default,
                "「{}」的默认 {} 比下限 {} 还小",
                boundary.id,
                boundary.default,
                boundary.min,
            );
            let 上限 = boundary.cap(最小窗口, boundary.side.of(最小窗口));
            assert!(
                boundary.min <= 上限,
                "「{}」在最小窗口上，上限 {上限} 掉到了下限 {} 底下",
                boundary.id,
                boundary.min,
            );
        }
    }

    #[test]
    fn 三屏都至少有一条拖得动的边界() {
        // 验收第 1 条点名的三屏。库屏与任务屏各只有一块正文，没有边界可拖。
        for screen in [View::Browse, View::Queue, View::Sublibraries] {
            assert!(
                Boundary::ALL.iter().any(|it| it.screen == screen),
                "{screen:?} 上一条边界都没有",
            );
        }
    }

    #[test]
    fn 读不懂的行与不认得的名字一律跳过() {
        let sizes = parse(
            "# 一句注释\n\
             \n\
             筛选 = 300\n\
             没这条边界 = 100\n\
             浏览详情 = 读不懂\n\
             这行没有等号\n\
             配目标=125.4\n",
        );
        assert_eq!(sizes.get("筛选"), Some(&300.0));
        assert_eq!(sizes.get("配目标"), Some(&125.4));
        assert_eq!(sizes.get("没这条边界"), None, "不认得的名字不该进来");
        assert_eq!(sizes.get("浏览详情"), None, "读不懂的值不该进来");
    }

    #[test]
    fn 离谱的值当场夹回范围内() {
        // 文件是纯文本，人改得动。拖不塌、也挤不没，这一道是第一层
        // （`egui::Panel` 画那一帧还会照「真的剩多少」再夹一次）。
        let sizes = parse("筛选 = 0\n浏览详情 = 999999\n批量 = -5\n");
        assert_eq!(sizes.get("筛选"), Some(&FILTER.min));
        assert_eq!(sizes.get("浏览详情"), Some(&SANE));
        assert_eq!(sizes.get("批量"), Some(&BATCHES.min));
    }

    #[test]
    fn 写出去再读回来是同一份() {
        let mut layout = Layout {
            path: PathBuf::from("/dev/null"),
            sizes: BTreeMap::new(),
            saved: BTreeMap::new(),
            error: None,
        };
        layout.sizes.insert(FILTER.id, 275.0);
        layout.sizes.insert(TARGET.id, 210.0);
        let text = layout.render();
        assert!(text.starts_with('#'), "开头那几句给人看的话不能丢");
        assert_eq!(parse(&text), layout.sizes);
    }
}
