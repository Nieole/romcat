//! **子库屏**：管住这几台设备。**不选内容**——改选择跳回浏览屏。
//!
//! ## 一屏三件事，不是八件
//!
//! 用户对上一版的原话是「子库页面的操作逻辑看不懂」。根因不在控件长得好不好看：
//! 那一屏**把「选什么」和「送到哪」混在了一起**——子库列表、表单、规则增删、例外记撤、
//! 差量步骤、容量账、同步、删除确认，八样东西挤在一处，而且没有先后顺序。
//!
//! 拆开之后，**选什么归浏览屏，送到哪归这一屏**，于是这一屏只剩三件事：
//!
//! 1. **选哪台**——中间那一列卡片，一台设备一张。
//! 2. **配目标**——底下那块面板：名字、目标路径、前端格式、容量上限、能力档案。
//! 3. **排差量后同步**——卡片下半截。
//!
//! **选择集在这一屏上只读。** 规则怎么改、例外记哪几条，全在浏览屏上做：卡上点
//! 「改选择」跳过去、这个子库的规则预填进筛选器，调完按「更新到子库」原样带回来。
//! 这不是为了少写几个控件——**在浏览屏上改规则，人看得见它真的筛出了什么**；
//! 在这一屏上改，改完只看得见一行字。
//!
//! ## 顺序是硬要求，不是排版
//!
//! ADR-0016：**同步前必须预览差量，且这是硬要求不是优化项**——「永远不能点了同步就开始
//! 传」。ADR-0015 再加一条：**删除前必须干跑预览**。这一屏把两句话变成一个构造上的事实：
//! [`Screen::sync`] 只认 [`Screen::prepared`] 里那份计划，而那份计划是
//! [`romcat_core::sync::prepare`](fn@romcat_core::sync::prepare) 排出来的、界面上正摆着的同一个值。**没预览就没有可传的
//! 东西**；改过选择、改过目标之后那份预览当场作废（[`Screen::invalidate`]），
//! 「同步」按钮跟着灰掉。
//!
//! ## 容量条三段：选中的、清单之外的、上限
//!
//! 三个数各有各的出处，而且**出处不同这件事要说得出口**（[`Gauge`]）：**选中**只问
//! 中立库，卡不在手边也算得出来；**清单之外**要目标设备在位，没排过差量预览时它是
//! 「还不知道」而不是零。画成同一个零的话，人会以为卡上是空的。
//!
//! 超没超由核心一处算（`sublibrary::over_capacity`），条子自己不算第二遍。
//! **容量超限只给建议，绝不自动截断**（ADR-0016）——而砍谁的落点是一条**排除例外**，
//! 那是浏览屏上的动作：这一屏把建议摆出来，按不动。
//!
//! ## 领域判断一条都不在这里
//!
//! 规则怎么读（`Rule::parse`）、选择集怎么求值（`sublibrary::select`）、三方对比怎么排
//! （`sync::plan`）、目标落在主库里要不要拦（`sync::prepare::refuse_target_in_library`）
//! ——全在核心。这一层只做三件事：把要来的画出来、把点的那一下写回去、把中文输入放在
//! 对的位置上。
//!
//! ## 同步与排差量预览都跑在画帧那条线程之外
//!
//! 一趟同步要搬的可能是几十 GiB。搬在画帧那条线程上，窗口就是几分钟的白板，连
//! 「停下」都点不动。所以计划一旦点头就整个搬进一条后台线程，主线程每帧只问一句
//! 「跑完没有」，外加一个真的按得动的**停下**（[`CancelToken`]）。中断的那一趟照样落清单
//! ——那份清单记的是「到中断为止目标上真实有什么」，下一趟才接得上。
//!
//! **排差量预览也一样，只是它走[任务台](crate::task)**：真机量级上它 343 毫秒，
//! 大头是走一遍全库折事实（挂账 D156）。这一屏点那个按钮，等于往任务台上排一趟活；
//! 跑完了台上把那份 [`Prepared`] 交回来（[`Screen::settle`]）。它整条只读，
//! 所以中途按停下**停在哪儿都是干净的**：一个字节都没写，再排一次就是。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::Instant;

use egui::{Align, Layout};
use romcat_core::catalog::CatalogError;
use romcat_core::report::{human_bytes, thousands};
use romcat_core::scan::CancelToken;
use romcat_core::site::Site;
use romcat_core::sublibrary::report::SelectionReport;
use romcat_core::sublibrary::{
    BrokenRule, ExceptionRow, Gauge, LoadedSelection, Rule, StoredRule, Sublibrary, Trim, rule,
};
use romcat_core::sync::{self, Act, Outcome, Prepared};
use romcat_core::task::{Done, Finished};

use crate::table::ROW_HEIGHT;
use crate::task::{Product, Tasks};

/// 差量预览摊开之后最多列几条步骤。再多就不是给人看的了——总数照旧在账上。
const TOP_STEPS: usize = 2_000;

/// 没摊开时卡上先摆几条步骤。**摆得出样子就够**：这几条回答的是「它大概要干什么」，
/// 「一共几步」那个数写在旁边，「到底哪几步」按「全部展开」。
const STEP_SAMPLE: usize = 6;

/// 意外与放不下的那几类，最多各列几条。
const TOP_NOTES: usize = 20;

/// 容量条画多高，像素。
const GAUGE_HEIGHT: f32 = 8.0;

/// 新建或改一个子库时界面上那份草稿。
///
/// **每一格都会碰到输入法**（目标路径里有中文目录名是常态），所以这几个控件全在底下那块
/// 不虚拟化的面板里（ADR-0005）。
#[derive(Debug, Clone, Default)]
pub struct Form {
    /// 子库叫什么。一台目标设备一个。
    pub name: String,
    /// 目标设备上的子库根：读卡器挂上来的那个盘上的目录。
    pub target: String,
    /// 前端格式（适配器名）。空着就是 Pegasus。
    pub format: String,
    /// 容量上限，如 `512GB`、`476GiB`。空着就是不设限。
    pub capacity: String,
    /// **能力档案**的名字。空着就是「不作声称」——不转换、不检查。
    pub capability: String,
}

impl Form {
    /// 从一个现成的子库填一份草稿。
    #[must_use]
    pub fn of(sublibrary: &Sublibrary) -> Self {
        Self {
            name: sublibrary.name.clone(),
            target: sublibrary.target.clone(),
            format: sublibrary.format.clone(),
            capacity: sublibrary.capacity.map(human_bytes).unwrap_or_default(),
            capability: sublibrary.capability.clone().unwrap_or_default(),
        }
    }
}

/// 「**改选择**」按下去之后要交给浏览屏的那一份东西。
///
/// 规则在这一屏上是**只读**的，改它的地方是浏览屏的筛选器。跳过去时带的正是这个：
/// 哪个子库、它的规则并成的那一条、以及有几条读不懂。
#[derive(Debug, Clone)]
pub struct Jump {
    /// 改的是哪个子库。
    pub sublibrary: String,
    /// 这个子库的规则并成一条（多条之间是**任一满足**，与求值同一条口径）。
    ///
    /// 一条都没有时是 `None`——那是「没有任何条件」，不是「一条都选不中」。
    pub rule: Option<Rule>,
    /// 有几条规则**读不懂**。它们没参与求值，这一趟也不会被带走或改掉。
    pub broken: usize,
}

/// 一趟正在后台跑的同步。
struct Running {
    /// 哪个子库。
    name: String,
    /// 停下用的那个信号。
    cancel: CancelToken,
    /// 什么时候开始的。
    started: Instant,
    /// 后台那条线程。
    handle: JoinHandle<Result<Outcome, String>>,
}

/// 只求了一次**选择集**的那份结果，连它花了多久。
///
/// 报告本身由核心折（[`SelectionReport::build`]）——超没超、砍谁、每条规则命中多少，
/// 与 `romcat sublibrary show` 印的是**同一个值**。这一屏另写一遍的话，
/// 「这份报告说装得下、那份说砍这几个」这种对不上的账迟早会出现
/// （`sublibrary::report` 的模块注释说的就是这件事）。
pub struct Evaluated {
    /// 报告本身。
    pub report: SelectionReport,
    /// 求它用了多久，毫秒。
    pub elapsed_ms: f64,
}

