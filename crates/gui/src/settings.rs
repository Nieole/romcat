//! **设置**那一屏：八节摆在一处（票 `gui-looks-like-the-design/31`，设计稿 `.sets`）。
//!
//! 左边一列八节，右边是那一节的内容：常规、工作目录、数据源、刮削、导出、工具、快捷键、关于。
//! **它是一屏，不是一层弹层**（规格实现决定三）：与另外五屏同级，走同一套左栏入口、同一个屏头，
//! `⌘/Ctrl+,` 也打得开（[`crate::app::App`] 那一处）。
//!
//! ## 这一层只摆、只转发（ADR-0005、ADR-0024）
//!
//! | 屏上那件事 | 核心库里是谁 |
//! |---|---|
//! | 主库原名是什么 | `Catalog::library_name` |
//! | 改名收不收、为什么不收 | `Catalog::set_library_name`（空白、重名、目录列不开三种，话也是它说的） |
//! | 换工作目录收不收 | `Roots::refuse_writing_into`（主库只读，ADR-0004） |
//! | 数据源各多少条、上次什么时候取的 | `sources::survey` |
//! | ffmpeg 在不在、是哪一版 | `scrape::preview::probe` |
//! | 认得哪几种前端格式 | `adapter::names` |
//! | 这个库导出到哪、导出成什么 | `Catalog::export_setup` |
//! | 库文件结构版本 | `catalog::SCHEMA_VERSION` |
//!
//! **这一层里一条领域判断都没有**：屏上那几句拒绝的话是核心库说的，这一层只把它印出来。
//!
//! ## 稿上有、今天接不上的那几格：照实写，不画假开关
//!
//! 设计稿在这一屏上画了六颗开关（每周自动检查更新、启动时直接打开上次那份、刮削下载媒体、
//! 导出时铺媒体……）。**这几样今天一个都没有落点**：数据源没有「自动检查」这条路，刮削面板
//! 那几个旋钮每次开窗从头起、一个都不落盘（挂单 `Q652` 明写导出铺媒体那颗「只记在那一段上、
//! 不记进库」），ScreenScraper 的账号只认启动前给的环境变量、配额只在一趟联网刮削跑着的时候
//! 报得出。
//!
//! 画一颗拨得动、关了窗就忘的开关，等于在屏上写一件做不到的事。所以这几格**摆的是一句实话
//! 加一个去处**——今天是怎么回事、要办这件事该上哪儿。挂单 `Q1062`–`Q1066` 记着每一格差的是
//! 什么，交给拿主意的人裁要不要各开一张票。
//!
//! ## 版式：一行两列，列是立起来的
//!
//! 一行（设计稿 `.srow`）是**名一列、值一列**：名那一列宽取令牌 `settings-row-label`，
//! **不跟着字长走**。跟着字长走的话，「界面语言」那一行的值就会比「外观」那一行往右挪，
//! 行与行必然参差。值那一列里的东西一律从同一条线起（`Screen::一行`），数字那几列右对齐
//! （`Screen::占用表`、`Screen::数据源表` 两张表），行底下那道线与页签条一样**横贯整个内容区**。
//! 截图门那几条钉着这三样（`tests/snapshot.rs`）。

use std::path::PathBuf;

use romcat_core::report::thousands;
use romcat_core::scrape::preview;
use romcat_core::site::Site;
use romcat_core::sources::{self, SourceState, SourceStatus};

use crate::tokens::Tokens;
use crate::{font, keys, look, pick, priority};

/// 左边那一列，八节，照设计稿的次序。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Section {
    /// 外观、主库原名、界面语言。
    #[default]
    General,
    /// 工作目录与媒体池在哪、里头装着什么。
    Workspace,
    /// 本机那几份数据源、数据源优先级、ScreenScraper。
    Sources,
    /// 刮削那几个旋钮的默认那一档。
    Scrape,
    /// 导出成什么、导出到哪、铺不铺媒体、撞上外部修改怎么办。
    Export,
    /// ffmpeg 与压缩格式。
    Tools,
    /// 快捷键表。
    Keys,
    /// 版本、库文件结构版本、数据源与许可。
    About,
}

impl Section {
    /// 全部八节。
    pub const ALL: [Self; 8] = [
        Self::General,
        Self::Workspace,
        Self::Sources,
        Self::Scrape,
        Self::Export,
        Self::Tools,
        Self::Keys,
        Self::About,
    ];

    /// 左边那一列上写的字。用**词表**里的词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::General => "常规",
            Self::Workspace => "工作目录",
            Self::Sources => "数据源",
            Self::Scrape => "刮削",
            Self::Export => "导出",
            Self::Tools => "工具",
            Self::Keys => "快捷键",
            Self::About => "关于",
        }
    }
}

/// 屏头上那句副标题（设计稿 `.scrhead .sub`）。
pub const SUBTITLE: &str = "工作目录、数据源、刮削与导出的默认值";

