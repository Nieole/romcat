//! **添加主库**：[开场](crate::opening)上那条一条龙向导。
//!
//! 三问——给**主库**起名 → 选第一个**根** → 看清扫描读哪儿、写哪儿，开扫。走完直接进主窗口，
//! 扫描已经排在**任务台**上跑着（ADR-0023：界面自足，从第一步起不必开终端）。
//!
//! ## 一条领域判断都没有
//!
//! 加一个根要拦的三种情况（根名重复、与已有的根套在一起、圈进**工作目录**）全在
//! [`romcat_core::catalog::roots::add_root`]，而这一层连那个函数都不直接调——它调的是
//! **库屏加根那个函数本人**（[`crate::roots::add_root_from_fields`]）。向导重画 UI，
//! 判断是同一个；两套判断只会分叉出两套行为（ADR-0005 的红线）。
//!
//! ## 当场就说收不收
//!
//! 起名那一框、选根那两框，**字一变就问一遍**，不等按「下一步」（设计稿「名称与目录当场校验」）。
//! 问的仍是那两处：名字问核心库建库入口（[`Catalog::refuse_create`]），根问库屏加根那个函数
//! （落在一份只活在内存里的库上）。**问一遍一个字节都不写。**
//!
//! 框空着时不问：人还没开口，先摆一句「不能是空白」是抢话。按「下一步」时照问，空着也问。
//!
//! ## 晚落盘
//!
//! 前两问**只在内存里攒**：这个类型手上从头到尾只有三串字。按下「开始扫描」那一下才真的建
//! 中立库（[`Catalog::create`]，明说在建的那个入口）。所以中途关窗、按「算了」，工作目录里
//! 一个文件都不多。
//!
//! 零根的库仍然是合法状态（核心库已经建模并测过）。晚落盘不是因为零根非法，而是因为
//! **一份零根空库对人没有用处，却会永久占着开场的一行**——而开场没有删库那条路。
//!
//! 这也是「按下开始扫描之后，加根那一条判断先在一份只活在内存里的库上走一遍」的理由：
//! 拦下的时候磁盘上什么都还没有，人改一改接着来。先建后拦的话，一份零根空库就留在那儿
//! 了，而且人再走一遍向导还会在第一步撞上自己刚留下的那个名字。

use std::path::{Path, PathBuf};

use romcat_core::catalog::Catalog;
use romcat_core::catalog::roots::LibraryRoot;
use romcat_core::path;
use romcat_core::site::Site;
use romcat_core::workspace::{self, Slug};

use crate::dialog::{Button, Dialog, Footer};
use crate::font;
use crate::look::{self, Tone};
use crate::roots::{ROOT_HINT, ROOT_NAME_HINT};
use crate::tokens::Tokens;

/// 起名那一问框里的提示字。
const NAME_HINT: &str = "主库原名，例如「主库」";

/// 弹层标题底下那句说明：一共几问、哪一下才落盘。
const NOTE: &str = "共三步。点「开始扫描」之前一个文件都不写，随时可以退出";

/// 标头那一排三问的名字（设计稿 `.steps`）。
const PAGES: [&str; 3] = ["命名", "选择目录", "开始扫描"];

/// 向导页脚上按下去的是哪一颗。
enum Pressed {
    /// 「算了」：退出那一颗，Esc 等于按它。
    Drop,
    /// 「下一步」：前两问上。
    Next,
    /// 「上一步」：后两问上。
    Back,
    /// 「开始扫描」：第三问上。
    Claim,
}

/// 向导这会儿问的是哪一样。
///
/// **不叫 `Step`**：词表**工序**那一条的 `_Avoid_` 里逐字列着「步骤」，而这个仓库里
/// `Stage` 已经是**工序**（`romcat_core::stage::Stage`）。屏上照旧说「共三步」——那是设计稿
/// 自己的话，说的是这条向导，不是工序。
enum Asking {
    /// 这个主库叫什么。
    Name,
    /// 第一个根在哪儿、叫什么。
    Root,
    /// 扫描读哪儿、写哪儿。**「开始扫描」那颗在这一问上。**
    ///
    /// 从前这一问没有自己的一屏（「中间没有任何新信息，单摆一屏只让人多按一下」）。设计稿把它
    /// 画成了第三页，因为它**有**新信息：前两问填的是字，这一问把那几串字折成人要背书的那几件事
    /// ——读的是哪块盘上的什么、写的是工作目录里哪几个文件（票 `gui-looks-like-the-design/05`）。
    Scan,
}