/// 子库这个屏幕。
pub struct Screen {
    /// 工作目录：中立库、**媒体池**、能力档案名册都在这儿。
    workspace: PathBuf,
    /// 库里现有的子库。**一台设备一张卡，就是这一串。**
    list: Vec<Sublibrary>,
    /// 眼下摊开的是哪一张卡。
    picked: Option<String>,
    /// 编辑草稿：底下那块「配目标」面板。
    form: Form,
    /// 摊开那个子库的规则原文，连库里的序号。**这一屏上只读。**
    rules: Vec<StoredRule>,
    /// 读不懂的那几条。**一条坏的不该让另外五条一起用不了。**
    broken: Vec<BrokenRule>,
    /// 摊开那个子库的例外。**这一屏上只数一数**：加减在浏览屏上做。
    exceptions: Vec<ExceptionRow>,
    /// 排出来的那份计划，**它就是差量预览**。
    prepared: Option<Prepared>,
    /// 正在排的那一趟差量预览是任务台上的第几号。
    ///
    /// **它同时是认领凭据**：跑完的那一趟按号对得上才收（[`Screen::settle`]）。
    /// 中途改过选择的话这个号会被 [`Screen::invalidate`] 抹掉，那一趟排出来的差量
    /// 说的已经不是眼下这套选择集会做的事了，收回来反而是骗人。
    previewing: Option<u64>,
    /// 排它用了多久，毫秒。
    prepare_ms: f64,
    /// 差量步骤摊开了没有。**默认不摊**：几百上千步铺满一屏，把它下面的按钮挤没了。
    expanded: bool,
    /// 每台设备各求一次**选择集**的结果：选中哪些、多大、超限多少、砍谁。
    ///
    /// **一趟折一次事实，全部子库共用**——折事实是走一遍全库（343 ms，挂账 D156），
    /// 而按选择集求值是内存里的事。一台一折的话，五张卡就是五趟全库。
    ///
    /// **与差量预览分开**，因为它们要的东西不一样：差量预览要目标设备在位（三方对比的
    /// 第三方就是目标上实际有什么），而「这套规则选出多少、装不装得下」只要中立库。
    /// 子库是持久实体，不是「插上卡才存在的东西」（ADR-0009）——卡不在手边时照样该
    /// 看得见容量账。
    evaluated: BTreeMap<String, Evaluated>,
    /// 计划里有删除时，要先勾这一格才动得了手。
    acknowledged: bool,
    /// 「改选择」按下去了，等窗口把它送去浏览屏（[`crate::app::App::route`]）。
    jump: Option<Jump>,
    /// 「删掉这个子库」按过一次了，等再按一次。
    ///
    /// **删的是选择集，不是文件**：规则、例外、清单跟着那条定义一起没。例外是
    /// **永久记住**的手挑决定（ADR-0016），而这一票之后规则也不在这一屏上重打得回来
    /// ——一下点掉太贵，所以要两下。
    confirm_remove: Option<String>,
    /// 正在跑的那一趟同步。
    running: Option<Running>,
    /// 上一趟同步的账。
    outcome: Option<Outcome>,
    /// 上一次动作的回执。
    notice: Option<String>,
    /// 上一趟同步有没有出岔子（被停、放弃、有步骤失败）。回执照红的画。
    failed: bool,
    /// 上一次出的错。
    error: Option<String>,
}

impl Screen {
    /// 开一个空屏幕。
    #[must_use]
    pub fn new(workspace: PathBuf) -> Self {
        Self {
            workspace,
            list: Vec::new(),
            picked: None,
            form: Form::default(),
            rules: Vec::new(),
            broken: Vec::new(),
            exceptions: Vec::new(),
            prepared: None,
            previewing: None,
            prepare_ms: 0.0,
            expanded: false,
            evaluated: BTreeMap::new(),
            acknowledged: false,
            jump: None,
            confirm_remove: None,
            running: None,
            outcome: None,
            notice: None,
            failed: false,
            error: None,
        }
    }