/// 「外观」那一档记在版式文件里的键（[`crate::layout::Layout::preference`]）。
pub const THEME_KEY: &str = "外观";

/// 「外观」三档写在屏上的字，与记进版式文件的值是同一个。
const THEMES: [(egui::ThemePreference, &str); 3] = [
    (egui::ThemePreference::System, "跟随系统"),
    (egui::ThemePreference::Light, "浅色"),
    (egui::ThemePreference::Dark, "深色"),
];

/// 改名那颗按钮上的字。
pub const RENAME: &str = "改名";
/// 改名那一格底下常驻的那句话（票面：屏上要说明改名不动路径锚）。
pub const RENAME_NOTE: &str =
    "改名不动路径锚：认库靠的是主库标识，改的只是给人看的那个名字，裁决照旧对得上。";
/// 换工作目录那颗按钮上的字。
pub const CHANGE: &str = "更换…";
/// 换工作目录那一格底下常驻的那句话。
pub const WORKSPACE_NOTE: &str =
    "工作目录不能放在主库的根之内：主库只读，中立库、裁决记录、媒体池一样都不许落进去。";
/// 数据源优先级那颗按钮上的字。
///
/// 照稿写「数据源优先级…」，与刮削面板那一颗上的 [`priority::OPEN`]（「调整优先级…」）不同字——
/// 稿上这两处本来就是两句话：那一处紧挨着「不该靠重采」那句，说的是「想换个显示的值，调这张表」；
/// 这一处在数据源那一节里，得自己说清调的是什么。**开的是同一层、走的是同一个
/// [`priority::Editor::open`]**（挂单 `Q782`），不同的只有按钮上这几个字。
pub const PRIORITY: &str = "数据源优先级…";
/// 重新探一下 ffmpeg 那颗按钮上的字。
pub const REPROBE: &str = "重新检测";
/// 「撞上外部修改」那一格的值：这一项**不给自动覆盖的开关**。
pub const EXTERNAL_EDIT: &str = "总是停下，逐份列出";

/// 主库改名改完之后，这一屏交给窗口的那一下。
///
/// 窗口标题与左栏顶上那张卡印的是开现场那一刻取的那个名字（`App::library_label`），改完名
/// 不转告一声就还是旧的，得关窗重开才换——而这一屏上刚刚才说「改完了」。
pub struct Renamed {
    /// 改成了什么。
    pub name: String,
}

/// 设置那一屏。
pub struct Screen {
    /// 工作目录：数据源、优先级表、媒体池都在它下面。
    workspace: PathBuf,
    /// 看的是哪一节。
    at: Section,
    /// 主库原名改到一半的那一份。`None` 是还没开始改（框里摆的是眼下这个名字）。
    name_draft: Option<String>,
    /// 上一次按「改名」的回话：`Ok` 是改成了，`Err` 是核心库拒的那句话。
    renamed: Option<Result<String, String>>,
    /// 改成了、还没被窗口取走的那个名字。
    handed: Option<Renamed>,
    /// 人挑好了、还没被窗口取走的那个新工作目录。
    switch_to: Option<PathBuf>,
    /// 换工作目录被拒的那句话（核心库说的）。
    refused: Option<String>,
    /// 三个数据源现在什么状况。`None` 是这一趟还没问过。
    sources: Option<Vec<SourceStatus>>,
    /// 探 ffmpeg 用哪个程序名。测试拿它走「它不在」那条路（`preview::NO_SUCH_PROGRAM`）。
    probe: String,
    /// 探出来的那一份：`Ok` 是版本，`Err` 是核心库说的那句话。`None` 是这一趟还没探过。
    ffmpeg: Option<Result<String, String>>,
    /// 人刚在「外观」那一排上挑的那一档，还没被窗口记进版式文件。
    theme_pick: Option<egui::ThemePreference>,
    /// 数据源优先级那一层弹层。**与刮削面板那一颗开的是同一种**（挂单 `Q782`）。
    priority: priority::Editor,
    /// 画时刻拿哪一刻当此刻。截图那一路钉死。
    clock: crate::clock::Clock,
}

