//! **刮削面板**：四个旋钮，外加按下去之前那本账。
//!
//! 它挂在[浏览屏](crate::library)上：筛出一批，按「刮削选中…」，面板摊开。
//!
//! ## 四个旋钮，两条轴分得清清楚楚
//!
//! | 旋钮 | 答的是 |
//! |---|---|
//! | **范围** | 对谁——筛出来的这批，**不可就地编辑**（要改回筛选器改） |
//! | **字段** | **我要什么** |
//! | **源** | **花多少代价**——默认只勾本地源；勾联网源先弹配额提醒 |
//! | **采法** | 跑多久——[补缺 / 重采](Gather) |
//!
//! 「我要什么」与「花多少代价」是两条轴：字段选窄了省的是库里多几行少几行，
//! 源与采法选宽了花的是**配额**——而在线那份配额同时按账号与 IP 计，撞穿了是永久封禁
//! （ADR-0007）。两条轴混成一个「质量」滑块，人就没法在「我要简介」与「我不想赌账号」
//! 之间分别表态。
//!
//! ## 底下那本账
//!
//! 请求数与耗时由[核心库](romcat_core::scrape::estimate)算，这一层只把它画出来
//! （ADR-0005）。**这块面板存在的全部理由就是那个数**：一个「预计 0 个请求」而按下去
//! 发了九千个的界面，比不给估算更坏。
//!
//! 还有一句常驻的话：**裁决与你手工维护的元数据不会被动。** 它不只是一句安慰——
//! 刮削结果按「锚点 × 字段 × 源」三元组**并存**，没有覆盖这回事，而**裁决**排在每条链
//! 的第一位（`priorities.toml` 的第一条规则、ADR-0001）；重采清采集记录时也**一条裁决
//! 都不删**（`Catalog::clear_scraped`）。
//!
//! ## 「有值了但我想换一个」不该按这个按钮
//!
//! 那是**优先级**的事：改一次排序、零成本、不重跑。面板上把这句话写出来，是因为
//! 不写的话，人唯一想得到的办法就是重采——那要花掉一整天的配额去换一件排序就能做到的事。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use romcat_core::catalog::{Catalog, CatalogError, Roots};
use romcat_core::dat::HttpFetcher;
use romcat_core::fs::RealFs;
use romcat_core::identify::fuzzy;
use romcat_core::report::{human_duration, thousands};
use romcat_core::scrape::estimate::{self, Estimate};
use romcat_core::scrape::online::{self, Credentials, Limits, Net};
use romcat_core::scrape::{self, Field, Gather, Options, Profile};
use romcat_core::site::Site;
use romcat_core::task::{Done, Finished, Handle};
use romcat_core::{verdict, workspace, zh};

use crate::task::{Product, Tasks};

/// 面板上那句常驻的话。**行为上也成立**，不只是写着好看：
/// 裁决排在每条优先级链的第一位，重采清采集记录时一条裁决都不删。
pub const UNTOUCHED: &str = "裁决与你手工维护的元数据不会被动。";

/// 「有值了但我想换一个」该怎么办。
pub const NOT_BY_RESCRAPE: &str =
    "「有值了但我想换一个」不该靠重采——那是**优先级**的事：改一次排序，零成本、不重跑。";

/// 勾联网源时弹的那句提醒。**照 ADR-0007 的口径说**。
pub const QUOTA_WARNING: &str = "\
    联网源赌的是你的**账号与 IP**：ScreenScraper 的配额同时按账号与 IP 计，\
    撞穿了是永久封禁，而汉化版在它眼里正是「未识别 ROM」。\
    这一档默认限流、只对**已确认**的条目发请求、配额超限当场停下——不重试、不换账号。";

/// **字段那个旋钮上摆得出来的那几个。**
///
/// 标题与汉化组**不在这张单子上，因此永远采**：中立库里标题永远是集合
/// （`CONTEXT.md` 的**标题集合**词条），而这个库最要紧的产出——中文名与别名——就落在
/// 那里。把它做成一个可关的开关，等于在界面上摆一个「把中文名关掉」的按钮，
/// 而它一分钱代价都省不下来（那几个源本来就是一次撞完，多带一栏不多花任何东西）。
pub const KNOBS: [Field; 5] = [
    Field::Description,
    Field::Genre,
    Field::Developer,
    Field::Publisher,
    Field::Year,
];

