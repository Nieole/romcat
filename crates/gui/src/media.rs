//! 详情面板里那几格**缩略图**。
//!
//! ## 这一层只把图画出来
//!
//! 解码、抽首帧、按内容哈希缓存进**媒体池**——三件事一件都不在这儿，全在
//! [`romcat_core::scrape::preview`]（ADR-0005）。这一层拿到的只是一片 RGBA 加宽高，
//! 干的活是把它传上显卡、按格子摆好、把点下去那一下转发回核心。
//!
//! ## 画帧那条线程一次都不等
//!
//! [`Loader`] 在后台解，这一层每帧问一次「好了没」：好了就画图，没好就画占位并请下一帧
//! 再来（`request_repaint`）。**翻行时切换选中不会卡**——那是这张票的一条验收，
//! 而它成立的理由就是这一层从不自己解码。
//!
//! ## 视频是一张首帧加一个播放标
//!
//! 点下去交给**系统默认程序**（[`preview::open_externally`]），窗口里一个字节都不解码——
//! 那要 ffmpeg 的 Rust 绑定，是规格的 Out of Scope。**ffmpeg 不在的机器上照常可用**：
//! 那一格显示占位，面板上说一句为什么，视频照样点得开。

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use romcat_core::catalog::Catalog;
use romcat_core::catalog::detail::MediaItem;
use romcat_core::scrape::pool::MediaPool;
use romcat_core::scrape::preview::{self, EDGE, Frame, Key, Loader, Missing, Preview, Thumbnail};

/// 一格**最少**多宽，点。照原型 `prototype.html` 里 `.thumb` 那条 `minmax(78px, 1fr)`
/// 的**前半截**。
///
/// 后半截那个 `1fr` 归 [`cell_size`] 管：**格子跟着详情面板的宽度长**。这一栏从前把
/// 78 点定死了（挂单 `Q126`），于是把面板拖宽一倍，多出来的地方全空着——而这一票的
/// 头一条验收正是「面板边界拖得动」。
pub const CELL: f32 = 78.0;

/// 一格的高宽比。原型上那格是 `aspect-ratio: 3/4`——竖着的封面是这一栏里最常见的形状。
const RATIO: f32 = 4.0 / 3.0;

/// 这么宽的一栏里，一格该多大。
///
/// 就是 CSS 那条 `repeat(auto-fill, minmax(78px, 1fr))`：先看这一栏摆得下几列
/// （每列至少 [`CELL`] 宽，列与列之间空 `spacing`），再把剩下的宽度均分给这几列。
/// 于是**面板拖宽一点，格子跟着大一点**，而不是右边空出一条。
///
/// 宽度**向下取整**：算出来的几列加上几个间隙必须摆得进这一栏，多出零点几个点就会
/// 少掉一列，那一列会掉到下一行去。
#[must_use]
pub fn cell_size(available: f32, spacing: f32) -> egui::Vec2 {
    let columns = ((available + spacing) / (CELL + spacing)).floor().max(1.0);
    let width = ((available - spacing * (columns - 1.0)) / columns)
        .floor()
        .max(CELL);
    egui::vec2(width, width * RATIO)
}

/// 缓存里最多攒几份图。
///
/// 详情面板一次只摆一个变体的媒体（真库上几件），这个数**不是为一屏定的**，
/// 是为「来回切几个变体不必每次重解」留的余量。超过了就只留还看得见的那几份——
/// 不淘汰的话，按真库翻一遍会攒下六百来份 RGBA，内存与显存各几百兆且永不回落。
const KEEP: usize = 64;

/// **等比缩到框里**，一个像素都不拉伸。
///
/// 单拎成一个函数是为了它测得到：「按比例缩放不变形」是这张票的头一条验收，而一个
/// 「宽高各自填满」的实现在跑起来之前看着与它一模一样。
#[must_use]
pub fn fit_into(inner: egui::Vec2, outer: egui::Vec2) -> egui::Vec2 {
    if inner.x <= 0.0 || inner.y <= 0.0 {
        return egui::Vec2::ZERO;
    }
    // 两个方向各要缩多少，取小的那个——大的那个会让另一边溢出框外。
    let scale = (outer.x / inner.x).min(outer.y / inner.y).min(1.0);
    inner * scale
}