impl Screen {
    /// 开一块，停在「常规」那一节。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            priority: priority::Editor::new(workspace.clone()),
            workspace,
            at: Section::default(),
            name_draft: None,
            renamed: None,
            handed: None,
            switch_to: None,
            refused: None,
            sources: None,
            probe: preview::FFMPEG.to_string(),
            ffmpeg: None,
            theme_pick: None,
            clock: crate::clock::Clock::default(),
        }
    }

    /// 看的是哪一节。
    #[must_use]
    pub fn section(&self) -> Section {
        self.at
    }

    /// 翻到某一节。截图那一路与 `⌘/Ctrl+,` 拿它点名。
    pub fn show_section(&mut self, at: Section) {
        self.at = at;
    }

    /// 探 ffmpeg 用哪个程序名。
    ///
    /// **截图那一路要它**：这一格照实探这台机器，而门禁那几台机器有的装了 ffmpeg、有的没装,
    /// 照实画的话同一张基线在两台机器上是两个样子。测试指 `preview::NO_SUCH_PROGRAM`，
    /// 画出来的一律是「没找到」那一档——设计稿上那一格画的也正是这一档。
    pub fn probe_with(&mut self, program: impl Into<String>) {
        self.probe = program.into();
        self.ffmpeg = None;
    }

    /// 画时刻拿哪一刻当此刻（数据源那张表的「上次更新」）。
    pub fn pin_clock(&mut self, clock: crate::clock::Clock) {
        self.clock = clock;
    }

    /// 数据源那几条这一趟问出来是什么，供测试查。
    #[must_use]
    pub fn sources(&self) -> Option<&[SourceStatus]> {
        self.sources.as_deref()
    }

    /// 上一次按「改名」的回话；`None` 是这一趟还没按过。
    #[must_use]
    pub fn renamed(&self) -> Option<Result<&str, &str>> {
        self.renamed
            .as_ref()
            .map(|said| said.as_deref().map_err(String::as_str))
    }

    /// **改名改成了、还没转告窗口的那一下**：窗口取走它去换标题与左栏那张卡上的名字。
    pub fn take_renamed(&mut self) -> Option<Renamed> {
        self.handed.take()
    }

    /// **人挑好了一个新的工作目录**：窗口取走它，整份退回开场（换工作目录等于换一整套工具状态）。
    pub fn take_workspace(&mut self) -> Option<PathBuf> {
        self.switch_to.take()
    }

    /// **人刚挑的那一档外观**：窗口取走它记进版式文件（版式那一份归窗口管，见
    /// [`crate::layout::Layout`]）。这一层不自己落盘——偏好落哪儿只有一处写。
    pub fn take_theme(&mut self) -> Option<egui::ThemePreference> {
        self.theme_pick.take()
    }

    /// 数据源优先级那一层，改得动的那一份：窗口取走它保存下来的那份表换进浏览屏
    /// （[`priority::Editor::take_saved`]，照浏览屏那一支）。
    pub fn priority_mut(&mut self) -> &mut priority::Editor {
        &mut self.priority
    }

    /// 画这一屏。
    pub fn ui(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        // 优先级那一层弹层**画在最上头**：它是一层弹层，底下这一屏这一帧不接快捷键
        // （[`crate::dialog`] 那道门）。
        self.priority.show(ui.ctx(), &facts.site.catalog);
        let tokens = Tokens::builtin();
        look::screen_body(ui, "设置屏", |ui| {
            ui.horizontal_top(|ui| {
                self.nav(ui);
                ui.add_space(tokens.space.settings_nav_divider);
                ui.separator();
                ui.add_space(tokens.space.settings_gap - ui.spacing().item_spacing.x);
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    match self.at {
                        Section::General => self.general(ui, facts),
                        Section::Workspace => self.workspace_section(ui, facts),
                        Section::Sources => self.sources_section(ui),
                        Section::Scrape => self.scrape(ui),
                        Section::Export => self.export(ui, facts),
                        Section::Tools => self.tools(ui),
                        Section::Keys => self.keys(ui),
                        Section::About => self.about(ui),
                    }
                });
            });
        });
    }

    /// 左边那一列（设计稿 `.sets nav`）：八节各一格，选中那一格铺强调浅色。
    fn nav(&mut self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let 宽 = tokens.layout.settings_nav_width;
        ui.allocate_ui_with_layout(
            egui::vec2(宽, 0.0),
            egui::Layout::top_down_justified(egui::Align::Min),
            |ui| {
                ui.spacing_mut().item_spacing.y = tokens.space.settings_nav_gap;
                ui.spacing_mut().interact_size.y = tokens.layout.settings_nav_height;
                ui.spacing_mut().button_padding.x = tokens.space.settings_nav_padding;
                for 节 in Section::ALL {
                    if ui.selectable_label(self.at == 节, 节.label()).clicked() {
                        self.at = 节;
                    }
                }
            },
        );
    }

    /// **一行**（设计稿 `.srow`）：名一列、值一列，底下一道横贯的线。
    ///
    /// 名那一列宽取令牌、**不跟着字长走**，于是值那一列每一行都从同一条线起——这正是
    /// 「同一列的元素左缘是同一条线」那一条（模块文档「版式」一节）。
    fn 一行(ui: &mut egui::Ui, 名: &str, add: impl FnOnce(&mut egui::Ui)) {
        let tokens = Tokens::builtin();
        let [行内缝, 列缝] = tokens.space.settings_row_gap;
        let 名宽 = tokens.layout.settings_row_label;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 列缝;
            ui.allocate_ui_with_layout(
                egui::vec2(名宽, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    // **这一句是这一列立得起来的全部理由**：`allocate_ui_with_layout` 交回来的是
                    // 里头那点内容占的地方，不是要的那个宽。少了它，「版本」那一行的值就比
                    // 「库文件结构」那一行往左 37 点——正是「跟着前面文字流走」那个毛病。
                    ui.set_min_width(名宽);
                    ui.add_space(tokens.space.settings_label_top);
                    ui.label(
                        font::strong(名)
                            .size(tokens.font.size_small_plus)
                            .color(ui.visuals().weak_text_color()),
                    );
                },
            );
            ui.vertical(|ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 行内缝;
                add(ui);
            });
        });
        ui.add_space(tokens.space.settings_row_bottom);
        look::divider(ui);
        ui.add_space(tokens.space.settings_body_gap);
    }

    /// 一行等宽的**路径**（设计稿 `.srow` 里那条 `.mono`）：路径占满左边，按钮贴右。
    ///
    /// **印的是工作目录那一段短写**（`~/.local/share/romcat`），不是这台机器上那条绝对路径：
    /// 截图那一路的工作目录是临时目录，照实印的话每台机器、每一趟都是另一串
    /// （`App::set_workspace_label` 的文档说的是同一件事）。
    fn 路径行(ui: &mut egui::Ui, 路径: &str, 按钮: Option<&str>) -> bool {
        let mut 按了 = false;
        let 宽 = 按钮.map_or(0.0, |字| look::small_button_width(ui, 字));
        ui.horizontal(|ui| {
            let 缝 = ui.spacing().item_spacing.x;
            let 剩下 = (ui.available_width() - 宽 - 缝).max(0.0);
            ui.allocate_ui_with_layout(
                egui::vec2(剩下, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_min_width(剩下);
                    ui.set_max_width(剩下);
                    ui.add(egui::Label::new(font::mono(路径)).truncate());
                },
            );
            if let Some(字) = 按钮 {
                按了 = look::small_buttons(ui, |ui| ui.button(字)).clicked();
            }
        });
        按了
    }
}