/// 算这本账时的**限流参数**。与命令行的默认那一份是同一个来处
/// （`online::DEFAULT_INTERVAL` / `DEFAULT_BUDGET`），于是屏上估的耗时就是真跑的节奏。
fn limits() -> Limits {
    Limits::default()
}

/// 一组旋钮的位置。**账是照哪一组算的，记下来**——不然每帧都要重算一遍，
/// 而算一遍要问好几次中立库。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Knobs {
    fields: BTreeSet<Field>,
    media: bool,
    online: bool,
    sweep: Gather,
    scope: usize,
}

/// 刮削面板。
pub struct Panel {
    /// 工作目录：媒体池、优先级表、中文离线索引、沉淀库都在里头。
    workspace: PathBuf,
    /// 面板摊开着没有。
    open: bool,
    /// **范围**：这一批变体的键。面板一摊开就定死——旋钮改的是「要什么」与
    /// 「花多少代价」，改范围要回筛选器改。
    scope: Vec<String>,
    /// 摊开这块面板的时候，屏上写着的是多少个变体。
    ///
    /// 与 `scope.len()` 是同一个数，留一格是为了**说得出它是从哪儿来的**：
    /// 屏上写几个、面板列几个、按下去动几个，三处同一个数（票 `gui-redesign/03`）。
    scope_shown: u64,
    /// **字段**旋钮。
    fields: BTreeSet<Field>,
    /// 收不收**媒体**。默认不收：真库那块盘 10 TB，收媒体要回盘把图读一遍。
    media: bool,
    /// 勾了**联网源**没有。**默认不勾**（ADR-0007）。
    online: bool,
    /// 配额提醒正摆着。勾联网源那一下先把它摆出来，人点过「我知道」才算真勾上。
    quota: bool,
    /// **采法**。
    sweep: Gather,
    /// 上一次算出来的账。`None` 是还没算，或者算不出来（错在 [`Self::error`] 里）。
    estimate: Option<Estimate>,
    /// 那本账是照哪一组旋钮算的。
    counted: Option<Knobs>,
    /// 正在跑的那一趟的任务号。
    running: Option<u64>,
    /// 上一次动作的回执。
    notice: Option<String>,
    /// 上一次出的错。
    error: Option<String>,
}