/// 详情面板那几格图的家当：一条后台解码线程、传上显卡的那些、以及没解出来的那些。
///
/// **没解出来的也记着**：一份坏图不该每帧重试一次，而「为什么没有图」本身正是
/// 「媒体读不动时说清是哪一件」那条验收要的东西。
#[derive(Default)]
pub struct Gallery {
    /// 后台那条解码线程。**媒体池不在位时是 `None`**——那时这一栏如实说「没查池子」，
    /// 而不是摆一屏「找不到文件」。
    loader: Option<Loader>,
    /// 传上显卡的那些。
    textures: HashMap<Key, egui::TextureHandle>,
    /// 没解出来的那些各是为什么。
    missing: HashMap<Key, Missing>,
    /// **抽出来了、已经落进池里、还没记进中立库的那几份首帧。**
    ///
    /// 两种情形会攒在这儿：库那一头正忙（扫描在后台跑，写锁被占着），或者上一趟
    /// 记库失败了。**攒着比丢掉要紧**——池里那个文件已经落下去了，不记库的话它就是
    /// 一个孤儿，下次打开照样重抽一遍。
    pending: Vec<Frame>,
    /// 记首帧那一下写库出的错。**不静默吞掉**：吞了的话人只看见「每次打开都重抽」。
    ///
    /// 它跟着 [`Self::pending`] 一起活：记进去了就清掉，没记进去就留着。
    /// 一句已经不成立的红字挂在面板上，比不说更坏。
    error: Option<String>,
}

impl std::fmt::Debug for Gallery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Gallery")
            .field("loader", &self.loader.is_some())
            .field("textures", &self.textures.len())
            .field("missing", &self.missing.len())
            .field("pending", &self.pending.len())
            .field("error", &self.error)
            .finish()
    }
}

impl Gallery {
    /// 开一个空的。**媒体池还没指进来，一格都画不出。**
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 指一份**媒体池**。指了才解得出图。
    ///
    /// 换池子时**连缓存一起丢**：同一个内容哈希在两个池子里指的是两个文件，
    /// 留着上一份的纹理等于把上一份库的封面画在这一份库的行上。
    pub fn set_pool(&mut self, pool: Option<MediaPool>) {
        self.textures.clear();
        self.missing.clear();
        self.pending.clear();
        self.error = None;
        self.loader = pool.map(|pool| Loader::start(pool, EDGE));
    }

    /// 换一个抽首帧的程序。**测试拿它走「ffmpeg 不在」那条路。**
    pub fn set_program(&mut self, program: &str) {
        if let Some(loader) = self.loader.as_mut() {
            loader.set_program(program);
        }
    }

    /// 指进池子了吗。
    #[must_use]
    pub fn has_pool(&self) -> bool {
        self.loader.is_some()
    }

    /// 眼下摆着几张解出来的图。
    #[must_use]
    pub fn ready(&self) -> usize {
        self.textures.len()
    }

    /// 还有几件在后台跑着。
    #[must_use]
    pub fn busy(&self) -> usize {
        self.loader.as_ref().map_or(0, Loader::busy)
    }

    /// 上一次记首帧出的错；记进去了就没有了。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 还有几份首帧落了池、没记进中立库。
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// 这一屏上有没有「这台机器没装 ffmpeg」那一档。
    ///
    /// 面板拿它决定那句提示说一遍还是每格都说——178 个视频各摆一句是噪音。
    #[must_use]
    pub fn lacks_ffmpeg(&self) -> bool {
        self.missing.values().any(Missing::is_no_ffmpeg)
    }