/// 这几节的正文。
impl Screen {
    /// **常规**：外观、主库原名、界面语言。
    fn general(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        Self::一行(ui, "外观", |ui| {
            let 眼下 = ui.ctx().options(|options| options.theme_preference);
            if let Some(挑的) = look::segmented(ui, &THEMES, 眼下) {
                ui.ctx().set_theme(挑的);
                self.theme_pick = Some(挑的);
            }
            look::help(ui, "跟随系统时，换一次系统主题，界面跟着换。");
        });
        self.rename(ui, facts);
        Self::一行(ui, "界面语言", |ui| {
            // **平常的字，不画成标签**：稿上这一格是一句话，标签那一档（`look::read_only`）
            // 是绿的、说的是「放心」——一屏的值全画成绿标签，屏上就成了一片「都挺好」。
            ui.label("简体中文");
            look::help(ui, "眼下只有简体中文。");
        });
    }

    /// **给已经建好的主库改名**（票面第 2 条验收）。
    ///
    /// 收不收、为什么不收全在核心库那一处（`Catalog::set_library_name`：空白不是名字、
    /// 同一个工作目录里重名、查重名时目录列不开），这一层只把那句话印出来。改成了之后
    /// **转告窗口一声**（[`Screen::take_renamed`]）——开场屏、报告与窗口标题上印的都是原名，
    /// 不转告就得关窗重开才换。
    fn rename(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        Self::一行(ui, "主库原名", |ui| {
            let 眼下 = facts.site.display_name();
            let 草稿 = self.name_draft.get_or_insert_with(|| 眼下.clone());
            let mut 按了 = false;
            ui.horizontal(|ui| {
                look::text_input(
                    ui,
                    Tokens::builtin().layout.settings_name_width,
                    egui::TextEdit::singleline(草稿),
                );
                按了 = look::buttons(ui, |ui| ui.button(RENAME)).clicked();
            });
            if 按了 {
                let 要改成 = self.name_draft.clone().unwrap_or_default();
                self.renamed = Some(match facts.site.catalog.set_library_name(&要改成) {
                    Ok(()) => {
                        let 新名 = facts.site.display_name();
                        self.name_draft = Some(新名.clone());
                        self.handed = Some(Renamed {
                            name: 新名.clone()
                        });
                        Ok(新名)
                    }
                    Err(why) => Err(format!("{why}")),
                });
            }
            match &self.renamed {
                Some(Ok(名字)) => {
                    look::impact(
                        ui,
                        &[("这个库现在叫「", false), (名字, true), ("」", false)],
                    );
                }
                Some(Err(why)) => {
                    ui.colored_label(ui.visuals().error_fg_color, why);
                }
                None => {}
            }
            look::note(ui, RENAME_NOTE);
            look::help(ui, "开场那一屏、窗口标题与报告上印的都是它。");
        });
    }