    /// 重新列一遍库里有哪些子库。
    pub fn reload(&mut self, site: &Site) {
        match site.catalog.sublibraries() {
            Ok(list) => {
                self.list = list;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
        if let Some(name) = self.picked.clone() {
            if self.list.iter().any(|row| row.name == name) {
                self.open(site, &name);
            } else {
                self.picked = None;
            }
        }
    }

    /// 库里现有的子库。
    #[must_use]
    pub fn list(&self) -> &[Sublibrary] {
        &self.list
    }

    /// 眼下摊开的是哪一张卡。
    #[must_use]
    pub fn picked(&self) -> Option<&str> {
        self.picked.as_deref()
    }

    /// 摊开那个子库的规则原文。**只读**：改它去浏览屏。
    #[must_use]
    pub fn rules(&self) -> &[StoredRule] {
        &self.rules
    }

    /// 读不懂的那几条规则。
    #[must_use]
    pub fn broken(&self) -> &[BrokenRule] {
        &self.broken
    }

    /// 摊开那个子库的例外。**这一屏只数一数**：加减在浏览屏上做。
    #[must_use]
    pub fn exceptions(&self) -> &[ExceptionRow] {
        &self.exceptions
    }

    /// 排出来的那份差量预览。
    #[must_use]
    pub fn prepared(&self) -> Option<&Prepared> {
        self.prepared.as_ref()
    }

    /// 排一次预览用了多久，毫秒。**任务台记的那个数**，不是界面自己掐的表。
    #[must_use]
    pub fn prepare_ms(&self) -> f64 {
        self.prepare_ms
    }

    /// 差量步骤摊开了没有。
    #[must_use]
    pub fn expanded(&self) -> bool {
        self.expanded
    }

    /// 摊开或收起差量步骤。**界面上按「全部展开」走的就是它**，测试拿它当那一下。
    ///
    /// 收起来的时候卡上只摆头几条（[`STEP_SAMPLE`]）——几百上千步铺满一屏，
    /// 会把它下面的「同步」挤到看不见的地方。摊开之后那张表是**虚拟化**的。
    pub fn expand(&mut self, on: bool) {
        self.expanded = on;
    }

    /// 正在排的那一趟差量预览是任务台上的第几号；没排着就是 `None`。
    #[must_use]
    pub fn previewing(&self) -> Option<u64> {
        self.previewing
    }

    /// 某一台设备那份求过的选择集。
    #[must_use]
    pub fn evaluated(&self, name: &str) -> Option<&Evaluated> {
        self.evaluated.get(name)
    }

    /// 某一台设备卡上那根**容量条**：选中的、清单之外的、上限。
    ///
    /// 三个数从哪儿来，[`Gauge`] 的文档写着。**清单之外只有排过差量预览才知道**，
    /// 而那份预览是对着某一台设备排的——所以只有那一台的条子有第二段。
    ///
    /// ## 条子与旁边那行「超出容量上限」必须是同一笔账
    ///
    /// 超没超由核心一处算（ADR-0016），这一屏两处摆它：排过差量的照
    /// [`Plan::over_capacity`]，没排过的照 [`SelectionReport::over_capacity`]。
    /// 于是条子的总量得**照同一条口径**填，不然会出现「条子画到九成、旁边说超了
    /// 200 MiB」——那正是 [`Gauge`] 的文档说不该发生的事。
    ///
    /// - 排过差量：`选中 = after_bytes − stranger_bytes`，于是 `taken()` 正好是
    ///   `after_bytes`——计划算超出量用的就是它。**它不等于「期望总量」**：卡上还
    ///   留着那些「对不上、因此本次不动」的文件，它们照样占地方（挂账 D76）。
    /// - 没排过：`选中 = report.bytes`、清单之外是 `None`，于是 `taken()` 正好是
    ///   `report.bytes`——报告算超出量用的就是它。
    ///
    /// [`Plan::over_capacity`]: romcat_core::sync::Plan::over_capacity
    /// [`SelectionReport::over_capacity`]: romcat_core::sublibrary::report::SelectionReport::over_capacity
    #[must_use]
    pub fn gauge(&self, name: &str) -> Gauge {
        let plan = self
            .prepared
            .as_ref()
            .filter(|prepared| prepared.sublibrary.name == name)
            .map(|prepared| &prepared.plan);
        Gauge {
            picked: plan.map_or_else(
                || {
                    self.evaluated
                        .get(name)
                        .map_or(0, |evaluated| evaluated.report.bytes)
                },
                |plan| plan.after_bytes.saturating_sub(plan.stranger_bytes),
            ),
            strangers: plan.map(|plan| plan.stranger_bytes),
            capacity: self
                .list
                .iter()
                .find(|row| row.name == name)
                .and_then(|row| row.capacity),
        }
    }

    /// 上一趟同步的账。
    #[must_use]
    pub fn outcome(&self) -> Option<&Outcome> {
        self.outcome.as_ref()
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

    /// 「配目标」那块面板上的草稿，供实测与测试填。
    pub fn form_mut(&mut self) -> &mut Form {
        &mut self.form
    }

    /// 摊开一张卡：读它的规则与例外。**排出来的那份预览当场作废**——
    /// 换了子库还留着上一个的差量，是这一屏最容易骗到人的一种写法。
    pub fn open(&mut self, site: &Site, name: &str) {
        self.picked = Some(name.to_string());
        self.confirm_remove = None;
        self.invalidate();
        if let Some(sublibrary) = self.list.iter().find(|row| row.name == name) {
            self.form = Form::of(sublibrary);
        }
        match site.catalog.selection(name) {
            Ok(loaded) => {
                self.exceptions = loaded.selection.exceptions;
                self.broken = loaded.broken;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
        match site.catalog.sublibrary_rules(name) {
            // **读得懂的与读不懂的分两栏摆**：合在一起的话，读不懂那几条会被印两遍
            // ——一遍在规则里当正常的，一遍在下面当坏的。
            Ok(stored) => {
                let broken: std::collections::BTreeSet<i64> =
                    self.broken.iter().map(|row| row.ordinal).collect();
                self.rules = stored
                    .into_iter()
                    .filter(|row| !broken.contains(&row.ordinal))
                    .collect();
            }
            Err(error) => self.error = Some(format!("中立库读不动：{error}")),
        }
    }

    /// 某一台设备的选择集或它自己被**别处**改过了：把这一屏为它缓着的东西全丢掉。
    ///
    /// 这一屏缓两样派生的东西：那份**差量预览**（[`Self::prepared`]）与**算过的选择集**
    /// （[`Self::evaluated`]）。两样的输入都躺在中立库里，而这一票之后**改它们的地方
    /// 在另一屏上**——浏览屏换规则、记例外、撤例外（票 `gui-redesign/11`）。
    /// 没有这一趟的话，屏上会摆着一份按旧规则算出来的容量与一份按旧选择集排出来的差量，
    /// 而「同步」按钮认的正是后者（ADR-0016 的「改过选择之后那份预览当场作废」）。
    ///
    /// 由 [`crate::app::App::route`] 每帧转过来——**只有窗口同时够得着两屏**。
    ///
    /// 只丢这一台的：折一趟事实全部设备共用，别人那几张卡的数还是好的。
    pub fn forget(&mut self, site: &Site, name: &str) {
        self.evaluated.remove(name);
        // 摊开的正是它的话，规则与例外也要重读一遍——卡上那几行印的就是它们。
        // `open` 自己会把差量预览作废（那份差量只可能是摊开这一台的）。
        if self.picked.as_deref() == Some(name) {
            self.open(site, name);
        }
    }

    /// 把排出来的那份预览作废。
    ///
    /// 改过选择、改过目标、改过子库本身之后都要走一趟：那份差量说的已经不是眼下这套
    /// 选择集会做的事了，而「同步」按钮认的正是它（ADR-0016）。
    ///
    /// **正在台上排着的那一趟也一并不认了**：它是照旧那套规则排的，收回来同样是骗人。
    /// 那趟活自己会跑完（整条只读，跑完也没有副作用），只是没人认领它。
    pub fn invalidate(&mut self) {
        self.prepared = None;
        self.previewing = None;
        // **耗时跟着那份差量一起作废。** 留着上一趟的数，下一趟被按停时旁边就摆着一个
        // 「排它用了 120 ms」——那说的是一份已经不在了的差量。
        self.prepare_ms = 0.0;
        self.expanded = false;
        self.acknowledged = false;
        self.outcome = None;
    }

    /// **每台设备各求一次选择集**：这套规则加例外选出什么、多大、装不装得下。
    ///
    /// **不碰目标设备**——卡不在手边时照样看得见容量账（ADR-0009）。折事实那一趟走一遍
    /// 全库，**全部子库共用它**：一台一折的话，五张卡就是五趟全库。折报告那一步整份交给
    /// 核心（[`SelectionReport::build`]），于是界面上摆着的与 `romcat sublibrary show`
    /// 印出来的是同一个值。容量超限时**只给建议，一个都不砍**（ADR-0016）。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn evaluate(&mut self, site: &Site) {
        let started = Instant::now();
        let facts = match romcat_core::sublibrary::facts(&site.catalog) {
            Ok(facts) => facts,
            Err(error) => {
                self.error = Some(format!("中立库读不动：{error}"));
                return;
            }
        };
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.evaluated.clear();
        for sublibrary in &self.list {
            let loaded = match site.catalog.selection(&sublibrary.name) {
                Ok(loaded) => loaded,
                Err(error) => {
                    self.error = Some(format!("中立库读不动：{error}"));
                    return;
                }
            };
            let selected = romcat_core::sublibrary::select(&loaded.selection, &facts);
            self.evaluated.insert(
                sublibrary.name.clone(),
                Evaluated {
                    report: SelectionReport::build(
                        site.catalog.location(),
                        sublibrary,
                        &loaded,
                        &facts,
                        &selected,
                    ),
                    // **摊在每一台头上的是同一趟折事实**：那才是这件事真花掉的时间，
                    // 按台数分摊或者各记一遍全程，两种写法印出来的都不是实情。
                    elapsed_ms,
                },
            );
        }
        self.error = None;
    }

    /// **排一次差量预览**：往[任务台](crate::task)上排一趟，跑在画帧那条线程之外。
    ///
    /// 只读：中立库读一遍、目标设备走只读接缝看一遍，一个文件都不写。因此中途按停下
    /// 停在哪儿都是干净的——**再排一次就是从头排一次**，几百毫秒的活，没有半截状态
    /// 要收拾（挂账 D156）。
    ///
    /// ## 后台那条线程读的是哪一份库
    ///
    /// `rusqlite::Connection` 不是 `Sync`，所以后台拿不到界面这条线程手里那一份。
    /// 它拿的是**同一个文件的第二份只读连接**（[`Catalog::read_only`]）：写不动、
    /// 不建表、跑完就丢，于是「两份库不一致」这条路在构造上就不存在。
    /// 只活在内存里的那种库（合成数据）分不出第二份连接，那一趟就**就地跑完**——
    /// 合成数据上它是几毫秒的事。**别的原因分不出来就直说**，不偷偷退到画帧那条线程上
    /// 跑一趟：那既会僵住窗口，又把真正的问题盖住了。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    ///
    /// [`Catalog::read_only`]: romcat_core::catalog::Catalog::read_only
    pub fn preview(&mut self, site: &Site, tasks: &mut Tasks) {
        let Some(name) = self.picked.clone() else {
            self.error = Some("先摊开一张卡。".to_string());
            return;
        };
        if self.previewing.is_some() {
            return;
        }
        self.invalidate();
        self.error = None;
        let workspace = self.workspace.clone();
        let title = format!("排差量预览 · {name}");
        self.previewing = Some(match site.catalog.read_only() {
            Ok(reader) => tasks.queue(title, move |task| {
                sync::prepare(&reader, &workspace, &name, &sync::Request::default(), task)
                    .map(|prepared| Product::Preview(Box::new(prepared)))
            }),
            // **只活在内存里的库分不出第二份连接**（合成数据走这条），那是意料之中的：
            // 这一趟就地跑完，几毫秒的事。
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here(title, |task| {
                sync::prepare(
                    &site.catalog,
                    &workspace,
                    &name,
                    &sync::Request::default(),
                    task,
                )
                .map(|prepared| Product::Preview(Box::new(prepared)))
            }),
            // 别的原因是**意外**——开这份库的时候它还好好的，文件却没了、或者结构版本
            // 对不上。这时**直说，不要退到画帧这条线程上偷偷跑一趟**：那既会僵住窗口，
            // 又把真正的问题盖在一句「怎么卡了一下」底下。
            Err(why) => {
                self.error = Some(format!(
                    "分不出第二份只读连接：{why}\n\
                     排差量预览要在画帧那条线程之外跑，而它读的是同一份中立库文件。\
                     先确认那个文件还在、版本还对得上。"
                ));
                return;
            }
        });
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去。**
    pub fn settle(&mut self, done: Finished<Product>) {
        if self.previewing != Some(done.id) {
            return;
        }
        self.previewing = None;
        match done.ended {
            Done::Product(Product::Preview(prepared)) => {
                // **只有真排出来那一趟才记耗时。** 被停下、出错的那趟什么都没排出来，
                // 摆一个「排它用了 120 ms」在旁边等于给一份不存在的差量记账。
                self.prepare_ms = done.elapsed.as_secs_f64() * 1000.0;
                self.prepared = Some(*prepared);
                self.error = None;
            }
            // 别的屏排上去的活轮不到这儿——`previewing` 那道判断已经挡掉了，
            // 这一支只为把 `Product` 那个枚举配全。
            Done::Product(_) => {}
            // **停下来的地方是干净的，就得这么说。** 说成「失败」会让人去找哪儿坏了。
            Done::Stopped => {
                self.notice = Some(
                    "排差量预览按停了。这一趟整条只读——中立库、媒体池、目标设备\
                     一个字节都没动，再排一次就是。"
                        .to_string(),
                );
                self.failed = false;
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Done::Failed { step, why } => {
                self.error = Some(if step.is_empty() {
                    why
                } else {
                    format!("排差量预览在「{step}」这一步停下了：{why}")
                });
            }
        }
    }

    /// 「**改选择**」：把这个子库的规则并成一条，交给窗口送去浏览屏。
    ///
    /// 这一屏不改选择集，所以这一下**什么都没写**——它只是把要改的东西装好。
    /// 真正的跳转由 [`crate::app::App::route`] 走：那儿才同时够得着两屏。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_selection(&mut self) {
        let Some(name) = self.picked.clone() else {
            self.error = Some("先摊开一张卡。".to_string());
            return;
        };
        // 读得懂的并成一条，读不懂的只数一数：它们本来就没参与求值，这一趟也不碰。
        let loaded = LoadedSelection::from_stored(&self.rules);
        self.jump = Some(Jump {
            sublibrary: name,
            rule: Rule::any_of(loaded.selection.rules),
            broken: self.broken.len(),
        });
    }

    /// 把「改选择」那一下取走。**取过就没了**：窗口一帧问一次。
    pub fn take_jump(&mut self) -> Option<Jump> {
        self.jump.take()
    }

    /// **同步**：把差量真正落到目标设备上，跑在后台线程里。
    ///
    /// 三道闸一道都不能少：**得先有预览**（ADR-0016）、**有删除就得先点头**（ADR-0015）、
    /// **目标不许落在主库里**（ADR-0004，判据在核心里）。
    pub fn sync(&mut self, site: &Site) {
        if self.running.is_some() {
            return;
        }
        let Some(prepared) = self.prepared.clone() else {
            // 这句话不是提示，是这一屏的规矩：没预览就没有可传的东西。
            self.error = Some("还没排过差量预览。先看一遍它要做什么（ADR-0016）。".to_string());
            return;
        };
        if prepared.plan.deletes.files > 0 && !self.acknowledged {
            self.error = Some(format!(
                "这份计划里有 {} 个**删除**（{}）。看过上面的预览之后，勾上「我看过删除清单」再来。",
                thousands(prepared.plan.deletes.files),
                human_bytes(prepared.plan.deletes.bytes),
            ));
            return;
        }
        if let Err(message) =
            sync::prepare::refuse_target_in_library(&site.catalog, &[], &prepared.root)
        {
            self.error = Some(message);
            return;
        }
        // 搬 ROM 要真的去读主库——**只有真要搬时才需要**：一趟只删文件、只重写元数据的
        // 同步，盘不在位照样跑得完（ADR-0009）。
        let library_roots = if prepared.needs_library() {
            match sync::prepare::library_roots(&site.catalog, &[]) {
                Ok(roots) => {
                    // 只看这一趟真要搬的那几个根在不在位——**别的盘挂没挂上与这趟无关**。
                    let missing = sync::prepare::missing_roots(&prepared, &roots);
                    if !missing.is_empty() {
                        self.error = Some(sync::prepare::missing_roots_message(&missing));
                        return;
                    }
                    Some(roots)
                }
                Err(message) => {
                    self.error = Some(message);
                    return;
                }
            }
        } else {
            None
        };

        let name = prepared.sublibrary.name.clone();
        let cancel = CancelToken::new();
        let token = cancel.clone();
        let handle =
            std::thread::spawn(move || run_sync(&prepared, library_roots.as_ref(), &token));
        self.running = Some(Running {
            name,
            cancel,
            started: Instant::now(),
            handle,
        });
        self.error = None;
        self.notice = None;
        self.failed = false;
    }

    /// 正在跑的那一趟同步跑完没有。跑完了就收账、把**清单**落回中立库。
    ///
    /// 每帧问一次。**中断的那一趟也要落清单**——那份清单记的是「到中断为止目标上真实有
    /// 什么」，下一趟才接得上。
    pub fn poll(&mut self, site: &mut Site) {
        let Some(running) = &self.running else {
            return;
        };
        if !running.handle.is_finished() {
            return;
        }
        let Some(running) = self.running.take() else {
            return;
        };
        let elapsed = running.started.elapsed().as_secs_f64();
        match running.handle.join() {
            Ok(Ok(outcome)) => {
                if let Err(error) = site.catalog.put_manifest(&running.name, &outcome.manifest) {
                    self.error = Some(format!(
                        "⚠️ 清单写不回中立库：{error}\n\
                         目标上的文件已经动过了，而清单还是旧的那一份——下一趟同步会把这次\n\
                         放上去的东西当成「清单之外」，于是碰都不敢碰。先修好中立库再跑一次。"
                    ));
                }
                // **中断、失败、主动停了，一样都不许吞。** 全成功的一趟与半数写失败的
                // 一趟若在界面上长得一样，那句「同步用了 X 秒」就是在骗人
                // （命令行那一侧靠 `Outcome::render_text` 把这几样印全）。
                let mut line = format!(
                    "同步用了 {elapsed:.1} 秒：动了 {} 个文件（新增 {}、更新 {}、删除 {}）。",
                    thousands(outcome.touched()),
                    thousands(outcome.added.files),
                    thousands(outcome.updated.files),
                    thousands(outcome.deleted.files),
                );
                if outcome.interrupted {
                    line.push_str(
                        "\n⚠️ **这一趟被你按停了**：目标上没有半份文件，清单记的是\
                                   到中断为止真实有什么，再跑一趟就接上。",
                    );
                }
                if outcome.gave_up {
                    line.push_str("\n⚠️ **连着失败太多次，主动停了**：多半是卡拔了或者写满了。");
                }
                if !outcome.failures.is_empty() {
                    line.push_str(&format!(
                        "\n⚠️ **有 {} 步没做成**：",
                        thousands(outcome.failures.len() as u64),
                    ));
                    for failure in outcome.failures.iter().take(TOP_NOTES) {
                        line.push_str(&format!("\n  {}：{}", failure.path, failure.why));
                    }
                    if outcome.failures.len() > TOP_NOTES {
                        line.push_str(&format!(
                            "\n  ……另有 {} 步没列",
                            outcome.failures.len() - TOP_NOTES,
                        ));
                    }
                }
                self.failed =
                    outcome.interrupted || outcome.gave_up || !outcome.failures.is_empty();
                self.notice = Some(line);
                self.outcome = Some(outcome);
                // 传完之后那份预览说的已经是过去时了：目标现在是另一个样子。
                self.prepared = None;
                self.acknowledged = false;
            }
            Ok(Err(message)) => self.error = Some(message),
            Err(_) => self.error = Some("同步那条线程炸了。".to_string()),
        }
    }

    /// 画一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        self.poll(site);
        if self.running.is_some() {
            // 后台在跑，主线程得继续画，不然「停下」按钮按不动。
            ui.ctx().request_repaint();
        }
        // 这条边界拖得动也记得住，声明在 [`crate::layout`]（票 `gui-redesign/12`）。
        crate::layout::TARGET.show(ui, |ui| self.form_ui(ui, site));
        egui::CentralPanel::default().show(ui, |ui| self.cards_ui(ui, site, tasks));
    }

    /// 顶栏上属于这一屏的那一段。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &Site) {
        if ui.button("重新列一遍").clicked() {
            self.reload(site);
        }
        ui.separator();
        ui.label(format!("{} 台设备", self.list.len()));
        if let Some(running) = &self.running {
            ui.separator();
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "正在同步「{}」，{:.0} 秒",
                    running.name,
                    running.started.elapsed().as_secs_f64()
                ),
            );
            if ui.button("停下").clicked() {
                running.cancel.cancel();
            }
        }
    }

    /// 中间那一列：**一台设备一张卡**。
    fn cards_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if let Some(notice) = &self.notice {
            let color = if self.failed {
                ui.visuals().error_fg_color
            } else {
                ui.visuals().warn_fg_color
            };
            ui.colored_label(color, notice);
        }
        ui.horizontal(|ui| {
            ui.strong("子库");
            ui.weak("一台目标设备一张卡。**这一屏不选内容**——改选择跳回浏览屏。");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("+ 新建")
                    .on_hover_text("底下那块面板填名字与目标路径。**选择集去浏览屏筛**：筛到满意按「存成子库」。")
                    .clicked()
                {
                    self.picked = None;
                    self.confirm_remove = None;
                    self.form = Form::default();
                    self.invalidate();
                }
                if ui
                    .button("算一遍容量")
                    .on_hover_text(
                        "只问中立库：每台设备的选择集各选出多少、多大、装不装得下。\
                         **卡不在手边也算得出来**。折一次事实，全部设备共用。",
                    )
                    .clicked()
                {
                    self.evaluate(site);
                }
            });
        });
        ui.separator();
        if self.list.is_empty() {
            ui.weak("一台设备都还没有。底下那块面板填个名字与目标路径，或者去浏览屏筛一批按「存成子库」。");
            return;
        }
        let names: Vec<String> = self.list.iter().map(|row| row.name.clone()).collect();
        egui::ScrollArea::vertical()
            .id_salt("设备卡片")
            .show(ui, |ui| {
                for name in names {
                    self.card_ui(ui, site, tasks, &name);
                    ui.add_space(8.0);
                }
            });
    }

    /// 一台设备那一张卡。
    fn card_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks, name: &str) {
        let Some(sublibrary) = self.list.iter().find(|row| row.name == name).cloned() else {
            return;
        };
        let open = self.picked.as_deref() == Some(name);
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(open, egui::RichText::new(&sublibrary.name).strong())
                    .clicked()
                {
                    if open {
                        self.picked = None;
                        self.invalidate();
                    } else {
                        self.open(site, name);
                    }
                }
                // **待同步步数**：排过差量的那一台才有。没排过就说没排过，不摆一个 0
                // ——「一步都不用做」与「还不知道要做什么」是两件事。
                match self.plan_of(name) {
                    Some(plan) if plan.touched() > 0 => {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("待同步 {} 步", thousands(plan.touched())),
                        );
                    }
                    Some(_) => {
                        ui.weak("已经对齐，一步都不用做");
                    }
                    None => {
                        ui.weak("还没排过差量");
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // **只有摊开那一张摆得出这个按钮**：它带走的是「这一台的规则」，
                    // 而规则要等 `open` 读回来才在手上。摆一个灰的在每张卡上只是噪音。
                    if open
                        && ui
                            .button("改选择…")
                            .on_hover_text(
                                "跳去浏览屏，**这个子库的规则预填进筛选器**。\
                                 在那儿改得见它真的筛出了什么；调完按「更新到子库」原样带回。",
                            )
                            .clicked()
                    {
                        self.edit_selection();
                    }
                });
            });
            ui.label(
                egui::RichText::new(format!(
                    "{}｜{}｜能力档案 {}｜上限 {}",
                    sublibrary.target,
                    sublibrary.format,
                    sublibrary.capability.as_deref().unwrap_or("不作声称"),
                    sublibrary
                        .capacity
                        .map_or_else(|| "不设限".to_string(), human_bytes),
                ))
                .weak(),
            );
            if !open {
                ui.weak("点名字摊开：选择集、容量、差量都在里头。");
                return;
            }
            ui.add_space(6.0);
            self.selection_ui(ui, name);
            ui.add_space(6.0);
            self.gauge_ui(ui, name);
            ui.add_space(6.0);
            self.delta_ui(ui, site, tasks);
        });
    }

    /// 排出来那份计划，若它正好是这台设备的。
    fn plan_of(&self, name: &str) -> Option<&romcat_core::sync::Plan> {
        self.prepared
            .as_ref()
            .filter(|prepared| prepared.sublibrary.name == name)
            .map(|prepared| &prepared.plan)
    }

    /// **选择集：只读展示。** 规则几条、各命中多少、例外几条、有没有引到空的维度。
    ///
    /// 改它按上面「改选择」——**这一屏一个写的动作都没有**。但**看**得尽量全：
    /// 「这条规则写对了吗」「为什么一个都没选中」这两个问题只有在摆着规则的地方才答得了，
    /// 而答案整份来自核心折的 [`SelectionReport`]（与 `romcat sublibrary show`
    /// 印的是同一个值），按过「算一遍容量」才有。
    fn selection_ui(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.horizontal(|ui| {
            ui.strong("选择集");
            ui.weak("只读——改它按上面「改选择」")
                .on_hover_text(
                    "规则与例外都在**浏览屏**上改：在那儿改得见它真的筛出了什么，\
                     在这儿改只看得见一行字。这一屏管的是「送到哪」。",
                );
        });
        let report = self.evaluated.get(name).map(|evaluated| &evaluated.report);
        if let Some(report) = report {
            ui.label(format!(
                "选出 {} / {} 个变体，分属 {} 个「作品 × 平台」",
                thousands(report.picked),
                thousands(report.variants),
                thousands(report.anchors),
            ));
        }
        if self.rules.is_empty() && self.broken.is_empty() {
            ui.weak("一条规则都没有——选择集是空的，同步过去也是空的。");
        }
        for stored in &self.rules {
            // **逐条各自算，不扣例外也不扣重叠**：这个数回答的是「我这条规则写对了吗」。
            let hits = report.and_then(|report| {
                report
                    .rules
                    .iter()
                    .find(|line| line.ordinal == stored.ordinal)
                    .map(|line| line.hits)
            });
            match hits {
                Some(hits) => ui
                    .label(format!(
                        "{} 个 ← {}. {}",
                        thousands(hits),
                        stored.ordinal,
                        stored.text,
                    ))
                    .on_hover_text("这一条自己命中多少个变体——不扣例外，也不扣与别条重叠的。"),
                None => ui.label(format!("{}. {}", stored.ordinal, stored.text)),
            };
        }
        for row in &self.broken {
            ui.colored_label(
                ui.visuals().error_fg_color,
                format!(
                    "{}. {}（读不懂：{}）——少选出来的东西全在它里面；\
                     它没参与求值，「改选择」也不会碰它",
                    row.ordinal, row.text, row.error,
                ),
            );
        }
        if !self.broken.is_empty() {
            // **界面上没有删它的路，那就把有路的那条说出来**（挂单 `Q86`）。
            // 筛选器摆的是一棵读得懂的树，一条读不回来的原文在那儿没有位置；
            // 而「改选择」只换读得懂的那几条（不然一次改选择会悄悄清掉人还没来得及修的
            // 东西）。于是这几条眼下只有命令行改得动——不说的话，人会一直找那颗删除键。
            ui.weak(format!(
                "读不懂的这 {} 条**界面上改不动**：按序号去命令行删\
                 （`romcat sublibrary rule <子库> --remove <序号>`），\
                 或者改对了再 `--add` 一条。",
                self.broken.len(),
            ));
        }
        let (收入, 排除) = exception_tally(&self.exceptions);
        if self.exceptions.is_empty() {
            ui.weak("一条例外都没有。");
        } else {
            let 顶用的 = report.map_or_else(String::new, |report| {
                format!(
                    "（其中 {} 条收入是多余的、{} 条排除真起了作用）",
                    thousands(report.forced_in_redundant),
                    thousands(report.forced_out_effective),
                )
            });
            ui.label(format!(
                "{} 条例外：收入 {}、排除 {}{顶用的}",
                thousands(self.exceptions.len() as u64),
                thousands(收入),
                thousands(排除),
            ))
            .on_hover_text(
                "**优先于规则、永久记住**：规则表达不了的个人口味（ADR-0016）。\
                 加减在浏览屏的详情面板里做。",
            );
        }
        let Some(report) = report else {
            return;
        };
        if !report.missing_exceptions.is_empty() {
            // **不是错误，也不删**：盘没插、目录改了名，例外照旧记着（ADR-0016）。
            ui.weak(format!(
                "有 {} 条例外指着库里眼下没有的变体——照旧记着，不删。",
                thousands(report.missing_exceptions.len() as u64),
            ));
        }
        if !report.thin_dimensions.is_empty() {
            // **「选不出东西」有两个原因，得分得开**：规则写错了，还是这份库里根本
            // 没有这一维的数据。不说的话人会去改规则，而问题在刮削还没跑。
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "规则引到了这几维，而这份库里一条数据都没有：{}。选不出东西是\
                     **缺数据**，不是规则写错了。",
                    report.thin_dimensions.join("、"),
                ),
            );
        }
    }

    /// **容量条**：选中的、清单之外的、上限，三段各自标得出数。
    fn gauge_ui(&mut self, ui: &mut egui::Ui, name: &str) {
        let gauge = self.gauge(name);
        ui.strong("容量");
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), GAUGE_HEIGHT),
            egui::Sense::hover(),
        );
        let painter = ui.painter();
        let rounding = GAUGE_HEIGHT / 2.0;
        painter.rect_filled(rect, rounding, ui.visuals().extreme_bg_color);
        let 选中色 = ui.visuals().selection.bg_fill;
        let 之外色 = ui.visuals().warn_fg_color;
        let mut x = rect.left();
        for (share, color) in [
            (gauge.picked_share(), 选中色),
            (gauge.stranger_share(), 之外色),
        ] {
            let width = rect.width() * share;
            if width <= 0.0 {
                continue;
            }
            let part = egui::Rect::from_min_size(
                egui::pos2(x, rect.top()),
                egui::vec2(width, rect.height()),
            );
            painter.rect_filled(part, rounding, color);
            x += width;
        }
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(选中色, "■");
            ui.label(format!("选中 {}", human_bytes(gauge.picked)))
                .on_hover_text(
                    "这个子库在卡上占的地方。**排过差量预览之后**算的是同步完的样子\
                     ——元数据与媒体也要占地方，转换又省下来一些，而卡上还留着那些\
                     「对不上、本次不动」的文件。没排过时就是选择集选出来那批变体一共多大。",
                );
            ui.separator();
            ui.colored_label(之外色, "■");
            match gauge.strangers {
                // **「还不知道」不画成零**：卡不在手边时目标上有什么本来就没看过，
                // 摆一个 0 出去等于说「卡上是空的」。
                None => {
                    ui.weak("清单之外 —（排一次差量预览才知道）");
                }
                Some(bytes) => {
                    ui.label(format!("清单之外 {}", human_bytes(bytes)))
                        .on_hover_text("工具没放过的文件：维护者自己拷进去的存档、金手指、截图。**连看都不看**（ADR-0015）。");
                }
            }
            ui.separator();
            match gauge.capacity {
                None => ui.weak("上限 不设限"),
                Some(bytes) => ui.weak(format!("上限 {}", human_bytes(bytes))),
            };
        });
        if gauge.picked == 0 && !self.evaluated.contains_key(name) && self.prepared.is_none() {
            ui.weak("还没算过：按右上角「算一遍容量」，或者排一次差量预览。");
        }
        // 超没超由核心一处算，这儿只把它摆出来。排过差量的那一台照计划里那份
        // （它把清单之外的占用也算进去了），没排过的照选择集那份。
        let over = self.plan_of(name).map_or_else(
            || {
                self.evaluated
                    .get(name)
                    .and_then(|evaluated| evaluated.report.over_capacity)
            },
            |plan| plan.over_capacity,
        );
        if let Some(over) = over {
            let trims = self.plan_of(name).map_or_else(
                || {
                    self.evaluated
                        .get(name)
                        .map(|evaluated| evaluated.report.trim_suggestions.clone())
                        .unwrap_or_default()
                },
                |plan| plan.trim_suggestions.clone(),
            );
            trim_ui(ui, over, gauge.capacity, &trims);
        }
    }

    /// 卡的下半截：**排差量、看步骤、按同步**。
    fn delta_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        ui.horizontal(|ui| {
            let 排着 = self.previewing.is_some();
            if ui
                .add_enabled(
                    self.running.is_none() && !排着,
                    egui::Button::new(if self.prepared.is_some() {
                        "重排差量"
                    } else {
                        "排差量预览"
                    }),
                )
                .on_hover_text(
                    "只读：中立库读一遍、目标设备看一遍，一个文件都不写。\
                     插上读卡器再点——目标不在位时它会直说。\
                     它进**任务队列**跑，期间这一屏照常用。",
                )
                .clicked()
            {
                self.preview(site, tasks);
            }
            // 正排着的时候把进度摆在按钮旁边：人是在这一屏点的，不该逼他先切去任务屏
            // 才知道排到哪儿了。**停下也在这儿按得着。**
            if let Some(id) = self.previewing
                && let Some(live) = tasks.running().filter(|live| live.id == id)
            {
                ui.weak(format!(
                    "正在排：{}（已用 {:.1} 秒）",
                    live.progress.render(),
                    live.elapsed.as_secs_f64(),
                ));
                if live.stopping {
                    ui.colored_label(ui.visuals().warn_fg_color, "正在停……");
                } else if ui.button("停下").clicked() {
                    tasks.stop(id);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let name = self.picked.clone().unwrap_or_default();
                let 问过了 = self.confirm_remove.as_deref() == Some(name.as_str());
                let label = if 问过了 {
                    format!("真的删掉「{name}」")
                } else {
                    "删掉这个子库".to_string()
                };
                let button = if 问过了 {
                    egui::Button::new(egui::RichText::new(label).color(ui.visuals().error_fg_color))
                } else {
                    egui::Button::new(label)
                };
                if ui
                    .add(button)
                    .on_hover_text(format!(
                        "只删中立库里的这条定义与它的规则、例外、清单；目标设备上的文件\
                         一个都不碰。**要按两下**：{} 条规则与 {} 条例外跟着一起没——\
                         例外是手挑的、**永久记住**的决定（ADR-0016），\
                         规则也不在这一屏上重打得回来。",
                        self.rules.len() + self.broken.len(),
                        self.exceptions.len(),
                    ))
                    .clicked()
                {
                    if 问过了 {
                        self.remove(site);
                    } else {
                        self.confirm_remove = Some(name);
                    }
                }
                if 问过了 && ui.button("算了").clicked() {
                    self.confirm_remove = None;
                }
            });
        });
        let Some(prepared) = self.prepared.clone() else {
            ui.weak(
                "还没有差量预览。**同步前必须先看一遍它要做什么**——那是硬要求，\
                 不是可以跳过的一步（ADR-0016）。",
            );
            return;
        };
        self.plan_ui(ui, site, &prepared);
    }

    /// 那份计划本身：账、要说出口的怪事、步骤，以及「真的传」。
    fn plan_ui(&mut self, ui: &mut egui::Ui, site: &Site, prepared: &Prepared) {
        let plan = &prepared.plan;
        ui.separator();
        tally_ui(ui, plan, self.prepare_ms);
        concerns_ui(ui, prepared);
        self.steps_ui(ui, plan);
        self.sync_ui(ui, site, plan);
    }

    /// 步骤那一段：收起来时摆头几条，摊开是那张虚拟化的表。
    fn steps_ui(&mut self, ui: &mut egui::Ui, plan: &romcat_core::sync::Plan) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.strong(format!(
                "这一趟要动的 {} 步（先删后传）",
                thousands(plan.touched())
            ));
            if plan.steps.len() > STEP_SAMPLE {
                let label = if self.expanded { "收起来" } else { "全部展开" };
                if ui.button(label).clicked() {
                    self.expanded = !self.expanded;
                }
            }
        });
        if self.expanded {
            steps_table(ui, plan);
            return;
        }
        for step in plan.steps.iter().take(STEP_SAMPLE) {
            ui.horizontal(|ui| {
                if step.act == Act::Delete {
                    ui.colored_label(ui.visuals().error_fg_color, step.act.label());
                } else {
                    ui.label(step.act.label());
                }
                ui.weak(step.kind.label());
                ui.label(&step.path);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.weak(human_bytes(if step.act == Act::Delete {
                        step.was
                    } else {
                        step.bytes
                    }));
                });
            });
        }
        if plan.steps.len() > STEP_SAMPLE {
            ui.weak(format!(
                "……另有 {} 步没列，按「全部展开」看全",
                plan.steps.len() - STEP_SAMPLE
            ));
        }
    }

    /// 「真的传」那一行，连**有删除就得先点头**那一格（ADR-0015）。
    fn sync_ui(&mut self, ui: &mut egui::Ui, site: &Site, plan: &romcat_core::sync::Plan) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if plan.deletes.files > 0 {
                ui.checkbox(
                    &mut self.acknowledged,
                    format!(
                        "我看过删除清单（{} 个，{}）",
                        thousands(plan.deletes.files),
                        human_bytes(plan.deletes.bytes),
                    ),
                );
            }
            let ready = self.running.is_none()
                && plan.touched() > 0
                && (plan.deletes.files == 0 || self.acknowledged);
            if ui
                .add_enabled(ready, egui::Button::new("同步"))
                .on_hover_text("把上面这份差量真的落到目标设备上。只碰清单里记录过的文件。")
                .clicked()
            {
                self.sync(site);
            }
            if plan.touched() == 0 {
                ui.label("一步都不用做：目标已经和选择集对齐了。");
            }
        });
    }
}