impl Asking {
    /// 这是第几问（从 0 数）：标头那一排照它亮。
    const fn at(&self) -> usize {
        match self {
            Self::Name => 0,
            Self::Root => 1,
            Self::Scan => 2,
        }
    }
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FirstRoot {
    /// 那块盘上的目录，人填的那一串。
    pub path: String,
    /// 这个根叫什么。**空的**就是「按目录自己的名字取」。
    pub name: String,
}

/// 添加主库那条向导。
pub struct Wizard {
    /// 添加进哪个**工作目录**。
    workspace: PathBuf,
    asking: Asking,
    /// **主库原名**。**只活在这儿**，直到按下「开始扫描」。
    name: String,
    /// 第一个根：那个目录，与它叫什么。同上，**只活在这儿**。
    ///
    /// 攒成走完之后交出去的那个形状，而不是两串散着的字：这两样从第二问的两个框起,
    /// 到库屏加根那一下为止一路同行（[`crate::app::App::claim_first_root`]）。
    first: FirstRoot,
    /// 起名那一框**上一回问核心库**问的是哪一串、怎么答的。那一串与框里眼下的对不上，就该再问了。
    name_said: Option<(String, Result<(), String>)>,
    /// 选根那两框上一回问的是哪两串、怎么答的。答得上时交回加根那一处**化好的根**——第三问照它
    /// 写根叫什么、在哪儿，不在这儿另折一遍。
    root_said: Option<(FirstRoot, Result<LibraryRoot, String>)>,
    /// 按「开始扫描」那一下被拦下时说的那句话。
    error: Option<String>,
}

impl Wizard {
    /// 开一条向导，添加进这个工作目录。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            asking: Asking::Name,
            name: String::new(),
            first: FirstRoot::default(),
            name_said: None,
            root_said: None,
            error: None,
        }
    }

    /// 画一帧：**一层弹层**（[`crate::dialog`]）。标题底下是那句说明与三问那一排，内容区是这一问，
    /// 页脚是「算了 ｜ 上一步 · 下一步 / 开始扫描」。Esc 等于「算了」。
    ///
    /// **一帧里只会按下一颗**（弹层一帧只交回一个动作），于是「一帧里既走完又算了」在结构上
    /// 撞不到——那时候库已经建出来了，而「算了」那一支说的是「磁盘上什么都没留下」。
    pub fn show(&mut self, ctx: &egui::Context) -> Outcome {
        let 算了 = || Button::new("算了", Pressed::Drop);
        let 下一步 = || Button::new("下一步", Pressed::Next).primary();
        let footer = match self.asking {
            Asking::Name => Footer::new(算了()).button(下一步()),
            Asking::Root => Footer::new(算了())
                .button(Button::new("上一步", Pressed::Back))
                .button(下一步()),
            Asking::Scan => Footer::new(算了())
                .button(Button::new("上一步", Pressed::Back))
                .button(
                    Button::new("开始扫描", Pressed::Claim)
                        .primary()
                        .hover("这一下才真的开出中立库：在这之前工作目录里一个文件都不多"),
                ),
        };
        let shown = Dialog::new("添加主库", "添加主库", footer)
            .note(NOTE)
            .pages(PAGES, self.asking.at())
            .show(ctx, |ui| self.fields_ui(ui));
        match shown.pressed {
            None => Outcome::Going,
            Some(Pressed::Drop) => Outcome::Dropped,
            Some(Pressed::Next) => {
                self.next();
                Outcome::Going
            }
            Some(Pressed::Back) => {
                self.back();
                Outcome::Going
            }
            Some(Pressed::Claim) => self.claim(),
        }
    }

    /// 「下一步」：**这一问收得下才往下走**，收不下就停在这一问上，那句话画在框底下。
    ///
    /// 按下去时照问一遍，不看上一回问过没有：框空着时当场不问（见模块文档），而按下去的人要的
    /// 正是一句「为什么走不下去」。
    fn next(&mut self) {
        match self.asking {
            Asking::Name => {
                self.ask_name();
                if matches!(self.name_said, Some((_, Ok(())))) {
                    self.asking = Asking::Root;
                }
            }
            Asking::Root => {
                self.ask_root();
                if matches!(self.root_said, Some((_, Ok(_)))) {
                    self.asking = Asking::Scan;
                }
            }
            Asking::Scan => {}
        }
    }

    /// 「上一步」。框里攒着的字原样留着。
    fn back(&mut self) {
        self.error = None;
        self.asking = match self.asking {
            Asking::Scan => Asking::Root,
            Asking::Root | Asking::Name => Asking::Name,
        };
    }

    /// 内容区：这一问的那几样。
    ///
    /// **按「开始扫描」被拦下时那句话是按下去之后才知道的**，而页脚那一下要等内容区画完才交回来
    /// ——于是它落在下一帧上。弹层按下任何一颗都会要一次重画，人看见的仍旧是「按下去就说」。
    fn fields_ui(&mut self, ui: &mut egui::Ui) {
        match self.asking {
            Asking::Name => self.name_ui(ui),
            Asking::Root => self.root_ui(ui),
            Asking::Scan => self.scan_ui(ui),
        }
    }

    /// 第一问：起名。框底下当场说收不收，再写清这份中立库会建在哪儿。
    fn name_ui(&mut self, ui: &mut egui::Ui) {
        field_label(ui, "主库名称");
        let 一栏宽 = ui.available_width();
        look::text_input(
            ui,
            一栏宽,
            egui::TextEdit::singleline(&mut self.name).hint_text(NAME_HINT),
        );
        self.refresh_name();
        if let Some((_, 答)) = &self.name_said {
            said_line(
                ui,
                答.as_ref().map(|()| "名称可用。").map_err(String::as_str),
            );
        }
        look::help(
            ui,
            "给这个主库起个名字。换了挂载点、盘符变了，靠这个名字还能找回同一份中立库。",
        );
        ui.add_space(look::step(2));
        // 名字空着时折不出那个文件——不画一个按空名字折出来的路径。
        if !self.name.trim().is_empty() {
            let 中立库 = workspace::catalog_path(&self.workspace, self.slug());
            key_value(ui, "中立库", |ui| {
                ui.label(font::mono(path::display(&中立库)));
            });
        }
        key_value(ui, "工作目录", |ui| {
            ui.label(font::mono(path::display(&self.workspace)));
        });
    }

    /// 第二问：选第一个根。框底下常驻那句退路（挂单 `Q695`），当场说这个目录收不收。
    fn root_ui(&mut self, ui: &mut egui::Ui) {
        look::help(
            ui,
            &format!(
                "给「{}」选第一个根。主库是一组根——往后再加第二块盘从库屏加",
                self.name.trim(),
            ),
        );
        ui.add_space(look::step(2));
        field_label(ui, "ROM 所在目录");
        ui.horizontal(|ui| {
            let 留给按钮 = look::button_width(ui, "选择…") + ui.spacing().item_spacing.x;
            let 框宽 = (ui.available_width() - 留给按钮).max(0.0);
            // 与旁边那颗默认档的「选择…」等高；宽占满留给它的那一截。
            look::text_input(
                ui,
                框宽,
                egui::TextEdit::singleline(&mut self.first.path)
                    .hint_text(ROOT_HINT)
                    .font(egui::TextStyle::Monospace),
            );
            // 系统目录选择器（[`crate::pick`]）：选中的目录落进左边这个框，与贴路径同一处。
            if ui.button("选择…").clicked() {
                self.picked_root(crate::pick::directory(
                    "选第一个根",
                    &self.root_pick_start(),
                ));
            }
        });
        // **弹不出选择窗口时的退路常驻在框底下**，不靠悬停（挂单 `Q695`）：弹不出来与取消交回的是
        // 同一个 `None`，按下去什么都不发生时人得当场看得见还有这个框可贴。
        look::help(ui, crate::pick::FALLBACK_HINT);
        ui.add_space(look::step(2));
        field_label(ui, "根名称");
        let 一栏宽 = ui.available_width();
        look::text_input(
            ui,
            一栏宽,
            egui::TextEdit::singleline(&mut self.first.name).hint_text(ROOT_NAME_HINT),
        );
        self.refresh_root();
        match &self.root_said {
            Some((_, Ok(根))) => {
                said_line(ui, Ok(&format!("这个目录收得下，根名「{}」。", 根.name)))
            }
            Some((_, Err(说的))) => said_line(ui, Err(说的)),
            None => {}
        }
        ui.add_space(look::step(2));
        look::read_only(ui, "只读访问，不会写入这个根");
    }

    /// 第三问：扫描读哪儿、写哪儿。**写哪儿**全是核心库折出来的那几条路径，与按下「开始扫描」
    /// 之后真写的是同几个文件。
    fn scan_ui(&self, ui: &mut egui::Ui) {
        // 走得到这一问，说明选根那一问上刚问过、答得上（[`Self::next`]）；框在这一问上改不了。
        let Some((_, Ok(根))) = &self.root_said else {
            return;
        };
        let slug = self.slug();
        let 中立库 = workspace::catalog_path(&self.workspace, slug);
        key_value(ui, "主库", |ui| {
            ui.label(self.name.trim());
        });
        key_value(ui, "根", |ui| {
            ui.label(&根.name);
            ui.label(font::mono(&根.path));
        });
        key_value(ui, "读取", |ui| {
            ui.label("这个目录底下每个文件的大小、修改时间与文件头，以及 zip、7z 这类透明容器里的文件列表。");
            look::read_only(ui, "只读");
        });
        key_value(ui, "写入", |ui| {
            ui.label(font::mono(path::display(&中立库)));
            look::help(
                ui,
                &format!(
                    "同一个工作目录里还会写：裁决记录 {}，扫描断点 {}",
                    path::display(&workspace::verdict_store_path(&self.workspace)),
                    path::display(&workspace::checkpoint_path(&self.workspace, slug, &根.name)),
                ),
            );
        });
        key_value(ui, "预计耗时", |ui| {
            ui.label(
                "约 10 TB 的库，首次扫描实测 37 分钟。扫描前先测这块盘的读取速度，据此定并发数。",
            );
        });
        ui.add_space(look::step(2));
        look::note(
            ui,
            egui::RichText::new(
                "扫描在任务台上跑，不耽误别的操作。中途关窗也不要紧：下次扫描从断点接着走。",
            )
            .small(),
        );
        if let Some(说的) = &self.error {
            ui.add_space(look::step(1));
            ui.colored_label(ui.visuals().error_fg_color, 说的);
        }
    }

    /// 起名那一框里的字变了就再问一遍；框空着不问（见模块文档「当场就说收不收」）。
    fn refresh_name(&mut self) {
        let 问过这一串 = self
            .name_said
            .as_ref()
            .is_some_and(|(问的, _)| *问的 == self.name);
        if 问过这一串 {
            return;
        }
        if self.name.is_empty() {
            self.name_said = None;
        } else {
            self.ask_name();
        }
    }

    /// 选根那两框里的字变了就再问一遍；目录那一框空着不问。
    fn refresh_root(&mut self) {
        let 问过这两串 = self
            .root_said
            .as_ref()
            .is_some_and(|(问的, _)| *问的 == self.first);
        if 问过这两串 {
            return;
        }
        if self.first.path.is_empty() {
            self.root_said = None;
        } else {
            self.ask_root();
        }
    }

    /// 问核心库：**这个名字收不收**。
    ///
    /// 问的是核心库建库入口那一处（[`Catalog::refuse_create`]，挂单 `Q524`）：名字是不是空白、
    /// 撞没撞这个工作目录里另一份库的主库原名、按这个名字折出来的那份库在不在、这个工作目录
    /// 写不写得进——**问一遍一个字节都不写**（ADR-0023 的晚落盘）。折文件名走的是核心库那条
    /// 算法本人（[`workspace::catalog_path`] + [`Slug::Named`]），与按下「开始扫描」那一下建出来
    /// 的是同一个文件。向导自己另判一份的话，这一步拦得住的与建库时拦的迟早不是一回事（ADR-0024）。
    ///
    /// 拦下时说的是**核心库那句原话**，一个字都不改写（ADR-0005）。
    fn ask_name(&mut self) {
        let slug = self.slug();
        let 那份 = workspace::catalog_path(&self.workspace, slug);
        let 答 =
            Catalog::refuse_create(&那份, &slug.display_name()).map_err(|error| format!("{error}"));
        self.name_said = Some((self.name.clone(), 答));
    }

    /// 问加根那一处：**这个目录、这个根名收不收**，落在一份只活在内存里的库上。
    fn ask_root(&mut self) {
        let 答 = self.在内存里试一遍();
        self.root_said = Some((self.first.clone(), 答));
    }

    /// 按下「开始扫描」那一下。**两段，次序是要紧的。**
    ///
    /// 1. **先把加根那一条判断再走一遍**，走的是[库屏加根那个函数本人](crate::roots::add_root_from_fields)
    ///    ——只是这一趟落在一份[只活在内存里的现场](在内存里试一遍)上。选根那一问上问过，可那是
    ///    人看第三问之前的事，盘在这中间可能被拔了。
    /// 2. 过了才真的建中立库（[`Catalog::create`]）。
    ///
    /// **反过来的话，被拦下的那一次会留下一份零根空库**：它对人没有用处，却会永久占着
    /// 开场的一行，而开场没有删库那条路。更难受的是人接着重走一遍向导——第一步就会撞上
    /// 自己刚才留下的那个名字。
    fn claim(&mut self) -> Outcome {
        match self.在内存里试一遍().and_then(|_| self.开出那份现场()) {
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

    /// **在一份只活在内存里的库上把加根走一遍**：收得下交回化好的根，拦下时交出那句话。
    ///
    /// 走的是[库屏加根那个函数本人](crate::roots::add_root_from_fields)，只是这一趟
    /// 递给它的是一份内存库。**那份库与真落盘那一份在加根这件事上一模一样**：两份都
    /// 是刚开出来、一个根都没有的库，而
    /// [`roots::add_root`](romcat_core::catalog::roots::add_root) 拦的几条里，问「库里
    /// 已经有哪些根」的三条（根名重复、落在已有的根里面、把已有的根圈进去）在两份上
    /// 都是同一个答案，余下两条问的是根名本身与**工作目录**——而工作目录这一趟从头到尾
    /// 就是同一个。
    fn 在内存里试一遍(&self) -> Result<LibraryRoot, String> {
        // 开一份内存库这一下不碰磁盘，也几乎不会失败（走到这儿说明 SQLite 自己出了事）。
        let 试 = Catalog::open_in_memory().map_err(|error| format!("这个根试不了：{error}"))?;
        crate::roots::add_root_from_fields(&试, &self.workspace, &self.first.path, &self.first.name)
    }

    /// 第二段：**这一下才落盘**——建出中立库，开出那份现场。
    ///
    /// 走的是核心库那两条现成的路：[`Catalog::create`] 建库、把人起的那个名字记进元数据表
    /// （不记的话，那个名字只活在这一刻——中立库的文件名折过一道滤字符、截断、缀哈希，
    /// 谁也从那串字里认不回来），[`Site::open_file`] 把中立库与**沉淀库**一起开出来。
    /// **与命令行 `romcat scan` 建库调的是同一个入口**，所以向导建出来的库命令行接得上；
    /// 名字收不收（空白、撞没撞同一个工作目录里另一份库的主库原名）也只在那儿判一次，
    /// 拦下时说的是核心库那句原话。
    ///
    /// **开出来那一步失手的话，刚建出来的那一份跟着一起收掉。** 那一步开的是**两份**
    /// 库（中立库刚建出来，而**沉淀库**是这个工作目录里共用的那一份，它可能压根开不动），
    /// 留下来的就是一份零根空库——它对人没有用处，却会永久占着开场的一行。
    fn 开出那份现场(&self) -> Result<Site, String> {
        let slug = self.slug();
        let path = workspace::catalog_path(&self.workspace, slug);
        // 建完就把它放下：底下那一句要按同一个文件把**现场**整个开出来，一份连接够了。
        // **建得成就说明它本来不在**：那个文件已经在时 `create` 当场报错、一个字节不碰，
        // 于是底下那一步失手时收掉的只会是这一趟建出来的那一份。
        drop(
            Catalog::create(&path, &slug.display_name())
                .map_err(|error| format!("这份中立库建不出来：{error}"))?,
        );
        Site::open_file(&self.workspace, &path, None).map_err(|error| {
            收掉刚建的(&path);
            format!("{error}")
        })
    }

    /// 人起的这个名字认的是哪一份库：框里那串字去掉两头空白，按名字折（[`Slug::Named`]）。
    ///
    /// **起名那一问、第三问、「开始扫描」那一下问的都是这一处**：几处各折一遍的话，前面查过的
    /// 与真建出来的迟早不是同一个文件。
    fn slug(&self) -> Slug<'_> {
        Slug::Named(self.name.trim())
    }
}