    /// **工作目录**：在哪、里头装着什么、怎么换，以及媒体池在哪。
    fn workspace_section(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        Self::一行(ui, "工作目录", |ui| {
            if Self::路径行(ui, facts.workspace_label, Some(CHANGE)) {
                self.pick_workspace(facts);
            }
            look::help(
                ui,
                "中立库、沉淀库、裁决记录、DAT 仓库与媒体池都在这儿，一样都不落在主库里。",
            );
            look::note(ui, WORKSPACE_NOTE);
            if let Some(why) = &self.refused {
                ui.colored_label(ui.visuals().error_fg_color, why);
            }
            look::help(
                ui,
                "换一个工作目录等于换一整套工具状态：挑好之后回到开场，在那儿挑一份库打开。",
            );
        });
        Self::一行(ui, "媒体池", |ui| {
            Self::路径行(ui, &format!("{}/media", facts.workspace_label), None);
            look::help(
                ui,
                "封面、截图与视频按内容存在这儿，同一张图不会存两份；各前端的媒体目录由它铺出来。",
            );
        });
        Self::一行(ui, "里头装着", |ui| {
            self.占用表(ui, facts);
            look::help(ui, "每一样占多少空间还算不出来，这儿报的是条数。");
        });
    }

    /// 把「主库原名」那一格里摆着的字换掉。
    ///
    /// **测试那一路要它**：屏上那个框里摆的是眼下这个名字，而不开窗那一路只发得了「往光标处
    /// 插一段字」——插出来的是「主库甲」，验不了「换成另一个名字」与「改成空白」这两条，
    /// 而后一条正是核心库那三种拒绝里的头一种。
    pub fn draft_name(&mut self, name: impl Into<String>) {
        self.name_draft = Some(name.into());
    }

    /// 人按了「更换…」：弹系统选择窗口，挑回来的那个交给 [`Self::offer_workspace`]。
    fn pick_workspace(&mut self, facts: &Facts<'_>) {
        let Some(挑的) = pick::directory("换一个工作目录", &self.workspace) else {
            return;
        };
        self.offer_workspace(挑的, facts);
    }

    /// 挑回来的那个目录**先过一遍「不许落在主库的根之内」**，过了才算数。
    ///
    /// **那道判据不在这一层**：`Roots::refuse_writing_into` 逐个根问一次
    /// `path::refuse_writing_into_library`（ADR-0004、ADR-0024），拒的那句话也是它说的。
    ///
    /// 与弹窗那一下分开，是因为**弹窗那一层测不到**（`crate::pick` 的模块文档：系统模态窗口，
    /// 门禁的界面测试不开窗）；分开之后，这道判据连同它那句话就验得了。
    pub fn offer_workspace(&mut self, 挑的: PathBuf, facts: &Facts<'_>) {
        self.refused = None;
        match romcat_core::catalog::roots::Roots::load(&facts.site.catalog) {
            Ok(roots) => match roots.refuse_writing_into(&挑的) {
                Ok(()) => self.switch_to = Some(挑的),
                Err(why) => self.refused = Some(why),
            },
            // 根读不出来（库刚被挪走、文件坏了）：**不拿一句猜测挡住人**，照旧换过去——
            // 开场那一屏本来就是空手进去的，换过去之后它自己会说那边有什么。
            Err(_) => self.switch_to = Some(挑的),
        }
    }

    /// 工作目录里那几样各有多少条（设计稿那张表的位置，换的是口径）。
    ///
    /// **稿上那一列是字节，这儿是条数**：核心库今天一个字节都算不出来（没有哪一处在把目录
    /// 走一遍求和），而条数是现成的。数字那一列**右对齐**——右对齐才比得出大小。
    fn 占用表(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        self.survey();
        let mut 几行: Vec<(String, String)> = vec![(
            "裁决记录".to_owned(),
            facts
                .verdicts
                .map_or_else(|| "—".to_owned(), |多少| format!("{} 条", thousands(多少))),
        )];
        for status in self.sources.iter().flatten() {
            几行.push((status.name.to_owned(), 条数(status)));
        }
        数字表(ui, "工作目录里头装着", &几行);
    }