/// 差量的账：新增 / 更新 / 删除 / 原样留着各几个文件、几个变体、多大，加上净变化。
fn tally_ui(ui: &mut egui::Ui, plan: &romcat_core::sync::Plan, prepare_ms: f64) {
    egui::Grid::new(format!("差量账 · {}", plan.sublibrary))
        .num_columns(4)
        .spacing([16.0, 4.0])
        .show(ui, |ui| {
            ui.strong("");
            ui.strong("文件");
            ui.strong("变体");
            ui.strong("容量");
            ui.end_row();
            for (what, tally) in [
                ("新增", plan.adds),
                ("更新", plan.updates),
                ("删除", plan.deletes),
                ("原样留着", plan.keeps),
            ] {
                ui.label(what);
                ui.label(thousands(tally.files));
                ui.label(thousands(tally.variants));
                ui.label(human_bytes(tally.bytes));
                ui.end_row();
            }
        });
    let net = if plan.net_bytes >= 0 {
        format!("＋{}", human_bytes(plan.net_bytes.unsigned_abs()))
    } else {
        format!("－{}", human_bytes(plan.net_bytes.unsigned_abs()))
    };
    ui.label(format!(
        "净变化 {net}；同步完之后目标上占 {}（眼下 {}）｜排它用了 {prepare_ms:.0} ms",
        human_bytes(plan.after_bytes),
        human_bytes(plan.actual_bytes),
    ));
}

