//! **认领一个新主库**：[开场](crate::opening)上那条一条龙向导。
//!
//! 三步——起**主库**名 → 选第一个**根** → 开扫。走完直接进主窗口，扫描已经排在
//! **任务台**上跑着（ADR-0023：界面自足，从第一步起不必开终端）。
//!
//! ## 一条领域判断都没有
//!
//! 加一个根要拦的三种情况（根名重复、与已有的根套在一起、圈进**工作目录**）全在
//! [`romcat_core::catalog::roots::add_root`]，而这一层连那个函数都不直接调——它调的是
//! **库屏加根那个函数本人**（[`crate::roots::Screen::add_root`]）。向导重画 UI，
//! 判断是同一个；两套判断只会分叉出两套行为（ADR-0005 的红线）。
//!
//! ## 晚落盘
//!
//! 起名与选根两步**只在内存里攒**：这个类型手上从头到尾只有三串字。按下「开始扫描」
//! 那一下才真的开中立库——而**开一份中立库这个动作本身就是建库**
//! （[`Catalog::open_named`] 打开即创建）。所以中途关窗、按「算了」，工作目录里一个
//! 文件都不多。
//!
//! 零根的库仍然是合法状态（核心库已经建模并测过）。晚落盘不是因为零根非法，而是因为
//! **一份零根空库对人没有用处，却会永久占着开场的一行**——而开场没有删库那条路。
//!
//! 这也是「按下开始扫描之后，加根那一条判断先在一份只活在内存里的库上走一遍」的理由：
//! 拦下的时候磁盘上什么都还没有，人改一改接着来。先建后拦的话，一份零根空库就留在那儿
//! 了，而且人再走一遍向导还会在第一步撞上自己刚留下的那个名字。

use std::path::{Path, PathBuf};

use romcat_core::catalog::Catalog;
use romcat_core::path;
use romcat_core::site::Site;
use romcat_core::workspace::{self, Slug};

use crate::roots::{ROOT_HINT, ROOT_NAME_HINT};

/// 起名那一步框里的提示字。
const NAME_HINT: &str = "主库名，例如「主库」";

/// 向导这会儿问的是哪一样。
///
/// **不叫 `Step`**：词表**工序**那一条的 `_Avoid_` 里逐字列着「步骤」，而这个仓库里
/// `Stage` 已经是**工序**（`romcat_core::stage::Stage`）。屏上照旧说「第一步」「第二步」
/// ——那是票面自己的话，说的是这条向导，不是工序。
enum Asking {
    /// 这个主库叫什么。
    Name,
    /// 第一个根在哪儿、叫什么。**「开始扫描」那颗就在这一问上**：第三步「开扫」不是
    /// 第三块界面——中间没有任何新信息，单摆一屏只让人多按一下确认。
    Root,
}

/// 向导这一帧的去向：还在走、不认领了、还是走完了。
///
/// **它不是[收场](romcat_core::task::Ending)**——那一条说的是一趟**任务**怎么结束的，
/// 这一个说的是这一帧画完之后向导还在不在。
pub enum Outcome {
    /// 还在向导里，接着画。
    Going,
    /// 人不认领了（按了「算了」）：回开场那张表。**磁盘上什么都没留下。**
    Dropped,
    /// 走完了。
    ///
    /// **装箱**：这一支拎着一份开好的**现场**（两份库各一条连接），另外两支一个字节
    /// 都不带，不装箱的话这个枚举永远按最大那一支算尺寸——而它每一帧都从画向导那条路
    /// 上返一次。
    Done(Box<Claimed>),
}

/// 向导走完交出来的那两样。
pub struct Claimed {
    /// 刚开出来的那份**现场**——中立库这一刻才建出来。
    pub site: Site,
    /// 第一个根：**还没加上**。加它与排那趟扫描都在进主窗口之后
    /// （[`crate::app::App::claim_first_root`]），走的是库屏那两条现成的路。
    pub root: FirstRoot,
}

/// 向导攒下的第一个根：两串字，原样。
///
/// **不在这儿化路径、也不在这儿兜底名字**：那两样是库屏加根那个函数的活
/// （[`crate::roots::Screen::add_root`]），在这儿先折一遍就成了第二套算法。
#[derive(Debug, Clone, Default)]
pub struct FirstRoot {
    /// 那块盘上的目录，人填的那一串。
    pub path: String,
    /// 这个根叫什么。**空的**就是「按目录自己的名字取」。
    pub name: String,
}