    /// **数据源**：本机那几份、优先级、ScreenScraper。
    fn sources_section(&mut self, ui: &mut egui::Ui) {
        Self::一行(ui, "本机那几份", |ui| {
            self.数据源表(ui);
            look::help(
                ui,
                "还不会自动检查更新：要更新的时候，去「库」那一屏数据源那一块按「全部更新」。",
            );
        });
        Self::一行(ui, "优先级", |ui| {
            if look::small_buttons(ui, |ui| ui.button(PRIORITY)).clicked() {
                self.priority.open();
            }
            look::help(ui, "同一个字段有好几个源给了值时，显示哪一个。");
        });
        Self::一行(ui, "ScreenScraper", |ui| {
            let 给了 = romcat_core::scrape::online::Credentials::from_env().is_some();
            if 给了 {
                look::read_only(ui, "账号已给");
            } else {
                look::chip(ui, look::Tone::Caution, "账号没给");
            }
            look::help(
                ui,
                "账号眼下只认开工具之前给的那四个环境变量（SCREENSCRAPER_DEVID、\
                 SCREENSCRAPER_DEVPASSWORD、SCREENSCRAPER_SSID、SCREENSCRAPER_SSPASSWORD），\
                 屏上还存不下来，也还测不了连接。",
            );
            look::help(
                ui,
                "两条配额只有联网刮削跑着的那一趟报得出，写在刮削面板上；这儿留不住。",
            );
            // **头上那一句留空**：这一段警示全仓只有一处写（[`QUOTA`]），
            // 把它拆成「粗体一句 + 正文」就是在第二处再写一遍同一件事。
            look::warn_box(ui, "", QUOTA);
        });
    }

    /// 本机那三份数据源：条数右对齐，上次更新照钟画。
    fn 数据源表(&mut self, ui: &mut egui::Ui) {
        self.survey();
        let clock = self.clock;
        let 几行: Vec<(String, String, String)> = self
            .sources
            .iter()
            .flatten()
            .map(|status| {
                let 时刻 = status
                    .fetched_at()
                    .map_or_else(|| "——".to_owned(), |at| clock.short(at));
                (status.name.to_owned(), 条数(status), 时刻)
            })
            .collect();
        let tokens = Tokens::builtin();
        let [行缝, 列缝] = tokens.space.cell_padding;
        let 数宽 = 最宽(ui, 几行.iter().map(|(_, 数, _)| 数.as_str()));
        egui::Grid::new("设置屏数据源")
            .num_columns(3)
            .min_col_width(0.0)
            .spacing(egui::vec2(2.0 * 列缝, 行缝))
            .show(ui, |ui| {
                for (名, 数, 时刻) in &几行 {
                    ui.label(名);
                    靠右(ui, 数宽, 数);
                    ui.label(egui::RichText::new(时刻).small().weak());
                    ui.end_row();
                }
            });
    }

    /// **刮削**：那几个旋钮每趟自己点，记不住。
    fn scrape(&mut self, ui: &mut egui::Ui) {
        Self::一行(ui, "默认那一档", |ui| {
            ui.label(romcat_core::scrape::Gather::default().label());
            look::help(
                ui,
                "每次打开刮削面板都从这一档起：输入指纹没变的整条跳过，只采还没采过的那些。",
            );
        });
        Self::一行(ui, "那几个旋钮", |ui| {
            look::help(
                ui,
                "采法、采哪几个字段、要不要联网、要不要下载媒体，每次打开刮削面板都从同一档起，\
                 改了不记住。刮削面板在「浏览」那一屏的屏头上。",
            );
        });
        Self::一行(ui, "联网请求间隔", |ui| {
            ui.label(间隔());
            look::help(ui, "眼下改不了。间隔越短越快，也越容易撞上配额。");
        });
    }

    /// **导出**：导出成什么、导出到哪、铺不铺媒体、撞上外部修改怎么办。
    fn export(&mut self, ui: &mut egui::Ui, facts: &Facts<'_>) {
        let 设过 = facts.site.catalog.export_setup().ok().flatten();
        Self::一行(ui, "前端格式", |ui| {
            match &设过 {
                Some(setup) => {
                    ui.label(&setup.format);
                }
                None => {
                    look::help(ui, "还没设过。");
                }
            }
            look::help(
                ui,
                &format!(
                    "认得这几种：{}。在「库」那一屏导出设置那一块里改。",
                    romcat_core::adapter::names().join("、")
                ),
            );
        });
        Self::一行(ui, "导出目录", |ui| {
            match &设过 {
                Some(setup) => {
                    Self::路径行(ui, &romcat_core::path::display(&setup.out), None);
                }
                None => {
                    look::help(ui, "还没设过。");
                }
            }
            look::help(
                ui,
                "通常就是主库根目录。只写元数据文件，一个 ROM 都不搬（主库只读）。",
            );
        });
        Self::一行(ui, "导出时铺媒体", |ui| {
            ui.label(font::strong("默认不铺"));
            look::help(
                ui,
                "要铺的话，在「库」那一屏导出设置那一块按「导出」之前勾上——这一勾每趟自己点，\
                 不记住；关掉只写元数据，前端里就没有封面。",
            );
        });
        Self::一行(ui, "撞上外部修改", |ui| {
            // 照稿那一格是粗体的一句（`<b>总是停下，逐份列出</b>`）。
            ui.label(font::strong(EXTERNAL_EDIT));
            look::note(
                ui,
                "这一项不给「自动覆盖」的开关：覆盖丢掉的是人的一次手改。逐份看过之后，\
                 可以在当时选择照写。",
            );
        });
    }