/// 折期望状态与排计划时那几件**要说出口**的怪事：核心报的那几条、**放不进目标的**、
/// **目标吃不下而这一版转不了的**、以及目标上对不上的那些。
///
/// 头一段整份来自核心（[`Prepared::concerns`]），**命令行与界面印同一份**：
/// 各写一遍的话，界面上会少掉其中一两条——而这几条正是「为什么这一趟少选出来这么多」
/// 的答案。
///
/// 后面几段以前这一屏一条都不画（只有命令行的 `Plan::render_text` 印）。
/// **放不进目标的**里头就有**落点撞车**（挂单 Q57 点名要这一票把它报出来）：
/// 两个根里同一条相对路径落在卡上同一个文件上——子库里的落点剥掉了根名（ADR-0013），
/// 于是它们撞在一起，而**撞上的一个都不放行**。人不知道这件事的话，
/// 会对着「明明选中了却没传过去」发呆。
fn concerns_ui(ui: &mut egui::Ui, prepared: &Prepared) {
    let plan = &prepared.plan;
    for concern in prepared.concerns() {
        ui.colored_label(ui.visuals().warn_fg_color, concern);
    }
    if !plan.rejected.is_empty() {
        ui.colored_label(
            ui.visuals().error_fg_color,
            format!(
                "{} 份**放不进目标**，这一趟既不新增也不删除——它们进不了卡，\
                 而「放不进去」这个判断本身也可能是错的，删掉别人的东西不可逆：",
                thousands(plan.rejected.len() as u64),
            ),
        );
        for row in plan.rejected.iter().take(TOP_NOTES) {
            ui.label(format!(
                "{}｜{}｜{}{}",
                row.reason.label(),
                row.path,
                human_bytes(row.bytes),
                if row.estimated { "（估的）" } else { "" },
            ))
            .on_hover_text(&row.detail);
        }
        if plan.rejected.len() > TOP_NOTES {
            ui.weak(format!(
                "……另有 {} 份没列",
                plan.rejected.len() - TOP_NOTES
            ));
        }
    }
    if !plan.unsupported.is_empty() {
        // **照搬，但点名说出口**（ADR-0017：矩阵错了比不转换更糟）。不说的话，
        // 人会以为工具已经替他处理妥当，直到在掌机上打不开才发现。
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "{} 份目标**吃不下、而这一版转不了**：照样传过去，但它在这台设备上多半打不开。",
                thousands(plan.unsupported.len() as u64),
            ),
        );
        for row in plan.unsupported.iter().take(TOP_NOTES) {
            ui.label(format!("{}｜{}｜要的是 {}", row.path, human_bytes(row.bytes), row.want))
                .on_hover_text(&row.why);
        }
        if plan.unsupported.len() > TOP_NOTES {
            ui.weak(format!(
                "……另有 {} 份没列",
                plan.unsupported.len() - TOP_NOTES
            ));
        }
    }
    if plan.surprises.is_empty() {
        return;
    }
    ui.colored_label(
        ui.visuals().warn_fg_color,
        format!(
            "{} 件对不上的事，**本次一律不动它们**：",
            thousands(plan.surprises.len() as u64)
        ),
    );
    for surprise in plan.surprises.iter().take(TOP_NOTES) {
        ui.label(format!(
            "{}｜{}｜{}",
            surprise.kind.label(),
            surprise.path,
            if surprise.still_wanted {
                "选择集还要它"
            } else {
                "选择集已经不要它了"
            },
        ));
    }
    if plan.surprises.len() > TOP_NOTES {
        ui.weak(format!(
            "……另有 {} 件没列",
            plan.surprises.len() - TOP_NOTES
        ));
    }
}