impl Panel {
    /// 开一块。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            open: false,
            scope: Vec::new(),
            scope_shown: 0,
            // **默认全勾**：字段那一栏答的是「我要什么」，而这几样本来就是一次撞完
            // 一起带回来的，少勾一个省不下任何代价。
            fields: KNOBS.into_iter().collect(),
            media: false,
            online: false,
            quota: false,
            sweep: Gather::default(),
            estimate: None,
            counted: None,
            running: None,
            notice: None,
            error: None,
        }
    }

    /// 面板摊开着没有。
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// **摊开面板**，范围就是交进来的这一批变体的键。
    ///
    /// `shown` 是屏上那句「作用于多少个变体」写着的数——两者对不上就是这一层出了问题，
    /// 而那正是「按下去动的比屏上写的多」那种事故。界面上按「刮削选中…」走的就是它。
    pub fn open(&mut self, keys: Vec<String>, shown: u64) {
        self.scope_shown = shown;
        self.scope = keys;
        self.open = true;
        self.counted = None;
        self.error = None;
        self.notice = None;
    }

    /// 收起面板。旋钮留在原位——人多半是回去改筛选，改完还想按同一套设置。
    pub fn close(&mut self) {
        self.open = false;
        self.quota = false;
    }

    /// 这一批有多少个变体。
    #[must_use]
    pub fn scope_total(&self) -> u64 {
        u64::try_from(self.scope.len()).unwrap_or(u64::MAX)
    }

    /// 摊开面板时屏上写着的那个数。**与 [`Self::scope_total`] 必须一样。**
    #[must_use]
    pub fn scope_shown(&self) -> u64 {
        self.scope_shown
    }

    /// 这一趟要哪几个字段。
    #[must_use]
    pub fn fields(&self) -> &BTreeSet<Field> {
        &self.fields
    }

    /// 拨一下某个字段。测试与实测拿它当界面上那一下。
    pub fn toggle_field(&mut self, field: Field) {
        if !self.fields.remove(&field) {
            self.fields.insert(field);
        }
    }

    /// 收不收媒体。
    #[must_use]
    pub fn media(&self) -> bool {
        self.media
    }

    /// 拨一下媒体那个勾。
    pub fn set_media(&mut self, on: bool) {
        self.media = on;
    }

    /// 勾上联网源了没有。**默认没有。**
    #[must_use]
    pub fn online(&self) -> bool {
        self.online
    }

    /// 配额提醒正摆着没有；摆着的时候联网源**还没**真的勾上。
    #[must_use]
    pub fn quota_prompt(&self) -> bool {
        self.quota
    }

    /// 拨一下联网源那个勾。
    ///
    /// **勾上不是一下就成的**：先把配额提醒摆出来，人点过「我知道」
    /// （[`Self::confirm_online`]）才算真勾上。取消勾选倒是一下就成——
    /// 少花配额这件事不必先请示。
    pub fn toggle_online(&mut self) {
        if self.online {
            self.online = false;
            self.quota = false;
        } else {
            self.quota = true;
        }
    }

    /// 看过配额提醒，确实要勾。
    pub fn confirm_online(&mut self) {
        self.quota = false;
        self.online = true;
    }

    /// 看过配额提醒，算了。
    pub fn decline_online(&mut self) {
        self.quota = false;
        self.online = false;
    }

    /// 采法。
    #[must_use]
    pub fn sweep(&self) -> Gather {
        self.sweep
    }

    /// 换一种采法。
    pub fn set_sweep(&mut self, sweep: Gather) {
        self.sweep = sweep;
    }

    /// 上一次算出来的那本账。
    #[must_use]
    pub fn estimate(&self) -> Option<&Estimate> {
        self.estimate.as_ref()
    }

    /// 上一次出的错。
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// 上一次动作的回执。
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// 正在跑的那一趟的任务号。
    #[must_use]
    pub fn running(&self) -> Option<u64> {
        self.running
    }

    /// 这一趟的选项。**估算与真跑收的是同一份**——两边各摆一份的话，
    /// 「屏上说 0 个请求」与「按下去发了几个」就没有共同的前提了。
    ///
    /// # Errors
    /// 中立库读不出那一组根时返回错误。
    pub fn options(&self, catalog: &Catalog) -> Result<Options, CatalogError> {
        let mut options = Options::new(
            Roots::load(catalog)?,
            workspace::media_pool_dir(&self.workspace),
        );
        options.profile = if self.online {
            Profile::Online
        } else {
            Profile::Offline
        };
        options.media = self.media;
        // **标题与汉化组永远采**（见 [`KNOBS`]）：它们不在旋钮上，也就没有关掉的办法。
        options.fields = self
            .fields
            .iter()
            .copied()
            .chain([Field::Title, Field::TranslationGroup])
            .collect();
        options.only = Some(estimate::only(self.scope.iter().cloned()));
        self.sweep.apply(&mut options);
        Ok(options)
    }

    /// 旋钮动过就重算一遍那本账。**每帧调一次，没动过是空操作。**
    ///
    /// 不每帧无脑重算，是因为算一遍要问好几次中立库——按作品取判据那一步在真库上是
    /// 九千次查询。放在画帧那条线程上每帧跑一遍就是挂账 D156 那件事的翻版。
    pub fn recount(&mut self, catalog: &Catalog) {
        let knobs = Knobs {
            fields: self.fields.clone(),
            media: self.media,
            online: self.online,
            sweep: self.sweep,
            scope: self.scope.len(),
        };
        if self.counted.as_ref() == Some(&knobs) {
            return;
        }
        self.counted = Some(knobs);
        match self
            .options(catalog)
            .map_err(|error| format!("中立库读不动：{error}"))
            .and_then(|options| {
                estimate::estimate(catalog, &options, limits())
                    .map_err(|error| format!("中立库读不动：{error}"))
            }) {
            Ok(estimate) => {
                self.estimate = Some(estimate);
                self.error = None;
            }
            Err(why) => {
                // **算不出来就说算不出来**，不摆一个 0 出去：屏上写着「0 个网络请求」
                // 而按下去发了几千个，那正是这块面板要防的事。
                self.estimate = None;
                self.error = Some(why);
            }
        }
    }

    /// **把这一趟排上任务台。**
    ///
    /// 后台那条线程按文件路径自己开一份现场（`rusqlite::Connection` 不是 `Sync`，
    /// 界面这条线程手里那一份交不过去），与扫描那一趟同一个套路。只活在内存里的库
    /// 分不出第二份，那时**就地跑完**——合成数据上这是几毫秒的事，与子库屏排差量预览
    /// 走的是同一条退路。
    ///
    /// 界面上按「加入任务队列」走的就是它，测试与实测拿它当那一下。
    pub fn start(&mut self, site: &mut Site, tasks: &mut Tasks) {
        if self.running.is_some() {
            return;
        }
        if self.scope.is_empty() {
            self.error =
                Some("这一批一个变体都没有。回筛选器筛一批，或者在列表里勾几行。".to_string());
            return;
        }
        let options = match self.options(&site.catalog) {
            Ok(options) => options,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                return;
            }
        };
        // **在线档要一套凭据，拿不到就别启动。** 悄悄退回离线跑完，只会让人对着一份
        // 缺封面的报告以为「在线源也没有」（`ScrapeError::NoNetwork` 的道理）。
        let credentials = match (self.online, Credentials::from_env()) {
            (false, _) => None,
            (true, Some(credentials)) => Some(credentials),
            (true, None) => {
                self.error = Some(format!(
                    "联网源要一套 ScreenScraper 凭据，从环境变量读：{}。\n\
                     devid 要在它的论坛人工申请（无 devid 直接 403），\
                     **不要拿别人的 devid 用**——那会连累对方被拉黑。",
                    online::ENV_KEYS.join(" / "),
                ));
                return;
            }
        };
        let workspace = self.workspace.clone();
        let title = format!(
            "刮削 · {} 个变体（{} · {}）",
            thousands(self.scope_total()),
            self.sweep.label(),
            if self.online { "含联网源" } else { "仅本地源" },
        );
        self.error = None;
        self.notice = None;
        self.running = Some(match site.catalog.file().map(Path::to_path_buf) {
            Some(file) => tasks.queue(title, move |task| {
                let mut site = Site::open_file(&workspace, &file, None)
                    .map_err(|error| format!("这份库在后台开不出来：{error}"))?;
                run(&mut site, &workspace, &options, credentials, task)
            }),
            // 只活在内存里的库（合成数据走这条）分不出第二份连接：**就地跑完**。
            // 那时窗口确实会僵一下，但那份库小到几毫秒就走完——真库一律走上面那条。
            None => tasks.run_here(title, |task| {
                run(site, &workspace, &options, credentials, task)
            }),
        });
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去**，返回「认领了没有」。
    pub fn settle(&mut self, done: &Finished<Product>) -> bool {
        if self.running != Some(done.id) {
            return false;
        }
        self.running = None;
        // 跑完一趟，采集记录变了，那本账跟着作废——重算一遍，屏上那个数才对得上。
        self.counted = None;
        match &done.ended {
            Done::Product(Product::Scraped(outcome)) => {
                self.notice = Some(finished(outcome));
                self.error = None;
            }
            // 别的屏排上去的活轮不到这儿——`running` 那道判断已经挡掉了。
            Done::Product(_) => {}
            // **停下来的地方是干净的，就得这么说。** 采完的那部分留在中立库里，
            // 再排一次从那儿接着采。
            Done::Stopped => {
                self.notice = Some(
                    "刮削按停了。已经采到的那些留在中立库里，再排一次接着采——\
                     不重做已经采完的部分。"
                        .to_string(),
                );
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Done::Failed { step, why } => {
                self.error = Some(if step.is_empty() {
                    why.clone()
                } else {
                    format!("刮削在「{step}」这一步停下了：{why}")
                });
            }
        }
        true
    }

    /// 画一帧。摊开着才画。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        if !self.open {
            return;
        }
        self.recount(&site.catalog);
        ui.horizontal(|ui| {
            ui.strong(format!(
                "刮削 · 作用于筛出来的 {} 个变体",
                thousands(self.scope_total()),
            ))
            .on_hover_text(
                "**范围就是筛出来的那一批**，这儿改不了——要改回左边的筛选器改。\
                 屏上写几个、这儿列几个、按下去动几个，三处同一个数。",
            );
            if ui.button("收起").clicked() {
                self.close();
            }
        });
        if self.quota {
            self.quota_ui(ui);
            return;
        }
        ui.separator();
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| self.fields_ui(ui));
            ui.separator();
            ui.vertical(|ui| self.sources_ui(ui));
            ui.separator();
            ui.vertical(|ui| self.sweep_ui(ui));
        });
        ui.separator();
        self.account_ui(ui);
        ui.separator();
        self.go_ui(ui, site, tasks);
    }

    /// **字段 · 我要什么。**
    fn fields_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("字段 · 我要什么");
        for field in KNOBS {
            let mut on = self.fields.contains(&field);
            if ui.checkbox(&mut on, field.label()).changed() {
                self.toggle_field(field);
            }
        }
        let mut media = self.media;
        if ui
            .checkbox(&mut media, "媒体（封面、截图、视频）")
            .on_hover_text(
                "本地那一半要回主库把图读一遍（真库上 539 份、71 秒，第二趟哈希从中立库\
                 取回）；**联网那一半每份图各花一个请求**。",
            )
            .changed()
        {
            self.media = media;
        }
        ui.weak("标题与汉化组不在这张单子上——它们**永远采**，而且一分代价都不多花。")
            .on_hover_text(
                "中立库里标题永远是集合（**标题集合**），中文名与别名就落在那里。\
                 那几个源本来就是一次撞完一起带回来的。",
            );
    }

    /// **源 · 花多少代价。**
    fn sources_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("源 · 花多少代价");
        let mut local = true;
        ui.add_enabled(false, egui::Checkbox::new(&mut local, "本地源"))
            .on_hover_text(
                "DAT、文件名、中文离线源、本地媒体。**免费**——一个网络请求都不发，\
                 也关不掉：它们不花任何配额，关掉只会让联网那一侧多背几个字段。",
            );
        let mut online = self.online;
        if ui
            .checkbox(&mut online, "ScreenScraper（联网 · 扣配额）")
            .on_hover_text(QUOTA_WARNING)
            .changed()
        {
            self.toggle_online();
        }
        if !self.online {
            ui.weak("默认只勾本地源。");
        }
    }

    /// **采法 · 跑多久。**
    fn sweep_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong("采法 · 跑多久");
        for sweep in Gather::all() {
            if ui
                .radio(self.sweep == sweep, format!("{}（{}）", sweep.label(), sweep.why()))
                .clicked()
            {
                self.sweep = sweep;
            }
        }
        ui.weak(NOT_BY_RESCRAPE).on_hover_text(
            "刮削结果按「锚点 × 字段 × 源」三元组**并存**，没有覆盖这回事。\
             真正需要重采的只有两种：数据源更新了，或者解析逻辑改了。",
        );
    }

    /// **底下那本账。**
    fn account_ui(&mut self, ui: &mut egui::Ui) {
        match &self.estimate {
            None => {
                ui.colored_label(
                    ui.visuals().error_fg_color,
                    self.error.as_deref().unwrap_or("这本账算不出来。"),
                );
            }
            Some(account) => {
                ui.label(summary(account));
                if let Some(wanted) = account.over_budget {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "这一批想问 {} 条，而这一趟自设的上限是 {} 个请求——\
                             撞上就当场停下，剩下的下一趟再来。",
                            thousands(wanted),
                            thousands(account.requests),
                        ),
                    );
                }
                if account.media_downloads {
                    ui.weak(format!(
                        "上面那个数是**查询**那一半：查回来之后，每采到一份图还要各下一次，\
                         而图有几份要查过才知道。**图与查询共用这一趟自设的 {} 个请求的上限**\
                         ——撞上就当场停下，剩下的下一趟再来。",
                        thousands(account.budget),
                    ));
                }
                if account.media_disk_read {
                    ui.weak(
                        "收媒体那一段没算进耗时里：首趟要把图从主库读一遍，第二趟哈希从\
                         中立库取回、几乎不花时间。",
                    );
                }
            }
        }
        ui.strong(UNTOUCHED).on_hover_text(
            "**裁决**排在每条优先级链的第一位（ADR-0001），刮削产出的值再多也排在它后面；\
             重采清采集记录时也一条裁决都不删。",
        );
    }

    /// **配额提醒。** 勾联网源那一下先摆它。
    fn quota_ui(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        ui.colored_label(ui.visuals().warn_fg_color, "勾上联网源之前，先看清这一条：");
        ui.label(QUOTA_WARNING);
        ui.horizontal(|ui| {
            if ui.button("我知道，勾上").clicked() {
                self.confirm_online();
            }
            if ui.button("算了").clicked() {
                self.decline_online();
            }
        });
    }

    /// **按下去那一行。**
    fn go_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        let mut go = false;
        ui.horizontal(|ui| {
            // **账算不出来就不许按。** 屏上写着「这本账算不出来」而按钮照旧按得下去的话，
            // 勾了联网源就是在零估算下拿账号与 IP 发几千个请求——那正是这块面板要防的
            // 那一件事（ADR-0007）。
            let ready =
                self.running.is_none() && !self.scope.is_empty() && self.estimate.is_some();
            go = ui
                .add_enabled(ready, egui::Button::new("加入任务队列"))
                .on_hover_text(
                    "跑在画帧那条线程之外：期间浏览、筛选、看详情照常。\
                     **底下那本账算不出来时按不动**——不知道要发多少请求就不该发。",
                )
                .clicked();
            if ui.button("取消").clicked() {
                self.close();
            }
            if self.running.is_some() {
                ui.weak("这一趟在任务屏里跑着。");
            }
        });
        if go {
            self.start(site, tasks);
        }
        if let Some(notice) = &self.notice {
            ui.weak(notice);
        }
        if let Some(error) = &self.error
            && self.estimate.is_some()
        {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }
}