    /// **工具**：ffmpeg 与压缩格式。
    fn tools(&mut self, ui: &mut egui::Ui) {
        Self::一行(ui, "ffmpeg", |ui| {
            if self.ffmpeg.is_none() {
                self.ffmpeg = Some(探一下(&self.probe));
            }
            ui.horizontal(|ui| {
                match &self.ffmpeg {
                    Some(Ok(版本)) => {
                        look::read_only(ui, &format!("已找到 · {版本}"));
                    }
                    Some(Err(why)) => {
                        look::chip(ui, look::Tone::Caution, "没找到");
                        ui.label(egui::RichText::new(why).small().weak());
                    }
                    None => {}
                }
                if look::small_buttons(ui, |ui| ui.button(REPROBE)).clicked() {
                    self.ffmpeg = Some(探一下(&self.probe));
                }
            });
            look::help(
                ui,
                "用来给视频抽一张预览帧。没有它也照常用：视频显示成占位图标，仍旧点得开，\
                 拿系统默认播放器放。",
            );
            look::help(
                ui,
                "眼下只在系统的程序搜索路径里找它，屏上还指不了别的路径。",
            );
        });
        Self::一行(ui, "压缩格式", |ui| {
            ui.label("zip、7z：内置");
            look::help(
                ui,
                "cue/bin 转 chd 一类镜像转换要另外的外部程序，这一版不支持。",
            );
        });
    }

    /// **快捷键**：摆的是全仓那唯一一份表（[`keys`]）。
    ///
    /// 照稿分两列（`.kgrid`）：一条里说明靠左、键帽靠右，底下一道虚线。**左右各是一列**——
    /// 同一列里那几枚键帽的右缘是同一条线（数字与键帽一类靠右对齐才比得出来），截图门钉着这一条。
    /// 靠左靠右走 [`egui::Sides`]，不自己算位置：上一轮在浏览屏上自算位置把按钮挤出过行外。
    fn keys(&mut self, ui: &mut egui::Ui) {
        let tokens = Tokens::builtin();
        let [行缝, 列缝] = tokens.space.keys_grid_gap;
        let 列宽 = ((ui.available_width() - 列缝) / 2.0).max(1.0);
        for group in keys::groups() {
            look::section(ui, group.title);
            ui.add_space(行缝);
            for 一排 in group.keys.chunks(2) {
                // **顶对齐**（`horizontal_top`，不是 `horizontal`）：`horizontal` 是
                // `Align::Center`，而这一排里每一格要的高是现长出来的——左边那一格先长完，
                // 右边那一格就被按着那个高居中，整格往下掉半格（实测 15 点），两列对不上。
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 列缝;
                    for (管什么, 键) in 一排 {
                        Self::一条快捷键(ui, 列宽, 管什么, 键);
                    }
                });
                ui.add_space(行缝);
            }
            ui.add_space(tokens.space.settings_body_gap);
        }
        look::help(ui, keys::NOTE);
    }

    /// 快捷键表上的一条：说明靠左、键帽靠右，底下一道虚线（设计稿 `.kgrid div`）。
    fn 一条快捷键(ui: &mut egui::Ui, 宽: f32, 管什么: &str, 键: &str) {
        let tokens = Tokens::builtin();
        ui.allocate_ui_with_layout(
            egui::vec2(宽, 0.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(宽);
                ui.add_space(tokens.space.keys_row_padding);
                egui::Sides::new().spacing(tokens.space.keys_row_gap).show(
                    ui,
                    |ui| {
                        ui.label(egui::RichText::new(管什么).size(tokens.font.size_small_plus));
                    },
                    |ui| {
                        look::kbd(ui, 键);
                    },
                );
                ui.add_space(tokens.space.keys_row_padding);
                let (线框, _) = ui.allocate_exact_size(egui::vec2(宽, 1.0), egui::Sense::hover());
                look::dashed_hline(
                    ui.painter(),
                    线框.x_range(),
                    线框.center().y,
                    ui.visuals().widgets.noninteractive.bg_stroke,
                );
            },
        );
    }

    /// **关于**：版本、库文件结构版本、数据源与许可。
    fn about(&mut self, ui: &mut egui::Ui) {
        Self::一行(ui, "版本", |ui| {
            ui.label(font::mono(env!("CARGO_PKG_VERSION")));
        });
        Self::一行(ui, "库文件结构", |ui| {
            ui.label(font::mono(format!(
                "版本 {}",
                romcat_core::catalog::SCHEMA_VERSION
            )));
        });
        Self::一行(ui, "数据源", |ui| {
            let registry = romcat_core::dat::registry::Registry::builtin();
            let 几家: Vec<&str> = registry
                .sources()
                .iter()
                .map(|source| source.name.as_str())
                .collect();
            ui.label(几家.join("、"));
            look::help(ui, "外加本机的中文离线源，以及可选的 ScreenScraper。");
        });
        Self::一行(ui, "许可", |ui| {
            look::help(
                ui,
                "各家数据源的许可条款还没收进工具里，以它们自己的说明为准。",
            );
        });
    }

    /// 三个数据源这一趟还没问过就问一遍。**不每帧问**：它要开三份本机库各数一次。
    fn survey(&mut self) {
        if self.sources.is_none() {
            self.sources = Some(sources::survey(&self.workspace));
        }
    }
}