    /// 这一屏上**没解出来**的那几件各是怎么回事：`(是哪一件, 为什么)`。
    ///
    /// **「媒体读不动时说清是哪一件、为什么，不静默留空」**是这张票的一条验收。
    /// 只把话藏在悬停提示里不算说清——人得先知道该往哪一格上悬停。
    ///
    /// **「这台机器没装 ffmpeg」那一档不在里头**：面板另说一句就够了，
    /// 真库里 178 个视频各摆一行是噪音（[`Self::lacks_ffmpeg`]）。
    #[must_use]
    pub fn troubles(&self, items: &[MediaItem]) -> Vec<(String, String)> {
        items
            .iter()
            .filter_map(|item| match self.look(item) {
                Look::Missing(why) if !why.is_no_ffmpeg() => Some((
                    format!("{} · {}", item.kind.label(), item.source),
                    why.render(),
                )),
                _ => None,
            })
            .collect()
    }

    /// 这一份的图好了没；没好（或者没有）时说得出为什么。
    #[must_use]
    pub fn look(&self, item: &MediaItem) -> Look<'_> {
        let key = key_of(item);
        if let Some(texture) = self.textures.get(&key) {
            return Look::Ready(texture);
        }
        match self.missing.get(&key) {
            Some(why) => Look::Missing(why),
            None if self.loader.is_none() => Look::NoPool,
            None => Look::Waiting,
        }
    }

    /// **每帧一次**：把后台跑完的收进来，把这一屏要的排出去。
    ///
    /// `writable` 是「眼下动得动中立库吗」。**任务台上有活在跑时给 `false`**：
    /// 那时后台正拿着另一份**写得动**的连接（扫描），而这条线程上的写会在
    /// `busy_timeout` 上一直等——最长十秒的画帧线程阻塞，正是这张票不许出现的东西。
    /// 记不成不丢：首帧攒在 [`Self::pending`] 里，下一帧再试。
    ///
    /// 落盘那一半后台已经做完了，记库这一半只能在这条线程上——
    /// **中立库全程只有一个写者**。
    pub fn sync(
        &mut self,
        ctx: &egui::Context,
        catalog: &mut Catalog,
        items: &[MediaItem],
        writable: bool,
    ) {
        let Some(loader) = self.loader.as_mut() else {
            return;
        };
        // 一、**先把后台跑完的收干净**——这一步与「这一屏正摆着什么」无关。
        //     不收的话，人点开一段视频、抽到一半切走，那份已经落进池里的首帧就成了
        //     孤儿：池里多一个文件，两张表里一行都没有，下次打开照样重抽。
        self.pending.extend(loader.drain());
        if writable && !self.pending.is_empty() {
            let mut 还没记的 = Vec::new();
            let mut 出的错 = None;
            for frame in std::mem::take(&mut self.pending) {
                // **先 put_media 再 put_media_frame**：`media_frame.frame` 指着
                // `media(hash)`，反过来写外键当场不认。
                let recorded = catalog
                    .put_media(&frame.hash, "png", frame.bytes)
                    .and_then(|()| catalog.put_media_frame(&frame.video, &frame.hash));
                if let Err(source) = recorded {
                    出的错 = Some(format!("首帧记不进中立库：{source}"));
                    还没记的.push(frame);
                }
            }
            self.pending = 还没记的;
            // **记进去了就把那句红字撤了。** 留着一句已经不成立的话比不说更坏。
            self.error = 出的错;
        }

        // 二、这一屏要的那几份，缺的排出去。
        //     **先把要问的抄出来**：问的时候要动 loader，而 items 是从详情里借来的。
        let mut fresh: Vec<(Key, egui::TextureHandle)> = Vec::new();
        let mut misses: Vec<(Key, Missing)> = Vec::new();
        for item in items {
            let key = key_of(item);
            if self.textures.contains_key(&key) || self.missing.contains_key(&key) {
                continue;
            }
            // **已经排出去的就别再问一遍。** 抽一帧要几百毫秒，这期间每帧再查一次
            // 「这份视频的首帧记过没有」是一次白花的库查询——而它跑在画帧线程上。
            if loader.inflight(&item.hash, &item.ext) {
                continue;
            }
            // 抽过的视频直接解那一份首帧，**一个进程都不拉起**。
            let frame = if preview::is_video(&item.ext) {
                match catalog.media_frame(&item.hash) {
                    Ok(frame) => frame,
                    Err(source) => {
                        self.error = Some(format!("首帧读不出来：{source}"));
                        None
                    }
                }
            } else {
                None
            };
            match loader.want(&item.hash, &item.ext, frame.as_deref()) {
                None => {}
                Some(Preview::Missing(why)) => misses.push((key, why.clone())),
                Some(Preview::Ready(thumb)) => {
                    fresh.push((key.clone(), upload(ctx, &key, thumb)));
                }
            }
        }
        self.textures.extend(fresh);
        self.missing.extend(misses);

        // 三、**攒过头就只留还看得见的那几份。** 按真库翻一遍是六百来份 RGBA，
        //     显存与内存各几百兆且永不回落。上限之内一份不动——来回切两个变体
        //     不该每次重解。
        let 看得见的: HashSet<Key> = items.iter().map(key_of).collect();
        if self.textures.len() > KEEP {
            self.textures.retain(|key, _| 看得见的.contains(key));
            self.missing.retain(|key, _| 看得见的.contains(key));
        }
        loader.trim(&看得见的, KEEP);

        // 四、还有在跑的就请下一帧再来。**这就是「不阻塞画帧」**：
        //     这一帧照常画完，图是下一帧的事。攒着没记的首帧也要下一帧。
        if loader.busy() > 0 || !self.pending.is_empty() {
            ctx.request_repaint();
        }
    }

    /// 画一格，`size` 那么大（[`cell_size`] 算出来的）。点了就返回[这一下算什么](Clicked)。
    ///
    /// 图片也点得开：缩略图长边只有 [`EDGE`] 像素，而池里那些封面动辄两千——
    /// 「想看清楚点」与「想放这段视频」是同一下动作，没道理只给视频。
    pub fn cell(&self, ui: &mut egui::Ui, item: &MediaItem, size: egui::Vec2) -> Option<Clicked> {
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
        let visuals = ui.visuals().clone();
        // **键盘焦点在这一格上也得看得见。** 这一格是自己画的，不走 egui 的按钮那条路，
        // 所以 [`crate::look::install`] 换的那圈强调色到不了这儿——得自己照它描一圈
        // （票 `gui-redesign/12` 验收第 7 条）。
        let focused = response.has_focus();
        let painter = ui.painter();
        let radius = 4.0;
        painter.rect_filled(rect, radius, visuals.extreme_bg_color);

        let look = self.look(item);
        let 边框 = match &look {
            // 池里没这个文件是**警告**，不是「还没好」：导出时它一张都铺不出去。
            Look::Missing(Missing::NotInPool { .. }) => {
                egui::Stroke::new(1.0, visuals.warn_fg_color)
            }
            _ => visuals.widgets.noninteractive.bg_stroke,
        };
        painter.rect_stroke(rect, radius, 边框, egui::StrokeKind::Inside);
        if focused {
            // **焦点那一圈画在里面一点，不顶掉原来那道边框**：那道边框是「池里没这个
            // 文件」的警告，被焦点顶掉的话，Tab 走过去的那一格就不再警告了。
            painter.rect_stroke(
                rect.shrink(2.0),
                radius,
                egui::Stroke::new(2.0, visuals.selection.stroke.color),
                egui::StrokeKind::Inside,
            );
        }

        match look {
            Look::Ready(texture) => {
                // **等比缩到格子里**，不填满、不拉伸。
                let size = fit_into(texture.size_vec2(), rect.size() - egui::vec2(4.0, 4.0));
                let at = egui::Rect::from_center_size(rect.center(), size);
                egui::Image::new(texture).paint_at(ui, at);
            }
            Look::Waiting => {
                标签(ui, rect, "…", visuals.weak_text_color());
            }
            Look::NoPool => {
                标签(ui, rect, "没查池子", visuals.weak_text_color());
            }
            Look::Missing(_) => {
                标签(ui, rect, item.kind.label(), visuals.weak_text_color());
            }
        }

        if preview::is_video(&item.ext) {
            // **播放标**：原型 `.thumb.vid::after` 那一层半透明黑底加一个 ▶。
            // 它压在首帧上，于是「这是段视频」不必读文字就看得出来。
            let painter = ui.painter();
            painter.rect_filled(rect, radius, egui::Color32::from_black_alpha(80));
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "▶",
                egui::FontId::proportional(18.0),
                egui::Color32::WHITE,
            );
        }

        let response = response.on_hover_text(self.tip(item));
        if !response.clicked() {
            return None;
        }
        Some(clicked_on(item))
    }

    /// 悬停时那一段：它是什么、来自哪个源、落在池里哪儿、**依据**，以及没有图时为什么。
    fn tip(&self, item: &MediaItem) -> String {
        let mut out = format!(
            "{} · {}｜{}\n{}.{}｜依据：{}",
            item.kind.label(),
            item.anchor.label(),
            item.source,
            item.hash,
            item.ext,
            item.evidence,
        );
        match self.look(item) {
            Look::Missing(why) => out.push_str(&format!("\n{}", why.render())),
            Look::Waiting => out.push_str("\n正在后台解……"),
            Look::NoPool => out.push_str("\n媒体池不在工作目录里，这一栏查不了。"),
            Look::Ready(_) => out.push_str(&format!(
                "\n点一下用系统默认程序打开{}",
                if preview::is_video(&item.ext) {
                    "（播放器）"
                } else {
                    "（看原图）"
                }
            )),
        }
        out
    }
}