/// 那本账排成给人看的一句话。
#[must_use]
pub fn summary(account: &Estimate) -> String {
    format!(
        "预计 {} 个网络请求、约 {}（{} 个变体锚点、{} 个作品锚点）。",
        thousands(account.requests),
        human_duration(u64::try_from(account.elapsed.as_millis()).unwrap_or(u64::MAX)),
        thousands(account.variants),
        thousands(account.works),
    )
}

/// 跑完那一趟排成一句回执。
fn finished(outcome: &scrape::Outcome) -> String {
    let mut out = format!(
        "刮削跑完了：{} 个「锚点 × 源」因为输入没变整条跳过，收进媒体 {} 份。",
        thousands(outcome.reused_probes),
        thousands(outcome.new_blobs + outcome.deduped),
    );
    if let Some(usage) = outcome.online {
        out.push_str(&format!(
            " 联网发了 {} 个请求，其中 {} 个是「查过、没有」。",
            thousands(usage.requests),
            thousands(usage.not_found),
        ));
    }
    if let Some(halt) = &outcome.halted {
        out.push_str(&format!(" 这一趟停在半路：{}", halt.describe()));
    }
    out
}

/// 后台那条线程真跑的那一趟。**装配全在这儿**：中文离线源、匹配裁决、优先级表、
/// 网络句柄。领域判断一条都不在这一层——它只是把核心库要的原料摆齐
/// （与命令行 `romcat scrape` 摆的是同一副）。
fn run(
    site: &mut Site,
    workspace: &Path,
    options: &Options,
    credentials: Option<Credentials>,
    task: &Handle,
) -> Result<Product, String> {
    task.steps(4);
    task.step("读优先级表").map_err(|_| "按停了".to_string())?;
    let priorities = romcat_core::sync::prepare::priorities(None, workspace)?;

    task.step("开中文离线源").map_err(|_| "按停了".to_string())?;
    // **剥离规则读工作目录里那份**（`sources::rules`，与命令行同一条查法）：正题正是
    // 拿去撞中文离线源的那一串字，两条路各用一份规则的话，同一个变体在命令行与界面上
    // 会撞到不同的条目——而那是写进库里的结论，不是显示上的差别。
    let rules = romcat_core::sources::rules(workspace)?;
    let store = open_zh(workspace, &rules)?;
    let index = match store.as_ref().map(zh::store::Store::load).transpose() {
        Ok(index) => index.filter(|index: &zh::Index| !index.is_empty()),
        Err(error) => return Err(format!("中文索引读不出来：{error}")),
    };
    let naming = fuzzy::Naming {
        rules: &rules,
        index: index.as_ref(),
        tuning: zh::Tuning::default(),
    };
    let summaries: Option<&dyn scrape::zh::Summaries> = index
        .as_ref()
        .and(store.as_ref())
        .map(|store| store as &dyn scrape::zh::Summaries);

    // **匹配裁决读不到就停下，不降级成「没人裁过」。** 当成没裁过跑下去，会把人否定掉
    // 的中文名整片撞回来——那正是沉淀库那条「宁可如实拒绝、绝不将就」要拦的事。
    task.step("摊平匹配裁决").map_err(|_| "按停了".to_string())?;
    let rulings = verdict::MatchIndex::load(&site.store, &site.library)
        .map_err(|error| format!("沉淀库读不动：{error}"))
        .and_then(|index| {
            scrape::zh::Rulings::resolve(&site.catalog, &index, fuzzy::SOURCE)
                .map_err(|error| format!("匹配裁决摊不平：{error}"))
        })?;

    task.step("采集").map_err(|_| "按停了".to_string())?;
    let fetcher = credentials
        .as_ref()
        .map(|_| HttpFetcher::with_throttle(limits().interval));
    let net = match (fetcher.as_ref(), credentials) {
        (Some(fetcher), Some(credentials)) => Some(Net::new(
            fetcher,
            limits(),
            credentials,
            task.cancel(),
        )),
        _ => None,
    };
    let mut progress = |done: scrape::Progress| task.tick(done.done, done.total);
    scrape::run(
        &RealFs::new(),
        &mut site.catalog,
        &priorities,
        options,
        net.as_ref(),
        &mut scrape::RunContext {
            cancel: task.cancel(),
            progress: &mut progress,
            naming: &naming,
            summaries,
            rulings: &rulings,
        },
    )
    .map(|outcome| Product::Scraped(Box::new(outcome)))
    .map_err(|error| format!("刮削失败：{error}"))
}