/// 字段上头那个名字（设计稿 `.field label`）：说明字号、正文色。
fn field_label(ui: &mut egui::Ui, text: &str) {
    let 正文 = ui.visuals().widgets.noninteractive.fg_stroke.color;
    ui.label(egui::RichText::new(text).small().color(正文));
}

/// 当场问过的那一句：收得下是一行好事颜色的字，收不下是那句原话、出错色。
fn said_line(ui: &mut egui::Ui, 答: Result<&str, &str>) {
    let (color, text) = match 答 {
        Ok(text) => (look::tone_colors(Tone::Good, ui.visuals()).0, text),
        Err(text) => (ui.visuals().error_fg_color, text),
    };
    ui.label(egui::RichText::new(text).small().color(color));
}

/// 「名 → 值」一行（设计稿 `.kv`）：左边一列定宽的弱字，右边那一格由调用方画。
fn key_value(ui: &mut egui::Ui, key: &str, value: impl FnOnce(&mut egui::Ui)) {
    let width = Tokens::builtin().layout.kv_key_width;
    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(width, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                ui.label(egui::RichText::new(key).small().weak());
            },
        );
        ui.vertical(value);
    });
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

impl Wizard {
    /// **目录选择器交回来的那一下**（[`crate::pick`]）：选中了一个目录，就把它落进选根那个框
    /// ——贴路径那条路填的正是这一处（[`FirstRoot::path`]）；取消了（`None`）什么都不动。
    ///
    /// **只填框，不替人按「下一步」**：选中之后那一问照旧往下走，人还要看一眼、起个根名再按。
    /// 化路径、按目录名兜底根名、拦下那几种情况，照旧全是加根那一处的事
    /// （[`crate::roots::add_root_from_fields`]）——框里的字一变，下一帧当场问它一遍。
    ///
    /// **按原样落成一串字**，不走 [`romcat_core::path::display`]：那一个是展示用的，
    /// 而这串字等会儿要被当成路径再去碰盘。
    ///
    /// 这一半与弹对话框那一层分开摆，是为了**它测得到**：对话框是系统的模态窗口，
    /// 测试驱动不了；拿到路径之后发生什么，测试递一个进来就验得着。
    pub fn picked_root(&mut self, 选中的: Option<PathBuf>) {
        let Some(目录) = 选中的 else {
            return;
        };
        self.first.path = 目录.to_string_lossy().into_owned();
    }

    /// 选根那个对话框从哪儿打开：**上次选过的位置，没有就用工作目录**。
    ///
    /// **「上次选过的位置」就是选根那个框里眼下那一串**：选中的那一下落进的正是它
    /// （[`Self::picked_root`]），不另记一份，也不落盘（挂单 `Q692`）。框里是人自己贴进去的一个
    /// 目录时同样从那儿打开——那也是人上一次说的「在这附近」。框里不是一个目录（空的、半截、
    /// 打错的）时退回工作目录。
    fn root_pick_start(&self) -> PathBuf {
        let 框里的 = Path::new(self.first.path.trim());
        if 框里的.is_dir() {
            框里的.to_path_buf()
        } else {
            self.workspace.clone()
        }
    }
}