/// 配额那一段警示的原话：**与刮削面板上那一段是同一句**（`crate::scrape::QUOTA_WARNING`）。
/// 两处各写一份的话，ADR-0007 那条命脉就会在屏上有两个说法。
const QUOTA: &str = crate::scrape::QUOTA_WARNING;

/// 一个数据源那一格写什么：取回了写条数，没取回或者读不动写那个词。
fn 条数(status: &SourceStatus) -> String {
    match &status.state {
        SourceState::Ready { records, .. } => format!("{} 条", thousands(*records)),
        SourceState::Missing => "未下载".to_owned(),
        SourceState::Broken { .. } => "读不动".to_owned(),
    }
}

/// 联网请求间隔那一格写什么：核心库默认的那一档。
fn 间隔() -> String {
    let 秒 = romcat_core::scrape::online::DEFAULT_INTERVAL.as_secs_f32();
    format!("{秒:.0} 秒")
}

/// 探一下 ffmpeg：核心库那一处判据（`preview::probe`），这一层只把话转出来。
fn 探一下(program: &str) -> Result<String, String> {
    preview::probe(program).map_err(|why| why.render())
}

/// 数字那几列用的**等宽字**：与 `font::mono` 画出来的是同一档。
///
/// 量宽与画字必须用同一个字：拿正文那一档去量、拿等宽那一档去画，量出来的列宽就不够，
/// 右对齐当场塌掉（长的那几段溢出去，右缘各是各的）。
fn 等宽字(ui: &egui::Ui) -> egui::FontId {
    let 正文 = egui::TextStyle::Body.resolve(ui.style());
    egui::FontId::new(正文.size, egui::FontFamily::Monospace)
}

/// 这几段字里最宽那一段有多宽（按[等宽字](等宽字)量）。定死一列的宽度用它。
fn 最宽<'a>(ui: &egui::Ui, 几段: impl Iterator<Item = &'a str>) -> f32 {
    let font = 等宽字(ui);
    几段.fold(0.0_f32, |最宽, 一段| {
        let galley =
            ui.painter()
                .layout_no_wrap(一段.to_owned(), font.clone(), ui.visuals().text_color());
        最宽.max(galley.size().x)
    })
}

/// 在宽 `宽` 的一格里**靠右**摆一段字：数字那几列右对齐才比得出大小。
///
/// **自己排一遍字再画**，不靠 `Layout::right_to_left` 摆：那条路摆出来的位置取决于这一格
/// 这一帧分到多宽，同一列里长短不一的几行会落在两条线上（实测差 10 点）。这儿把右缘直接
/// 钉在这一格的右沿上——截图门那条「数字那一列右对齐」量的就是它。
fn 靠右(ui: &mut egui::Ui, 宽: f32, 字: &str) {
    let galley = ui
        .painter()
        .layout_no_wrap(字.to_owned(), 等宽字(ui), ui.visuals().text_color());
    let 要的 = egui::vec2(宽.max(galley.size().x), galley.size().y);
    let (格, _) = ui.allocate_exact_size(要的, egui::Sense::hover());
    let 落点 = egui::pos2(格.max.x - galley.size().x, 格.min.y);
    ui.painter().galley(落点, galley, ui.visuals().text_color());
}

/// 一张「名 → 数」两列表：名靠左，**数靠右**。
fn 数字表(ui: &mut egui::Ui, id: &str, 几行: &[(String, String)]) {
    let tokens = Tokens::builtin();
    let [行缝, 列缝] = tokens.space.cell_padding;
    let 数宽 = 最宽(ui, 几行.iter().map(|(_, 数)| 数.as_str()));
    egui::Grid::new(id)
        .num_columns(2)
        .min_col_width(0.0)
        .spacing(egui::vec2(2.0 * 列缝, 行缝))
        .show(ui, |ui| {
            for (名, 数) in 几行 {
                ui.label(名);
                靠右(ui, 数宽, 数);
                ui.end_row();
            }
        });
}

/// 画这一屏要知道的几样。
pub struct Facts<'a> {
    /// 这份**现场**：主库原名、一组根、导出设过什么，都问它。
    pub site: &'a Site,
    /// **工作目录**那一段短写（`HOME` 缩成 `~`）：与底部状态栏右边印的是同一句。
    pub workspace_label: &'a str,
    /// 沉淀库里已保存多少条裁决；读不出来是 `None`。左栏底下那句用的是同一个数。
    pub verdicts: Option<u64>,
}