/// 认领一个新主库那条向导。
pub struct Wizard {
    /// 认领进哪个**工作目录**。
    workspace: PathBuf,
    asking: Asking,
    /// 主库名。**只活在这儿**，直到按下「开始扫描」。
    name: String,
    /// 第一个根：那个目录，与它叫什么。同上，**只活在这儿**。
    ///
    /// 攒成走完之后交出去的那个形状，而不是两串散着的字：这两样从第二步的两个框起,
    /// 到库屏加根那一下为止一路同行（[`crate::app::App::claim_first_root`]）。
    first: FirstRoot,
    /// 上一下被拦下时说的那句话。
    error: Option<String>,
}

impl Wizard {
    /// 开一条向导，认领进这个工作目录。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            asking: Asking::Name,
            name: String::new(),
            first: FirstRoot::default(),
            error: None,
        }
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui) -> Outcome {
        ui.strong("认领一个新主库");
        let out = match self.asking {
            Asking::Name => {
                self.name_ui(ui);
                Outcome::Going
            }
            Asking::Root => self.root_ui(ui),
        };
        // **走完了就到此为止**：这一帧是向导画的最后一帧，底下那句话与那颗「算了」都没有
        // 下一帧可看了。摆这一句还有第二个理由——**一帧里不许既走完又算了**：那时候库已经
        // 建出来了，而「算了」那一支说的是「磁盘上什么都没留下」。
        if !matches!(out, Outcome::Going) {
            return out;
        }
        // **那句话画在两个框底下**：它是**按下去之后**才知道的，而这一帧的按下就发生在
        // 上面那几行画完的那一刻——画在上头的话，人得多等一帧才看见它。
        if let Some(说的) = &self.error {
            ui.add_space(6.0);
            ui.colored_label(ui.visuals().error_fg_color, 说的);
        }
        ui.add_space(6.0);
        if ui.button("算了").clicked() {
            return Outcome::Dropped;
        }
        Outcome::Going
    }

    /// 第一步：起名。
    fn name_ui(&mut self, ui: &mut egui::Ui) {
        ui.weak(
            "第一步：给这个主库起个名字。\
             换了挂载点、盘符变了，靠这个名字还能找回同一份中立库",
        );
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.name)
                    .hint_text(NAME_HINT)
                    .desired_width(320.0),
            );
            if ui.button("下一步").clicked() {
                self.named();
            }
        });
    }

    /// 第二步：选第一个根。**「开始扫描」那颗就在这一步上**——第三步没有自己的一屏。
    fn root_ui(&mut self, ui: &mut egui::Ui) -> Outcome {
        let mut out = Outcome::Going;
        ui.weak(format!(
            "第二步：给「{}」选第一个根。主库是一组根——往后再加第二块盘从库屏加",
            self.name.trim(),
        ));
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.first.path)
                    .hint_text(ROOT_HINT)
                    .desired_width(320.0),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.first.name)
                    .hint_text(ROOT_NAME_HINT)
                    .desired_width(180.0),
            );
        });
        ui.horizontal(|ui| {
            if ui
                .button("开始扫描")
                .on_hover_text("这一下才真的开出中立库：在这之前工作目录里一个文件都不多")
                .clicked()
            {
                out = self.claim();
            }
            if ui.button("上一步").clicked() {
                self.asking = Asking::Name;
                self.error = None;
            }
        });
        out
    }

    /// 按下「开始扫描」那一下。**两段，次序是要紧的。**
    ///
    /// 1. **先把加根那一条判断走一遍**，走的是[库屏加根那个函数本人](crate::roots::Screen::add_root)
    ///    ——只是这一趟落在一份[只活在内存里的现场](在内存里试一遍)上。
    /// 2. 过了才真的开中立库。**开这一下就是建库**（[`Catalog::open_named`] 打开即创建）。
    ///
    /// **反过来的话，被拦下的那一次会留下一份零根空库**：它对人没有用处，却会永久占着
    /// 开场的一行，而开场没有删库那条路。更难受的是人接着重走一遍向导——第一步就会撞上
    /// 自己刚才留下的那个名字。
    fn claim(&mut self) -> Outcome {
        let name = self.name.trim().to_string();
        let slug = Slug::Named(&name);
        match self
            .在内存里试一遍()
            .and_then(|()| self.开出那份现场(&slug))
        {
            Ok(site) => {
                self.error = None;
                Outcome::Done(Box::new(Claimed {
                    site,
                    root: self.first.clone(),
                }))
            }
            // 拦下时说的是**核心库那句原话**，一个字都不改写（ADR-0005）：人在库屏上
            // 加根撞上同一条时看见的是同一句话。
            Err(说的) => {
                self.error = Some(说的);
                Outcome::Going
            }
        }
    }

    /// 第一段：**在一份只活在内存里的库上把加根走一遍**，拦下时交出那句话。
    ///
    /// 走的是[库屏加根那个函数本人](crate::roots::add_root_from_fields)，只是这一趟
    /// 递给它的是一份内存库。**那份库与真落盘那一份在加根这件事上一模一样**：两份都
    /// 是刚开出来、一个根都没有的库，而
    /// [`roots::add_root`](romcat_core::catalog::roots::add_root) 拦的几条里，问「库里
    /// 已经有哪些根」的三条（根名重复、落在已有的根里面、把已有的根圈进去）在两份上
    /// 都是同一个答案，余下两条问的是根名本身与**工作目录**——而工作目录这一趟从头到尾
    /// 就是同一个。
    fn 在内存里试一遍(&self) -> Result<(), String> {
        // 开一份内存库这一下不碰磁盘，也几乎不会失败（走到这儿说明 SQLite 自己出了事）。
        let 试 = Catalog::open_in_memory().map_err(|error| format!("这个根试不了：{error}"))?;
        crate::roots::add_root_from_fields(&试, &self.workspace, &self.first.path, &self.first.name)
            .map(|_| ())
    }

    /// 第二段：**这一下才落盘**——建出中立库，开出那份现场。
    ///
    /// 走的是核心库那两条现成的路：[`Catalog::open_named`] 把人起的那个名字记进元数据表
    /// （不记的话，那个名字只活在这一刻——中立库的文件名折过一道滤字符、截断、缀哈希，
    /// 谁也从那串字里认不回来），[`Site::open_file`] 把中立库与**沉淀库**一起开出来。
    /// **与命令行 `romcat scan` 建库走的是同一条**（`open_catalog`），所以向导建出来的
    /// 库命令行接得上。
    ///
    /// **开出来那一步失手的话，刚建出来的那一份跟着一起收掉。** 那一步开的是**两份**
    /// 库（中立库刚建出来，而**沉淀库**是这个工作目录里共用的那一份，它可能压根开不动），
    /// 留下来的就是一份零根空库——它对人没有用处，却会永久占着开场的一行。
    fn 开出那份现场(&self, slug: &Slug<'_>) -> Result<Site, String> {
        let path = workspace::catalog_path(&self.workspace, *slug);
        // **这一刻它还不在**（第一步查过重名）。记下这一笔：底下那一步失手时，该收掉的
        // 只有「这一趟建出来的」那一份——万一它本来就在，一个字节都不许动。
        let 本来不在 = !path.exists();
        // 建完就把它放下：底下那一句要按同一个文件把**现场**整个开出来，一份连接够了。
        drop(
            Catalog::open_named(&path, &slug.display_name())
                .map_err(|error| format!("这份中立库建不出来：{error}"))?,
        );
        Site::open_file(&self.workspace, &path, None).map_err(|error| {
            if 本来不在 {
                收掉刚建的(&path);
            }
            format!("{error}")
        })
    }

    /// 起名那一下：**重名在这一步就拦**。
    fn named(&mut self) {
        let name = self.name.trim();
        if name.is_empty() {
            self.error = Some("先给这个主库起个名字。".to_string());
            return;
        }
        // **靠工作目录里那个文件在不在，而不是等落盘时才发现**（ADR-0023）。折文件名
        // 走的是核心库那条算法本人（[`workspace::catalog_path`] + [`Slug::Named`]），
        // 与按下「开始扫描」那一下建出来的是同一个文件——两处各折一遍的话，拦得住的
        // 与建出来的就不是一回事了。
        let 那份 = workspace::catalog_path(&self.workspace, Slug::Named(name));
        if 那份.exists() {
            self.error = Some(format!(
                "这个工作目录里已经有一份叫「{name}」的主库了（{}）。\
                 换个名字，或者回去把那一份开进来。",
                path::display(&那份),
            ));
            return;
        }
        self.error = None;
        self.asking = Asking::Root;
    }
}

/// 把刚建出来那一份中立库连 SQLite 自己那两个附件一起删掉。
///
/// **只在建完、却没开成现场的那一步走**：那时手上一条连接都没有（建的那一份已经放下，
/// 开现场那一趟的两份都随着那个 `Err` 一起没了），删得干净。
///
/// **删不掉就作罢**：人看见的仍旧是开不出现场那句话，只是开场那一行上会多出一份空库
/// ——而在这儿再报一句「顺带说一声，那份库也没删掉」，只会盖住真正的那句。
fn 收掉刚建的(path: &Path) {
    let _ = std::fs::remove_file(path);
    for 尾 in ["-wal", "-shm"] {
        let mut 附件 = path.as_os_str().to_os_string();
        附件.push(尾);
        let _ = std::fs::remove_file(PathBuf::from(附件));
    }
}