/// 本机那份中文离线索引。**没取过数不是错误**——少一层而已，报告会说清楚。
///
/// 结构版本对不上时从本机那份原件就地重建（`zh::sync::rebuild`），一个网络请求都不发；
/// **重建不成也只是少一层**，与命令行同一条口径（那一侧的 `heal_zh_store`）。
/// 打不开那份库才是错——那说明它在，只是坏了或者比程序新，静悄悄当成「没取过数」跑下去，
/// 用户会对着一份缺了简介的报告以为数据源就是这么浅。
fn open_zh(
    workspace: &Path,
    rules: &romcat_core::filename::Rules,
) -> Result<Option<zh::store::Store>, String> {
    let path = workspace::zh_store_path(workspace);
    if !path.exists() {
        return Ok(None);
    }
    let mut store = zh::store::Store::open(&path)
        .map_err(|error| format!("中文索引打不开：{error}"))?;
    if store.rebuilding().is_some() {
        // 平台清单同样读工作目录里那份：重建要把数据源写的平台名折成本工具的平台名。
        let manifest = romcat_core::sources::manifest(workspace)?;
        let _ = zh::sync::rebuild(
            &RealFs,
            &mut store,
            &manifest,
            rules,
            &workspace::zh_cache_dir(workspace),
        );
    }
    Ok(Some(store))
}