/// 点一格算什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Clicked {
    /// 用**系统默认程序**打开这个文件：视频给播放器，图片给看图工具。
    Open(PathBuf),
    /// 点了，可这一格指不出文件——**这是为什么**。
    ///
    /// 单立一档而不是返回 `None`：点下去一句反馈都没有，人只会以为界面坏了。
    Nothing(String),
}

/// 一份媒体眼下看着是什么样。
///
/// 手写 `Debug`：`egui::TextureHandle` 自己没有，而这一档带着它。
#[derive(Clone, Copy)]
pub enum Look<'a> {
    /// 图好了，就是这张。
    Ready(&'a egui::TextureHandle),
    /// 还在后台解。
    Waiting,
    /// 没有图，这是为什么。
    Missing(&'a Missing),
    /// **媒体池不在位**，压根没查——与「查了、没有」是两件事。
    NoPool,
}

impl std::fmt::Debug for Look<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ready(texture) => write!(f, "Ready({:?})", texture.size()),
            Self::Waiting => f.write_str("Waiting"),
            Self::Missing(why) => write!(f, "Missing({why:?})"),
            Self::NoPool => f.write_str("NoPool"),
        }
    }
}

/// 点这一格算什么。
///
/// **池里没那个文件就别说「打开了」**：[`preview::open_externally`] 走的是 `spawn`，
/// `xdg-open` / `start` 这个进程照样起得来、照样返回成功，而什么都没打开——
/// 于是屏上那句「交给系统默认程序打开」是句假话。
fn clicked_on(item: &MediaItem) -> Clicked {
    match (&item.at, item.in_pool) {
        (Some(at), Some(true)) => Clicked::Open(at.clone()),
        (_, Some(false)) => Clicked::Nothing(format!(
            "{}：{}",
            item.kind.label(),
            Missing::NotInPool {
                hash: item.hash.clone(),
                ext: item.ext.clone(),
            }
            .render(),
        )),
        _ => Clicked::Nothing("媒体池不在工作目录里，这一格指不出文件在哪。".to_string()),
    }
}