impl Screen {
    /// 底下那块面板：**配目标**。名字、目标路径、前端格式、容量上限、能力档案。
    ///
    /// **这一屏上全部中文输入都在这里**：面板不虚拟化，正在组字的那一行不会凭空消失
    /// （ADR-0005 的修订段）。
    fn form_ui(&mut self, ui: &mut egui::Ui, site: &mut Site) {
        egui::ScrollArea::vertical()
            .id_salt("配目标")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("配目标");
                    ui.weak("这台设备是什么样的。**要什么内容**去浏览屏筛。");
                });
                egui::Grid::new("配目标格")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        for (label, value, hint) in [
                            ("名字", &mut self.form.name, "一台目标设备一个"),
                            ("目标路径", &mut self.form.target, "读卡器挂上来的那个目录"),
                            ("前端格式", &mut self.form.format, "空着就是 Pegasus"),
                            ("容量上限", &mut self.form.capacity, "如 512GB；空着不设限"),
                            ("能力档案", &mut self.form.capability, "空着就是不作声称"),
                        ] {
                            ui.label(label);
                            ui.add(
                                egui::TextEdit::singleline(value)
                                    .desired_width(f32::INFINITY)
                                    .hint_text(hint),
                            );
                            ui.end_row();
                        }
                    });
                let ready =
                    !self.form.name.trim().is_empty() && !self.form.target.trim().is_empty();
                if ui
                    .add_enabled(ready, egui::Button::new("存下来"))
                    .on_hover_text(
                        "新建或改写这台设备。**目标设备不在位也存得下**——子库是持久实体。",
                    )
                    .clicked()
                {
                    self.save(site);
                }
            });
    }

    /// 存下这个子库。**界面上按那个按钮走的就是它**，实测与测试拿它当那一下。
    ///
    /// **目标落在主库里当场拦下**（ADR-0004、验收第 8 条）：判据在核心里
    /// （`sync::prepare::refuse_target_in_library`），与同步那一道是同一条。
    /// 拦在存下来这一步而不是等到同步，是因为一个指着主库的子库定义放在库里，
    /// 下一次点同步之前谁都不知道它错了。
    pub fn save(&mut self, site: &mut Site) {
        let capacity = if self.form.capacity.trim().is_empty() {
            None
        } else {
            match rule::parse_size(self.form.capacity.trim()) {
                Some(bytes) => Some(bytes),
                None => {
                    self.error = Some(format!(
                        "看不懂容量「{}」。写成 `512GB` 或 `476GiB` 那样，单位得写全。",
                        self.form.capacity.trim(),
                    ));
                    return;
                }
            }
        };
        let name = self.form.name.trim().to_string();
        let target = std::path::PathBuf::from(self.form.target.trim());
        if let Err(message) =
            sync::prepare::refuse_target_in_library(&site.catalog, &[], &target)
        {
            self.error = Some(message);
            return;
        }
        let format = if self.form.format.trim().is_empty() {
            "Pegasus".to_string()
        } else {
            self.form.format.trim().to_string()
        };
        // 两种路径形式怎么折，**由核心的 `Sublibrary::at` 一处说了算**（ADR-0020）。
        let mut sublibrary = Sublibrary::at(&name, &target, &format, capacity);
        sublibrary.capability =
            Some(self.form.capability.trim().to_string()).filter(|value| !value.is_empty());
        match site.catalog.put_sublibrary(&sublibrary) {
            Ok(()) => {
                self.notice = Some(format!("存下了子库「{name}」。"));
                // **算过的那份跟着作废**：容量上限改了，报告里的「超出多少、砍谁」
                // 说的还是上一个上限——卡上会出现「上限写着 1 TB、旁边说超了 200 GiB」。
                self.evaluated.remove(&name);
                self.reload(site);
                self.open(site, &name);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 删掉这个子库的定义。**目标设备上的文件一个都不碰。**
    pub fn remove(&mut self, site: &mut Site) {
        let Some(name) = self.picked.clone() else {
            return;
        };
        match site.catalog.remove_sublibrary(&name) {
            Ok(true) => {
                self.notice = Some(format!(
                    "删掉了子库「{name}」的定义。目标设备上的文件一个都没动。"
                ));
                self.picked = None;
                self.confirm_remove = None;
                self.evaluated.remove(&name);
                self.invalidate();
                self.reload(site);
            }
            Ok(false) => self.notice = Some("那个子库已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }
}

/// 例外分两向各有几条。
fn exception_tally(rows: &[ExceptionRow]) -> (u64, u64) {
    let mut 收入 = 0;
    let mut 排除 = 0;
    for row in rows {
        match row.kind {
            romcat_core::sublibrary::Exception::Include => 收入 += 1,
            romcat_core::sublibrary::Exception::Exclude => 排除 += 1,
        }
    }
    (收入, 排除)
}

/// 摊开之后那张步骤表。**虚拟化**：几千步滚起来的代价与总步数无关。
fn steps_table(ui: &mut egui::Ui, plan: &romcat_core::sync::Plan) {
    let shown = plan.steps.len().min(TOP_STEPS);
    egui_extras::TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .id_salt(format!("差量步骤 · {}", plan.sublibrary))
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(egui_extras::Column::initial(60.0).at_least(50.0))
        .column(egui_extras::Column::initial(70.0).at_least(50.0))
        .column(egui_extras::Column::initial(90.0).at_least(70.0))
        .column(egui_extras::Column::remainder().at_least(160.0).clip(true))
        .header(22.0, |mut header| {
            for title in ["干什么", "类别", "容量", "目标上的路径"] {
                header.col(|ui| {
                    ui.strong(title);
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, shown, |mut row| {
                let index = row.index();
                let Some(step) = plan.steps.get(index) else {
                    return;
                };
                row.col(|ui| {
                    if step.act == Act::Delete {
                        ui.colored_label(ui.visuals().error_fg_color, step.act.label());
                    } else {
                        ui.label(step.act.label());
                    }
                });
                row.col(|ui| {
                    ui.label(step.kind.label());
                });
                row.col(|ui| {
                    ui.label(human_bytes(if step.act == Act::Delete {
                        step.was
                    } else {
                        step.bytes
                    }));
                });
                row.col(|ui| {
                    ui.label(&step.path);
                });
            });
        });
    if plan.steps.len() > shown {
        ui.weak(format!("……另有 {} 步没列", plan.steps.len() - shown));
    }
}

/// 后台那条线程干的活：把计划落到目标上。
///
/// **它拿到的只有计划里那些步骤**——清单之外的路径连进来的门都没有（ADR-0015）。
fn run_sync(
    prepared: &Prepared,
    library_roots: Option<&romcat_core::catalog::Roots>,
    cancel: &CancelToken,
) -> Result<Outcome, String> {
    let sources = sync::Sources {
        library: &romcat_core::fs::RealFs,
        library_roots,
        target_root: &prepared.root,
        from_pool: &prepared.from_pool,
        generated: &prepared.generated,
        // 探测的源那一头是**媒体池自己的临时目录**：两头都得是工具的地盘，
        // 拿主库里的文件去试链接会改到主库那一侧的 inode（ADR-0004）。
        link_probe_dir: Some(&prepared.scratch),
        // **默认不缓存**（ADR-0017）：边转边流式写进目标。
        convert_cache: None,
    };
    sync::execute::run(
        &prepared.plan,
        &prepared.desired,
        &prepared.actual,
        &prepared.manifest,
        &sources,
        cancel,
    )
    .map_err(|error| format!("目标写不了：{error}"))
}

/// **容量超限**那一段：超了多少、按体积排序的裁剪建议。
///
/// 只求选择集与排完差量预览两处摆的是同一段——超出量与建议本来就由核心一处算
/// （`sublibrary::over_capacity` / `trim_suggestions`），画法也就只该写一处：
/// 各画一遍的话，改了一处的措辞另一处就跟着说另一套话。
///
/// **这一段按不动**（ADR-0016 的「砍谁由人定」在浏览屏上做）：砍一个的落点是一条
/// **排除例外**，而例外的加减这一票整个搬去了浏览屏（票 `gui-redesign/11`）。
/// 在这儿留一个「排除」按钮，等于把刚拆开的那两件事又缝回去。
fn trim_ui(ui: &mut egui::Ui, over: u64, capacity: Option<u64>, trims: &[Trim]) {
    ui.colored_label(
        ui.visuals().error_fg_color,
        format!(
            "超出容量上限 {}（上限 {}）。**不会自动截断**——砍谁由你定：\
             按上面「改选择」跳去浏览屏，在详情面板里把它排除掉。",
            human_bytes(over),
            capacity.map_or_else(|| "—".to_string(), human_bytes),
        ),
    );
    ui.label("按体积排序的裁剪建议：");
    for trim in trims {
        ui.label(format!(
            "{}  砍到这条为止腾出 {}  {}",
            human_bytes(trim.bytes),
            human_bytes(trim.cumulative),
            trim.variant,
        ));
    }
    // **「砍到第几个才够」要一眼看得出来**：超出量动辄几十上百 GiB，让人自己把十行
    // 数字加一遍是白让他算。
    if let Some(last) = trims.last()
        && last.cumulative < over
    {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!("这几个全砍掉还差 {}。", human_bytes(over - last.cumulative)),
        );
    }
}