/// 一格里那句居中的小字。
fn 标签(ui: &egui::Ui, rect: egui::Rect, text: &str, color: egui::Color32) {
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(10.0),
        color,
    );
}

/// 一份媒体在缓存里的键：**内容哈希加扩展名**，与媒体池同一个键（ADR-0009）。
fn key_of(item: &MediaItem) -> Key {
    (item.hash.clone(), item.ext.clone())
}

/// 把一张缩略图传上显卡。
fn upload(ctx: &egui::Context, key: &Key, thumb: &Thumbnail) -> egui::TextureHandle {
    let size = [thumb.width() as usize, thumb.height() as usize];
    let image = egui::ColorImage::from_rgba_unmultiplied(size, thumb.rgba());
    // 名字带上内容哈希：egui 拿它做纹理的标识，两张图重名就会互相盖掉。
    ctx.load_texture(
        format!("媒体 {}.{}", key.0, key.1),
        image,
        egui::TextureOptions::LINEAR,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 等比缩到框里一个像素都不拉伸() {
        // 一张 2:1 的横图塞进 3:4 的竖格子：宽度顶满、高度按比例留白。
        let got = fit_into(egui::vec2(200.0, 100.0), egui::vec2(78.0, 104.0));
        assert!((got.x - 78.0).abs() < 0.01, "{got:?}");
        assert!((got.y - 39.0).abs() < 0.01, "{got:?}");
        assert!(
            (got.x / got.y - 2.0).abs() < 0.01,
            "缩完还得是 2:1，实际 {got:?}",
        );
    }

    #[test]
    fn 比格子小的图不放大() {
        let got = fit_into(egui::vec2(16.0, 16.0), egui::vec2(78.0, 104.0));
        assert_eq!(got, egui::vec2(16.0, 16.0));
    }

    /// 一条媒体引用，池里在不在由参数说了算。
    fn 一条(in_pool: Option<bool>) -> MediaItem {
        MediaItem {
            anchor: romcat_core::scrape::AnchorKind::Work,
            kind: romcat_core::scrape::MediaKind::Cover,
            source: "测试".to_string(),
            hash: "abcdef".to_string(),
            ext: "png".to_string(),
            at: in_pool.map(|_| PathBuf::from("/池/ab/abcdef.png")),
            in_pool,
            evidence: "测试".to_string(),
        }
    }

    #[test]
    fn 池里有文件才算打开得了() {
        assert_eq!(
            clicked_on(&一条(Some(true))),
            Clicked::Open(PathBuf::from("/池/ab/abcdef.png")),
        );
    }

    #[test]
    fn 池里没那个文件时点下去说得出为什么() {
        // 不说的话，`spawn` 照样成功、屏上照样写「交给系统默认程序打开」——那是句假话。
        let Clicked::Nothing(为什么) = clicked_on(&一条(Some(false))) else {
            panic!("池里没有就不该说打开得了");
        };
        assert!(为什么.contains("封面"), "得说清是哪一件：{为什么}");
        assert!(为什么.contains("abcdef"), "得说清是哪一份：{为什么}");
    }

    #[test]
    fn 没查池子时点下去说的是没查而不是没有() {
        // 与 ADR-0021 同一条规矩：「没查」与「查了、没有」得分得开。
        let Clicked::Nothing(为什么) = clicked_on(&一条(None)) else {
            panic!("没查池子就指不出文件");
        };
        assert!(为什么.contains("媒体池不在工作目录里"), "{为什么}");
    }

    #[test]
    fn 高瘦的图按高度顶满() {
        let got = fit_into(egui::vec2(300.0, 1200.0), egui::vec2(78.0, 104.0));
        assert!((got.y - 104.0).abs() < 0.01, "{got:?}");
        assert!((got.x - 26.0).abs() < 0.01, "{got:?}");
    }
}
