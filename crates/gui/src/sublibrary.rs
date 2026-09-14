//! **子库屏**：管住这几台设备。**不增删规则**——改选择跳回浏览屏；只有超限时的删减建议上
//! 按得出一条排除例外。
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
//! 2. **配目标**——「目标设置」那一层弹层（屏头「新建子库」、卡上「目标设置…」打开）：名字、目标路径、
//!    前端格式、容量上限、能力档案。
//! 3. **排差量后同步**——卡片下半截。
//!
//! **选择集在这一屏上只读**——只留一处例外：超限时删减建议表上的「排除」（见下面「容量条三段」）。
//! 规则怎么改、例外怎么增减，全在浏览屏上做：卡上点
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
//! 「同步」按钮跟着灰掉。**搬上任务台之后这条一个字都没松**——台上排着的那一趟认的仍是
//! 排它时那份计划，见底下「三条长活全走任务台」。
//!
//! ## 差量步骤一步都不截
//!
//! 摊开之后那张表（[`steps_table`]）**列得全**：真机量级上一次同步动上万个文件（实测
//! 合成数据 **21,571 步**，`docs/library-facts.md`），而 ADR-0016 那句「同步前必须看一遍
//! 它要做什么」不该有一半落在界面之外。不截的代价是零——计划整份本来就在 [`Screen::prepared`] 里，而
//! `TableBody::rows` 只画视口里那几十行；[`Screen::steps_drawn`] 把「这一帧真的画了
//! 几行」数出来，于是这句话是被数出来的（挂账 `D158`）。
//!
//! ## 容量条三段：选中的、清单之外的、上限
//!
//! 三个数各有各的出处，而且**出处不同这件事要说得出口**（[`Gauge`]）：**选中**只问
//! 中立库，卡不在手边也算得出来；**清单之外**要目标设备在位，没看过目标时它是
//! 「还不知道」而不是零。画成同一个零的话，人会以为卡上是空的。
//!
//! 超没超由核心一处算——同步计划器，底是目标现占 ＋ 净变化；「算一遍容量」那份报告里的
//! 是从计划抄来的（`sublibrary::Fit`，挂账 D76）。条子自己不算第二遍。
//! **容量超限只给建议，绝不自动截断**（ADR-0016）——删减建议表上按「排除」，落的就是这个子库的
//! 一条**排除例外**（与浏览屏详情面板里记的是同一种，票 `gui-looks-like-the-design/20`），
//! 盘上的文件一个都不动。
//!
//! ## 领域判断一条都不在这里
//!
//! 规则怎么读（`Rule::parse`）、选择集怎么求值（`sublibrary::select`）、三方对比怎么排
//! （`sync::plan`）、目标落在主库里要不要拦（`sync::prepare::refuse_target_in_library`）
//! ——全在核心。这一层只做三件事：把要来的画出来、把点的那一下写回去、把中文输入放在
//! 对的位置上。
//!
//! ## 三条长活全走任务台
//!
//! 这一屏上会跑一会儿的有三条：**排差量预览**（真机量级 343 毫秒）、**算一遍容量**
//! （同一趟全库事实，343 毫秒）、**同步**（几十 GiB、可能几十分钟）。三条都跑在画帧
//! 那条线程之外，而且都走同一张[任务台](crate::task)——点那三个按钮等于各往台上排一趟活，
//! 跑完了台上按号把产物交回来（[`Screen::settle`]）。于是规格里那句「扫描、识别、刮削、
//! 同步统一排队，一处看得见」在这一屏上是真的：名字、进度、已用时间、按得停、跑完那条
//! 带耗时的历史，五样都在任务屏上，而这一屏在按钮旁边摆的是**同一份**快照。
//!
//! **台上那一趟认的是排它时那份计划。** 按下同步的那一刻，那份 [`Prepared`] 就整份交给
//! 了台上那趟活（闭包自己拿着一份，不是屏上这一份的借用）。此后规则怎么改、屏上那份
//! 预览怎么作废，都动不了已经排出去的那一趟——ADR-0016 那句「同步前必须预览差量」
//! 因此照旧是**构造上的事实**：跑的正是人点头时看过的那一份。反过来也说得通：改过规则
//! 之后屏上那份预览当场作废，想把改完的那一批传上去就得**重排一次差量**。
//!
//! **要写中立库的那一步在认领里做，不在台上。** 台上那条线拿的是同一个库文件的
//! **只读**连接，写不动；而一趟同步跑完——**包括被按停的那一趟**——必须把**清单**落回
//! 库里：那份清单记的是「到中断为止目标上真实有什么」，下一趟才接得上。所以它是当作
//! 产物交回来的（[`Product::Synced`]），落库发生在 [`Screen::settle`]。

use std::collections::BTreeMap;
use std::path::PathBuf;

use egui::{Align, Layout};
use romcat_core::capability::Roster;
use romcat_core::catalog::CatalogError;
use romcat_core::catalog::sublibrary::RemovedSublibrary;
use romcat_core::report::{decimal_bytes, human_bytes, thousands};
use romcat_core::site::Site;
use romcat_core::sublibrary::report::SelectionReport;
use romcat_core::sublibrary::{
    BrokenRule, Exception, ExceptionRow, Fit, Gauge, LoadedSelection, Room, Rule, StoredRule,
    Sublibrary, rule,
};
use romcat_core::sync::{self, Act, Outcome, Prepared};
use romcat_core::task::{Cutoff, Ending, Finished, Handle};

use crate::dialog::{Button, Dialog, Footer, Width};
use crate::look::step;
use crate::table::ROW_HEIGHT;
use crate::task::{Product, Tasks};
use crate::toast::{self, Toast};
use crate::tokens::Tokens;
use crate::{font, look};

/// 没摊开时卡上先摆几条步骤。**摆得出样子就够**：这几条回答的是「它大概要干什么」，
/// 「一共几步」那个数写在旁边，「到底哪几步」按「全部展开」。
const STEP_SAMPLE: usize = 6;

/// 意外与放不下的那几类，最多各列几条。
const TOP_NOTES: usize = 20;

/// 容量条上「清单之外：还不知道」那一段画多长，占整条的几成。
///
/// 取的是设计稿 `devCard` 里那个数（没看过目标时那一段 `min(12%, 余下的)`）：它不是一个量出来的
/// 容量——还不知道就没有数可画——只是让「这儿有一段不知道的」看得见，而不是缩成零。
const UNKNOWN_SHARE: f32 = 0.12;

/// 还没排过差量预览时卡上那条提示框（设计稿 `diffHTML` 的 `.note`）。
const PREVIEW_NOTE: &str = "同步前需要先生成差量预览，确认将要进行的更改。\
     生成预览只读取数据，不会写入任何文件。选择集修改后，已有的预览会失效。";

/// 卡不在位时容量图例底下那一行普通小字（设计稿 `devCard`；逐字照稿、样式也照稿，拿主意的人定）。
const ABSENT_NOTE: &str = "设备未连接时仍可计算已选容量；\
     清单外文件需要连接设备并生成差量预览后才能统计，未知不代表为零。";

/// 卡片底下（没有子库时是空态卡底下）那一行帮助字（设计稿子库屏的 `.help`，逐字照稿）。
const TRIM_HELP: &str = "空间不足时只给出删减建议，不会自动删除任何内容。\
     删减建议里的「排除」会记为这个子库的手动例外。";

/// 「**目标设置**」那层弹层开着时，开的是哪一种。
#[derive(Debug, Clone, PartialEq, Eq)]
enum TargetDialog {
    /// 屏头「新建子库」：一份空草稿。
    New,
    /// 卡上「目标设置…」：这一台的草稿。
    Of(String),
}

/// 新建或改一个子库时界面上那份草稿。
///
/// **每一格都会碰到输入法**（目标路径里有中文目录名是常态），所以这几个控件摆在「目标设置」那层弹层的
/// 内容区里：内容区每帧把整份内容都摆一遍，正在组字的那一格不会凭空消失（[`crate::dialog`]、ADR-0005）。
#[derive(Debug, Clone, Default)]
pub struct Form {
    /// 子库叫什么。一台目标设备一个。
    pub name: String,
    /// 目标设备上的子库根：读卡器挂上来的那个盘上的目录。
    pub target: String,
    /// 前端格式（适配器名）。空着就是 Pegasus。
    pub format: String,
    /// 容量上限，如 `512GB`、`476GiB`。空着就是不设限。从现成的子库填进来时写一位小数的十进制（`511.1 GB`，
    /// 挂单 `Q856`）。
    pub capacity: String,
    /// **能力档案**的名字。空着就是「不作声称」——不转换、不检查。
    pub capability: String,
    /// 从现成的子库填草稿时（[`Self::of`]），容量那一格**填进去的那串字与它原来的字节数**。
    ///
    /// 那一格写的是一位小数（`511.1 GB`），读回来是 511,100,000,000——人没碰那一格就按保存的话，上限会被
    /// 悄悄改掉。所以字没改过就沿用原来的字节数，不重新解析（拿主意的人 2026-09-14 定）。
    kept_capacity: Option<(String, u64)>,
}

impl Form {
    /// 从一个现成的子库填一份草稿。
    #[must_use]
    pub fn of(sublibrary: &Sublibrary) -> Self {
        let kept_capacity = sublibrary
            .capacity
            .map(|bytes| (decimal_bytes(bytes), bytes));
        Self {
            name: sublibrary.name.clone(),
            target: sublibrary.target.clone(),
            format: sublibrary.format.clone(),
            capacity: kept_capacity
                .as_ref()
                .map(|(text, _)| text.clone())
                .unwrap_or_default(),
            capability: sublibrary.capability.clone().unwrap_or_default(),
            kept_capacity,
        }
    }
}

/// 「**改选择**」按下去之后要交给浏览屏的那一份东西。
///
/// 规则在这一屏上是**只读**的，改它的地方是浏览屏的筛选器。跳过去时带的正是这个：
/// 哪个子库、它的规则并成的那一条、以及读不懂的那几条原样。
#[derive(Debug, Clone)]
pub struct Jump {
    /// 改的是哪个子库。
    pub sublibrary: String,
    /// 这个子库的规则并成一条（多条之间是**任一满足**，与求值同一条口径）。
    ///
    /// 一条都没有时是 `None`——那是「没有任何条件」，不是「一条都选不中」。
    pub rule: Option<Rule>,
    /// 读不懂的那几条**原样带过去**（序号、原文、错在哪）。
    ///
    /// 它们没参与求值，「更新到子库」也不会碰它们；带过去是为了**在那一屏上扔得掉**
    /// （票 `gui-redesign/14`）。只带一个数的话，浏览屏摆得出「有 N 条」却指不出是哪几条。
    pub broken: Vec<BrokenRule>,
    /// 规则行上「✎」跳过去的：只改这个子库的**第几条**（`rule` 就是那一条），浏览屏「更新到子库」只换回这一条
    /// （票 `gui-looks-like-the-design/20`）。`None` 是「从浏览添加…」那种整批改。
    pub ordinal: Option<i64>,
}

/// 规则行尾那两颗图标按钮这一帧按了哪一颗、哪一条（序号）。
#[derive(Debug, Clone, Copy)]
enum RulePressed {
    /// 「✎」：去浏览屏只改这一条。
    Edit(i64),
    /// 「×」：移除这一条（先问一层）。
    Remove(i64),
}

/// 一台设备卡头上的**能力档案**与**文件系统**：名册里解出来的那一份的名字。
#[derive(Debug, Clone)]
struct Profiled {
    /// 真会用上的那份档案叫什么。
    profile: String,
    /// 那份档案声明的文件系统叫什么。
    filesystem: String,
    /// 子库记着一份档案的名字、名册里却查不到（[`Roster::find`]）时，记着的那个名字。
    missing: Option<String>,
}

/// 一台设备**存在中立库里的那份选择集**：规则原文（连序号）、读不懂的那几条、例外。
/// 卡上的规则列表照它摆。
#[derive(Debug, Clone, Default)]
struct StoredSelection {
    /// 读得懂的规则原文，连库里的序号。
    rules: Vec<StoredRule>,
    /// 读得懂的每一条规则的短名（[`Rule::label`]，设计稿 `autoName`），按序号。读回来时拼一次，不在画帧里拼。
    labels: BTreeMap<i64, String>,
    /// 读不懂的那几条。
    broken: Vec<BrokenRule>,
    /// 例外。
    exceptions: Vec<ExceptionRow>,
}

/// 删掉一台之后留着的那一份撤销：核心交回来的整份子库，与底边那条提示条。
#[derive(Debug)]
struct Undo {
    /// 删之前整份留下来的那一份（[`Catalog::take_sublibrary`](romcat_core::catalog::Catalog::take_sublibrary)）。
    removed: RemovedSublibrary,
    /// 「已删除子库「…」，设备上的文件没有改动」，带一颗「撤销」。
    toast: Toast,
}

/// 子库这个屏幕。
pub struct Screen {
    /// 工作目录：中立库、**媒体池**、能力档案名册都在这儿。
    workspace: PathBuf,
    /// 库里现有的子库。**一台设备一张卡，就是这一串。**
    list: Vec<Sublibrary>,
    /// 每台设备卡头上那两格：**能力档案**与它的**文件系统**，照核心的名册解出来
    /// （[`Roster::find_or_unclaimed`]，与排差量那一趟用的是同一处）。随 [`Self::reload`] 重读，
    /// 不在画帧里读盘。
    profiled: BTreeMap<String, Profiled>,
    /// 能力档案名册读不动时那句话。**那时卡头不编一份档案出来。**
    roster_error: Option<String>,
    /// 上一回看的时候，**目标不在位**的那几台（[`target_absent`] 那一个口径）。卡头那枚标签与「装不装得下
    /// 算不出」那一行照它说话。**不在画帧里查盘**：随 [`Self::reload`] 与任务台交回一趟活时各查一遍
    /// （[`Self::look_at_targets`]），于是插上卡之后要等下一回才换过来（挂单 `Q855`）。
    absent: std::collections::BTreeSet<String>,
    /// 眼下摊开的是哪一张卡。
    picked: Option<String>,
    /// 编辑草稿：「目标设置」那层弹层里那几格。
    form: Form,
    /// 「目标设置」那层弹层开没开着、开的是哪一种（[`Self::begin_new`] / [`Self::edit_target`]）。
    target_dialog: Option<TargetDialog>,
    /// **每台设备**的选择集原文：规则（连库里的序号）、读不懂的那几条、例外。
    ///
    /// 每张卡都摆它自己的规则列表（票 `gui-looks-like-the-design/20`），所以一台不落全读回来
    /// （[`Self::reload`]），不只读摊开那一张。**一条坏的不该让另外五条一起用不了**：
    /// 读不懂的另放一栏。
    selections: BTreeMap<String, StoredSelection>,
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
    /// 刚摊开、还没把那张表滚进视口。
    ///
    /// 卡上差量表上面摆着卡头、规则、容量条与差量账，**摊开时那张表多半在视口底下**——
    /// 虚拟化的表只画视口里的行，于是按了「全部展开」却一行都看不见。摊开那一下把它滚到
    /// 视口顶上：看得见几行由视口多高决定，与上面那几段（按钮多高、规则几条）无关。
    reveal_steps: bool,
    /// 上一帧那张步骤表**真的画了几行**。
    ///
    /// 它是「翻行的代价与总步数无关」那句话的**量具**，与 [`crate::table::Window::reads`]
    /// 同一个路子：不掐表，数一帧真的做了多少事。挂钟在门禁上是一张彩票
    /// （票 `parking-3/01` 拿掉过一条挂钟断言），而这个数机器忙不忙一个字都不影响。
    steps_drawn: usize,
    /// 每台设备各求一次**选择集**的结果：选中哪些、多大、超限多少、砍谁。
    ///
    /// **一趟折一次事实，全部子库共用**——折事实是走一遍全库（343 ms，挂账 D156），
    /// 而按选择集求值是内存里的事。一台一折的话，五张卡就是五趟全库。
    /// 整趟活在核心里（`sublibrary::survey`），于是屏上摆着的与 `romcat sublibrary show`
    /// 印出来的是同一个值。
    ///
    /// **与差量预览分开**：「这套规则选出多少、多大」只要中立库，子库是持久实体，不是
    /// 「插上卡才存在的东西」——卡不在手边时照样该看得见。**装不装得下**两边走的是同一条
    /// 排计划的线（目标现占 ＋ 净变化，挂账 D76），只是这里一趟问全部设备、不留可同步的
    /// 计划；卡不在手边的那台如实说算不出。
    evaluated: BTreeMap<String, SelectionReport>,
    /// 正在算的那一趟容量是任务台上的第几号。**它同时是认领凭据**
    /// （与 [`Self::previewing`] 一个写法）。
    evaluating: Option<u64>,
    /// 计划里有删除时，要先勾这一格才动得了手。
    acknowledged: bool,
    /// 「改选择」按下去了，等窗口把它送去浏览屏（[`crate::app::App::route`]）。
    jump: Option<Jump>,
    /// 「删除子库」那层确认弹层开着时，删的是哪一台（[`Self::ask_remove`]）。
    ///
    /// **删的是选择集，不是文件**：规则、例外、清单跟着那条定义一起没。例外是**永久记住**的手挑决定
    /// （ADR-0016），规则也不在这一屏上重打得回来——所以按下去先问一层，问的时候把名字与目标路径写明。
    delete_dialog: Option<String>,
    /// 规则行上「×」那层确认弹层开着时，移除的是哪一台的第几条规则（[`Self::ask_remove_rule`]）。
    rule_dialog: Option<(String, i64)>,
    /// 刚删掉的那一台**整份留着**，提示条上那颗「撤销」按下去原样放回去（[`Self::undo_remove`]）。
    ///
    /// 提示条收起来、或者换到别的屏（[`Self::leave`]）就丢掉：那之后删除就是真的删了。
    undo: Option<Undo>,
    /// 每台设备的**清单**记着几条（[`Self::read_manifests`]）：卡头那枚「已同步 · 清单 N 条」照它写。
    manifest_rows: BTreeMap<String, usize>,
    /// 排上任务台的那一趟同步是第几号。
    ///
    /// **它同时是认领凭据**：跑完的那一趟按号对得上才收（[`Screen::settle`]）。
    /// **但它不是「那一趟认哪份计划」的凭据**——计划在排它那一刻就整份交给了台上那趟活，
    /// 屏上这一份此后怎么变都影响不到它（模块文档「台上那一趟认的是排它时那份计划」）。
    syncing: Option<u64>,
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
            profiled: BTreeMap::new(),
            roster_error: None,
            absent: std::collections::BTreeSet::new(),
            picked: None,
            form: Form::default(),
            target_dialog: None,
            selections: BTreeMap::new(),
            prepared: None,
            previewing: None,
            prepare_ms: 0.0,
            expanded: false,
            reveal_steps: false,
            steps_drawn: 0,
            evaluated: BTreeMap::new(),
            evaluating: None,
            acknowledged: false,
            jump: None,
            delete_dialog: None,
            rule_dialog: None,
            undo: None,
            manifest_rows: BTreeMap::new(),
            syncing: None,
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
        // **档案名解到哪一份由核心说**：记着的名字在名册里没有时退回「不作声称」
        // ——卡头写的是真会用上的那一份，不是记着的那个名字。
        match Roster::in_workspace(&self.workspace) {
            Ok(roster) => {
                self.profiled = self
                    .list
                    .iter()
                    .map(|sublibrary| {
                        let recorded = sublibrary.capability.as_deref();
                        let profile = roster.find_or_unclaimed(recorded);
                        (
                            sublibrary.name.clone(),
                            Profiled {
                                profile: profile.name,
                                filesystem: profile.filesystem.name,
                                missing: recorded
                                    .filter(|name| roster.find(name).is_none())
                                    .map(ToString::to_string),
                            },
                        )
                    })
                    .collect();
                self.roster_error = None;
            }
            Err(error) => {
                self.profiled.clear();
                self.roster_error = Some(format!("能力档案名册读不动：{error}"));
            }
        }
        self.look_at_targets();
        self.read_manifests(site);
        self.selections.clear();
        // **摊开的那一张先由 `open` 读**（它会重设这一屏那句错），别的卡随后读：
        // 次序反过来的话，别的卡读不动的那句话会被 `open` 抹掉。
        match self.picked.clone() {
            Some(name) if self.list.iter().any(|row| row.name == name) => self.open(site, &name),
            Some(_) => self.picked = None,
            None => {}
        }
        let others: Vec<String> = self
            .list
            .iter()
            .map(|row| row.name.clone())
            .filter(|name| self.picked.as_deref() != Some(name.as_str()))
            .collect();
        for name in &others {
            if let Err(why) = self.read_selection(site, name) {
                self.error = Some(why);
            }
        }
    }

    /// 查一眼每台设备的目标**此刻在不在位**，记进 [`Self::absent`]。
    ///
    /// 口径与排差量预览、同步按下去那一刻拦的是同一个（[`target_absent`]：查一眼那个目录有没有），
    /// 查的是子库自己那条读盘路径（`Sublibrary::read_path`，ADR-0020）。
    fn look_at_targets(&mut self) {
        self.absent = self
            .list
            .iter()
            .filter(|sublibrary| target_absent(&sublibrary.read_path()).is_some())
            .map(|sublibrary| sublibrary.name.clone())
            .collect();
    }

    /// 读一遍每台设备的**清单**记着几条，记进 [`Self::manifest_rows`]。
    ///
    /// 走核心现成的读清单入口（`Catalog::manifest`）。**读不动的、还是空清单的那一台不记**：卡头那枚标签
    /// 退回「已经对齐」，不编一个数。**不在画帧里读库**：随 [`Self::reload`] 与任务台交回一趟活时各读一遍。
    fn read_manifests(&mut self, site: &Site) {
        self.manifest_rows = self
            .list
            .iter()
            .filter_map(|sublibrary| {
                let manifest = site.catalog.manifest(&sublibrary.name).ok()?;
                (!manifest.files.is_empty())
                    .then(|| (sublibrary.name.clone(), manifest.files.len()))
            })
            .collect();
    }

    /// **换到别的屏了**：删掉一台之后留着的那一份撤销跟着丢掉——从这一刻起删除就是真的删了（拿主意的人
    /// 2026-09-14 定）。窗口在画别的屏的每一帧里调它（[`crate::app::App::ui`]）。
    pub fn leave(&mut self) {
        self.undo = None;
    }

    /// 眼下还撤销得了的那一台叫什么（提示条还摆着）；没有就是 `None`。
    #[must_use]
    pub fn undo_pending(&self) -> Option<&str> {
        self.undo.as_ref().map(|undo| undo.removed.name())
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
        self.picked_selection().map_or(&[], |chosen| &chosen.rules)
    }

    /// 摊开那个子库读不懂的那几条规则。
    #[must_use]
    pub fn broken(&self) -> &[BrokenRule] {
        self.picked_selection().map_or(&[], |chosen| &chosen.broken)
    }

    /// 摊开那个子库的例外。
    #[must_use]
    pub fn exceptions(&self) -> &[ExceptionRow] {
        self.picked_selection()
            .map_or(&[], |chosen| &chosen.exceptions)
    }

    /// 摊开那一台的选择集原文。
    fn picked_selection(&self) -> Option<&StoredSelection> {
        self.picked
            .as_ref()
            .and_then(|name| self.selections.get(name))
    }

    /// 把一台设备的选择集原文从中立库读回来，替掉缓着的那一份。读不动时返回那句话，
    /// 缓着的那一份照旧丢掉——**不留一份说不清是哪一刻的旧规则**摆在卡上。
    fn read_selection(&mut self, site: &Site, name: &str) -> Result<(), String> {
        self.selections.remove(name);
        let loaded = site
            .catalog
            .selection(name)
            .map_err(|error| format!("中立库读不动：{error}"))?;
        let stored = site
            .catalog
            .sublibrary_rules(name)
            .map_err(|error| format!("中立库读不动：{error}"))?;
        // **读得懂的与读不懂的分两栏摆**：合在一起的话，读不懂那几条会被印两遍
        // ——一遍在规则里当正常的，一遍在下面当坏的。
        let broken: std::collections::BTreeSet<i64> =
            loaded.broken.iter().map(|row| row.ordinal).collect();
        let labels = loaded
            .selection
            .rules
            .iter()
            .zip(&loaded.ordinals)
            .map(|(rule, ordinal)| (*ordinal, rule.label()))
            .collect();
        self.selections.insert(
            name.to_string(),
            StoredSelection {
                rules: stored
                    .into_iter()
                    .filter(|row| !broken.contains(&row.ordinal))
                    .collect(),
                labels,
                broken: loaded.broken,
                exceptions: loaded.selection.exceptions,
            },
        );
        Ok(())
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
    /// 收起来的时候卡上只摆头几条（`STEP_SAMPLE`）——几百上千步铺满一屏，
    /// 会把它下面的「同步」挤到看不见的地方。摊开之后那张表是**虚拟化**的，
    /// **一步都不截**（[`steps_table`]）。
    pub fn expand(&mut self, on: bool) {
        self.expanded = on;
        self.reveal_steps = on;
    }

    /// 上一帧那张步骤表真的画了几行。见这个字段的文档：它是那句「翻行的代价与总步数
    /// 无关」的量具。摊开着才有值，没摊开是 0。
    #[must_use]
    pub fn steps_drawn(&self) -> usize {
        self.steps_drawn
    }

    /// 正在排的那一趟差量预览是任务台上的第几号；没排着就是 `None`。
    #[must_use]
    pub fn previewing(&self) -> Option<u64> {
        self.previewing
    }

    /// 某一台设备那份求过的**选择集报告**：选出多少、多大、超限多少、砍谁。
    #[must_use]
    pub fn evaluated(&self, name: &str) -> Option<&SelectionReport> {
        self.evaluated.get(name)
    }

    /// 正在算的那一趟容量是任务台上的第几号。
    #[must_use]
    pub fn evaluating(&self) -> Option<u64> {
        self.evaluating
    }

    /// 排上任务台的那一趟同步是第几号。
    #[must_use]
    pub fn syncing(&self) -> Option<u64> {
        self.syncing
    }

    /// 某一台设备卡上那根**容量条**：选中的、清单之外的、上限。
    ///
    /// 三个数从哪儿来，[`Gauge`] 的文档写着。**清单之外要看过目标才知道**：排过差量预览的
    /// 那一台、或者算过容量时卡在手边的那几台，条子才有第二段。
    ///
    /// ## 条子与旁边那行「超出容量上限」必须是同一笔账
    ///
    /// 超没超由核心一处算（ADR-0016），这一屏两处摆它：排过差量的照
    /// [`Plan::over_capacity`]，没排过的照报告里从计划抄来的那一份（[`Fit`]）——同一个底。
    /// 于是条子的总量得**照同一条口径**填，不然会出现「条子画到九成、旁边说超了
    /// 200 MiB」——那正是 [`Gauge`] 的文档说不该发生的事。
    ///
    /// - 排过差量、或者算过容量时卡在手边：`选中 = after_bytes − stranger_bytes`，于是
    ///   `taken()` 正好是 `after_bytes`——计划算超出量用的就是它，报告里那份是从计划抄的。
    ///   **它不等于「期望总量」**：卡上还留着那些「对不上、因此本次不动」的文件，
    ///   它们照样占地方（挂账 D76）。
    /// - 没看过目标：`选中 = report.bytes`、清单之外是 `None`，而旁边**不给超出量**
    ///   （算不出）——条子只画选中那一段与上限。
    ///
    /// [`Plan::over_capacity`]: romcat_core::sync::Plan::over_capacity
    /// [`Fit`]: romcat_core::sublibrary::Fit
    #[must_use]
    pub fn gauge(&self, name: &str) -> Gauge {
        let room = self.room_of(name);
        Gauge {
            picked: room.as_ref().map_or_else(
                || self.evaluated.get(name).map_or(0, |report| report.bytes),
                |room| room.after_bytes.saturating_sub(room.stranger_bytes),
            ),
            strangers: room.as_ref().map(|room| room.stranger_bytes),
            capacity: self
                .list
                .iter()
                .find(|row| row.name == name)
                .and_then(|row| row.capacity),
        }
    }

    /// 这一台卡上那笔**装得下吗**的账：排过差量的照那份计划，没排过的照「算一遍容量」
    /// 报告里从计划抄来的那份——两份同底（目标现占 ＋ 净变化，挂账 D76）。
    /// 卡不在手边、也没排过差量时是 `None`：**不是「装得下」，是算不出**。
    fn room_of(&self, name: &str) -> Option<romcat_core::sublibrary::Room> {
        self.plan_of(name)
            .map(romcat_core::sublibrary::Room::of)
            .or_else(|| {
                self.evaluated
                    .get(name)
                    .and_then(|report| report.fit.known())
                    .cloned()
            })
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

    /// 「目标设置」那层弹层里那份草稿，供实测与测试填。
    pub fn form_mut(&mut self) -> &mut Form {
        &mut self.form
    }

    /// 「目标设置」那层弹层开着没有。
    #[must_use]
    pub fn target_settings_open(&self) -> bool {
        self.target_dialog.is_some()
    }

    /// 摊开一张卡：读它的规则与例外。**排出来的那份预览当场作废**——
    /// 换了子库还留着上一个的差量，是这一屏最容易骗到人的一种写法。
    pub fn open(&mut self, site: &Site, name: &str) {
        self.picked = Some(name.to_string());
        self.invalidate();
        if let Some(sublibrary) = self.list.iter().find(|row| row.name == name) {
            self.form = Form::of(sublibrary);
        }
        self.error = self.read_selection(site, name).err();
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
        if let Some(说一句) = self.drop_survey() {
            self.notice = Some(说一句.to_string());
        }
        // 摊开的正是它的话，规则与例外也要重读一遍——卡上那几行印的就是它们。
        // `open` 自己会把差量预览作废（那份差量只可能是摊开这一台的）。
        // 没摊开的那一张也摆着自己的规则列表，同样要重读。
        if self.picked.as_deref() == Some(name) {
            self.open(site, name);
        } else if let Err(why) = self.read_selection(site, name) {
            self.error = Some(why);
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
        self.reveal_steps = false;
        self.acknowledged = false;
        self.outcome = None;
    }

    /// **台上那趟还没认领的「算一遍容量」一并不认了**，返回要对人说的那半句话。
    ///
    /// 三处要走它：改过某一台的选择集（[`Self::forget`]）、存过一个子库
    /// （[`Self::save`]）、删掉一个子库（[`Self::remove`]）。理由是同一条：**那一趟折
    /// 报告用的是排它时那份子库与那时候的规则**——上限、目标、能力档案全在里头。改完之后
    /// 把它收回来，卡上就会摆出「上限写着 64 GB、旁边说超了 200 GiB」，那正是 [`Gauge`]
    /// 的文档说不该发生的事。删掉一台之后收回来更糟：认领是**整份替换**，
    /// 刚删掉那一台的报告会又长回来。
    ///
    /// **丢得不吭声是不行的**：人按过那个按钮、可能等了几分钟（真库上它排在扫描后面），
    /// 屏上却一个数都没长出来，那看着就像按钮坏了。
    ///
    /// 被弃认的那趟活自己会跑完——它整条只读，跑完也没有副作用，只是没人认领它。
    /// **弃认的是整趟而不是那一台**：产物是一份「全部设备」的表，按台拆开认领要多立一种
    /// 合并口径（「这一台按新的、那几台按旧的」），而那份账本来就是一趟折出来的
    /// ——多算一遍全库比多等一趟诚实。
    fn drop_survey(&mut self) -> Option<&'static str> {
        self.evaluating
            .take()
            .map(|_| "正在算的那一趟容量不认了——它算的是改之前那一套；再按一次「算一遍容量」。")
    }

    /// **每台设备各求一次选择集**：这套规则加例外选出什么、多大、装不装得下。
    /// 往[任务台](crate::task)上排一趟，跑在画帧那条线程之外。
    ///
    /// 选中多少只问中立库；**装不装得下对着目标排一遍计划**（与差量预览同一条线，挂账 D76），
    /// 卡不在手边的那台如实说算不出、不给数。折事实那一趟走一遍
    /// 全库，**全部子库共用它**：一台一折的话，五张卡就是五趟全库。整趟活整份交给核心
    /// （[`survey`](romcat_core::sublibrary::survey)），于是界面上摆着的与
    /// `romcat sublibrary show` 印出来的是同一个值。容量超限时**只给建议，一个都不砍**
    /// （ADR-0016）。
    ///
    /// **它原先跑在画帧那条线程上**：真机量级上按一下窗口僵 343 毫秒，期间连「停下」
    /// 都没有（挂单 Q87、挂账 D156）。搬上任务台之后进度看得见、停得动，而它整条只读，
    /// 所以停在哪儿都是干净的——一个字节都没写，再算一次就是。
    ///
    /// 后台那条线程读的是同一个库文件的**第二份只读连接**，与 [`Self::preview`] 同一条路
    /// （分不出来的那种库就地跑完，理由见那一处）。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn evaluate(&mut self, site: &Site, tasks: &mut Tasks) {
        if self.evaluating.is_some() {
            return;
        }
        self.error = None;
        // **算的是排它这一刻库里摆着的那几台设备**，这一份名单跟着那趟活走。
        let list = self.list.clone();
        let workspace = self.workspace.clone();
        let title = "算一遍容量".to_string();
        self.evaluating = Some(match site.catalog.read_only() {
            Ok(reader) => tasks.queue(title, move |task| {
                romcat_core::sublibrary::survey(&reader, &workspace, &list, task)
                    .map(|reports| Product::Evaluated(Box::new(reports)))
            }),
            Err(CatalogError::NotOnDisk { .. }) => tasks.run_here(title, |task| {
                romcat_core::sublibrary::survey(&site.catalog, &workspace, &list, task)
                    .map(|reports| Product::Evaluated(Box::new(reports)))
            }),
            Err(why) => {
                self.error = Some(no_second_connection("算一遍容量", &why));
                return;
            }
        });
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
        // **卡不在位是按下去之前就判得出的**（票 `gui-looks-like-the-design/07`）：不排，只在屏上
        // 说为什么不行、去哪儿办。排上去的话那一趟在「看一眼目标」那一下撞上、记一条失败——任务
        // 历史里就多一条压根没开跑的「失败」。查的是子库自己那条读盘路径（`Sublibrary::read_path`，
        // ADR-0020），与那一趟看的是同一个目录。
        //
        // **记着的前端格式这一版没有适配器，不在这里拦**：找不到时该说哪句话是核心库的事，而核心库
        // 对子库没有对外的那一问（`ExportSetup::adapter` 的文档：界面那一层不自己去
        // `adapter::find`）。它照旧排上去，在那一趟里记失败（挂单 `Q797`）。
        if let Some(why) = self
            .list
            .iter()
            .find(|sublibrary| sublibrary.name == name)
            .and_then(|sublibrary| target_absent(&sublibrary.read_path()))
        {
            self.error = Some(why);
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
                self.error = Some(no_second_connection("排差量预览", &why));
                return;
            }
        });
    }

    /// 任务台交回来一趟跑完的活。**不是自己那一趟就放过去。**
    ///
    /// 这一屏在台上排三种活——排差量预览、算一遍容量、同步——各按自己那个号认领。
    /// **同步那一支要写中立库**（把**清单**落回去），所以这一层收的是可写的那份现场：
    /// 台上那条线拿的是只读连接，写不动。
    pub fn settle(&mut self, site: &mut Site, done: Finished<Product>) {
        if self.previewing == Some(done.id) {
            self.previewing = None;
            self.settle_preview(done);
        } else if self.evaluating == Some(done.id) {
            self.evaluating = None;
            self.settle_evaluate(done);
        } else if self.syncing == Some(done.id) {
            self.syncing = None;
            self.settle_sync(site, done);
        }
        // 那一趟刚看过目标（或者往上写过）：卡头说的「在不在位」与清单记着几条跟着换过来。
        self.look_at_targets();
        self.read_manifests(site);
    }

    /// 排差量预览那一趟回来了。
    fn settle_preview(&mut self, done: Finished<Product>) {
        match done.ended {
            Ending::Done(Product::Preview(prepared))
            | Ending::Halfway {
                product: Product::Preview(prepared),
                ..
            } => {
                // **只有真排出来那一趟才记耗时。** 被撤掉、出错的那趟什么都没排出来，
                // 摆一个「排它用了 120 ms」在旁边等于给一份不存在的差量记账。
                // （**停在半路**那一档它到不了：排差量整条只读，停下来什么都不留下。
                // 并进这一支只为把那条轴配全。）
                self.prepare_ms = done.elapsed.as_secs_f64() * 1000.0;
                self.prepared = Some(*prepared);
                self.error = None;
            }
            // 别的屏排上去的活轮不到这儿——`previewing` 那道判断已经挡掉了，
            // 这一支只为把 `Product` 那个枚举配全。
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **停下来的地方是干净的，就得这么说。** 说成「失败」会让人去找哪儿坏了。
            Ending::Stopped => {
                self.notice = Some(format!(
                    "排差量预览{}。这一趟整条只读——中立库、媒体池、目标设备\
                     一个字节都没动，再排一次就是。",
                    Ending::<()>::Stopped.render(),
                ));
                self.failed = false;
            }
            // **不静默结束**：哪一步、为什么，两样都说出来。
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "排差量预览{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// 算一遍容量那一趟回来了。
    fn settle_evaluate(&mut self, done: Finished<Product>) {
        match done.ended {
            Ending::Done(Product::Evaluated(reports))
            | Ending::Halfway {
                product: Product::Evaluated(reports),
                ..
            } => {
                self.evaluated = *reports;
                self.error = None;
            }
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **停下来的地方是干净的，就得这么说。** 上一趟算出来的那几个数照旧摆着
            // ——它们没有因为这一趟被停而变得不对。
            Ending::Stopped => {
                self.notice = Some(format!(
                    "算容量{}。这一趟整条只读——中立库、主库、目标设备一个字节都没动，\
                     再算一次就是。",
                    Ending::<()>::Stopped.render(),
                ));
                self.failed = false;
            }
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "算一遍容量{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// 同步那一趟回来了：**清单落回中立库**，然后把这一趟的账摆出来。
    ///
    /// **清单要在这一步写**——台上那条线拿的是只读连接，写不动（[`Product::Synced`]
    /// 的文档）。**被按停的那一趟也要落清单**：那份清单记的是「到中断为止目标上真实有
    /// 什么」，下一趟才接得上。
    fn settle_sync(&mut self, site: &mut Site, done: Finished<Product>) {
        let elapsed = done.elapsed.as_secs_f64();
        match done.ended {
            // **跑完的那一趟与「停在半路」那一趟走同一条**：后者交出来的清单同样是真的
            // ——它记着「到中断为止目标上真实有什么」，不落库下一趟就接不上（ADR-0015）。
            // 回执里那句「⚠️ 这一趟被你按停了」由 `sync_notice` 照 `interrupted` 印，
            // 与任务屏历史那一行说的是同一件事。
            Ending::Done(Product::Synced(outcome))
            | Ending::Halfway {
                product: Product::Synced(outcome),
                ..
            } => {
                if let Err(error) = site
                    .catalog
                    .put_manifest(&outcome.sublibrary, &outcome.manifest)
                {
                    self.error = Some(format!(
                        "⚠️ 清单写不回中立库：{error}\n\
                         目标上的文件已经动过了，而清单还是旧的那一份——下一趟同步会把这次\n\
                         放上去的东西当成「清单之外」，于是碰都不敢碰。先修好中立库再跑一次。"
                    ));
                }
                self.notice = Some(sync_notice(&outcome, elapsed));
                self.failed =
                    outcome.interrupted || outcome.gave_up || !outcome.failures.is_empty();
                // 传完之后那份预览说的已经是过去时了：目标现在是另一个样子。
                //
                // **只清掉这一台的那一份。** 台上排着队可能排上几十分钟，认领回来的时候
                // 屏上摊开的多半已经是另一台、摆着的是那一台刚排好的差量——那一份还没传呢，
                // 一并清掉等于让人白排一趟。
                if self
                    .prepared
                    .as_ref()
                    .is_some_and(|prepared| prepared.sublibrary.name == outcome.sublibrary)
                {
                    self.prepared = None;
                    self.acknowledged = false;
                }
                self.outcome = Some(*outcome);
            }
            Ending::Done(_) | Ending::Halfway { .. } => {}
            // **走到这儿的只有「还排着队就被撤掉」那一种**：真跑起来的那一趟被按停时
            // 照旧交出产物（[`Product::Synced`] 的文档），走的是上面那一支
            // ——那是「停了，留下了产物」，这一支是「停了，什么都没留下」。
            // 于是这一句敢说「一个字节都没动」。
            Ending::Stopped => {
                self.notice = Some(
                    "同步还没轮到就被撤掉了。目标设备上一个字节都没动，那份差量还摆着，\
                     再按一次同步就是。"
                        .to_string(),
                );
                self.failed = false;
            }
            Ending::Failed { step, why } => {
                self.error = Some(format!(
                    "同步{}",
                    Ending::<()>::Failed { step, why }.render()
                ));
            }
        }
    }

    /// **删减建议表上按「排除」**：把这个变体记成这台设备的一条**排除例外**，然后重算一遍容量。
    ///
    /// ADR-0016：超限只给建议，砍谁由人定。落的就是浏览屏详情面板里那种例外
    /// （`Catalog::set_exception`），不是第二套机制；**盘上的文件一个都不动**——主库只读
    /// （ADR-0004），卡上的文件等下一趟差量预览与同步照清单去对。
    ///
    /// 记完之后这一台缓着的差量与容量账全部作废（[`Self::forget`]），并且**当场重排一趟
    /// 「算一遍容量」**：人对着建议表一项项往下排除，下一项还要不要排由核心重新说，屏上不留
    /// 一份排除之前的账。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn exclude(&mut self, site: &mut Site, tasks: &mut Tasks, name: &str, key: &str) {
        match site
            .catalog
            .set_exception(name, key, Exception::Exclude, None)
        {
            Ok(()) => {
                // 台上那趟还没认领的「算一遍容量」算的是排除之前那一套，`forget` 把它弃认；
                // 紧接着就重排一趟，所以它留下的那句「再按一次」换成下面这句。
                self.forget(site, name);
                self.notice = Some(format!(
                    "给「{name}」记下了一条排除例外：{key}。盘上的文件一个都没动；容量正在重算。"
                ));
                self.evaluate(site, tasks);
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 「**改选择**」：把这个子库的规则并成一条，交给窗口送去浏览屏。
    ///
    /// 这一下**什么都没写**——它只是把要改的东西装好（规则与例外的增减在浏览屏上做）。
    /// 真正的跳转由 [`crate::app::App::route`] 走：那儿才同时够得着两屏。
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_selection(&mut self) {
        let Some(name) = self.picked.clone() else {
            self.error = Some("先摊开一张卡。".to_string());
            return;
        };
        // 读得懂的并成一条，读不懂的只数一数：它们本来就没参与求值，这一趟也不碰。
        let chosen = self.selections.get(&name).cloned().unwrap_or_default();
        let loaded = LoadedSelection::from_stored(&chosen.rules);
        self.jump = Some(Jump {
            sublibrary: name,
            rule: Rule::any_of(loaded.selection.rules),
            broken: chosen.broken,
            ordinal: None,
        });
    }

    /// 规则行上「**✎**」：把这一台的**第 `ordinal` 条**规则交给窗口送去浏览屏，只预填这一条；调完按「更新到子库」只换回
    /// 这一条（拿主意的人 2026-09-14 定）。与 [`Self::edit_selection`] 一样这一下什么都没写。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_rule(&mut self, name: &str, ordinal: i64) {
        let chosen = self.selections.get(name).cloned().unwrap_or_default();
        let Some(rule) = chosen
            .rules
            .iter()
            .find(|stored| stored.ordinal == ordinal)
            .and_then(|stored| Rule::parse(&stored.text).ok())
        else {
            self.error = Some(format!(
                "子库「{name}」的第 {ordinal} 条规则不在了，或者读不懂。"
            ));
            return;
        };
        self.jump = Some(Jump {
            sublibrary: name.to_string(),
            rule: Some(rule),
            broken: chosen.broken,
            ordinal: Some(ordinal),
        });
    }

    /// 把「改选择」那一下取走。**取过就没了**：窗口一帧问一次。
    pub fn take_jump(&mut self) -> Option<Jump> {
        self.jump.take()
    }

    /// **同步**：把差量真正落到目标设备上，往[任务台](crate::task)上排一趟。
    ///
    /// 四道闸一道都不能少：**得先有预览**（ADR-0016）、**目标得在位**（票
    /// `gui-looks-like-the-design/07`）、**有删除就得先点头**（ADR-0015）、
    /// **目标不许落在主库里**（ADR-0004，判据在核心里）。四道都过在**排它之前**——
    /// 排上去之后没人再看第二眼。
    ///
    /// ## 台上那一趟认的是排它时那份计划
    ///
    /// 那份 [`Prepared`] 与主库那一组根都是**整份交给**台上那趟活的（闭包自己拿着一份，
    /// 不是屏上这一份的借用）。于是排上去之后规则怎么改、屏上那份预览怎么作废，跑的仍然
    /// 是人点头时看过的那一份——ADR-0016 那句「同步前必须预览差量」因此照旧是构造上的
    /// 事实。**要把改过规则之后的那一批传上去，就得重排一次差量预览再按一次。**
    ///
    /// 界面上按那个按钮走的就是它，实测与测试拿它当那一下。
    pub fn sync(&mut self, site: &Site, tasks: &mut Tasks) {
        if self.syncing.is_some() {
            return;
        }
        let Some(prepared) = self.prepared.clone() else {
            // 这句话不是提示，是这一屏的规矩：没预览就没有可传的东西。
            self.error = Some("还没排过差量预览。先看一遍它要做什么。".to_string());
            return;
        };
        // **排完差量之后卡被拔了，就不排**：同步那一趟起手就把目标根建出来，卡拔了之后那个路径
        // 指着的是本机的盘——一份子库会被悄悄写进本机一个新建的空目录里。这是按下去之前就判得出
        // 的，只在屏上说（票 `gui-looks-like-the-design/07`）。**写到一半写不进**（卡满了、中途
        // 被拔）是跑起来才撞上的，照旧记失败（[`run_sync`]）。
        if let Some(why) = target_absent(&prepared.root) {
            self.error = Some(why);
            return;
        }
        if prepared.plan.deletes.files > 0 && !self.acknowledged {
            self.error = Some(format!(
                "这份计划里有 {} 个删除（{}）。看过上面的预览之后，勾上「我看过删除清单」再来。",
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
        // **计划在这一刻整份交出去。** 闭包拿的是它自己那一份，屏上那一份此后作废也好、
        // 重排也好，都改不了台上这一趟要做的事（这条正是 ADR-0016 在搬上任务台之后
        // 还成立的原因）。
        self.syncing = Some(tasks.queue(format!("同步「{name}」"), move |task| {
            run_sync(&prepared, library_roots.as_ref(), task)
                .map(|outcome| Product::Synced(Box::new(outcome)))
        }));
        self.error = None;
        self.notice = None;
        self.failed = false;
    }

    /// 画一帧。
    ///
    /// **不必在这儿问「跑完没有」**：三条长活全在任务台上，窗口每帧问它一次
    /// （`App::poll_tasks`），台上有活时那一帧自己会请求下一帧。
    pub fn ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        // 屏头由窗口画（[`crate::look::screen_header`]：标题、副标题，右侧是 [`Self::status`]）；这一屏只画屏体
        // （[`look::screen_body`]：内边距照稿，整块竖着滚）。
        look::screen_body(ui, "子库屏体", |ui| self.cards_ui(ui, site, tasks));
        // 「目标设置」开着时盖在上面（[`crate::dialog`]）：遮罩盖住整个窗口，底下那一屏点不动。
        let ctx = ui.ctx().clone();
        self.target_dialog_ui(&ctx, site);
        // 「删除子库」那层确认弹层同一个路子；删掉之后底边那条提示条盖在最上面（[`crate::toast`]）。
        self.delete_dialog_ui(&ctx, site);
        self.rule_dialog_ui(&ctx, site);
        self.toast_ui(&ctx, site);
    }

    /// 屏头右侧属于这一屏的那一段（窗口调进 [`crate::look::screen_header`]）：一颗小号幽灵按钮「重新列一遍」，
    /// 最右头一颗小号主按钮「新建子库」（设计稿 `.scrhead`）。
    ///
    /// **「重新列一遍」照留**（拿主意的人 2026-09-14 选 B，挂单 `Q896`）：窗口只在开窗与从浏览屏回来时重读这一屏，
    /// 命令行在窗口开着时改过的子库、刚插上的卡，不按它就要等下一趟活才看得见。原来顶栏上的「N 台设备」（左栏角标
    /// 已经写着）与台上那一趟的进度、「停止」（卡上按钮旁边与任务屏上都按得着）照稿拿掉。
    pub fn status(&mut self, ui: &mut egui::Ui, site: &Site) {
        look::small_buttons(ui, |ui| {
            if ghost_button(ui, "重新列一遍")
                .on_hover_text("重读中立库里的子库，并查一眼每台设备的目标在不在位。")
                .clicked()
            {
                self.reload(site);
            }
            // 「算一遍容量」挪进了每张卡「容量」那一段的抬头（挂单 `Q857`）。
            if primary_button(ui, "新建子库")
                .on_hover_text("填名字与目标路径。选择集去浏览屏筛：筛到满意按「存成子库」。")
                .clicked()
            {
                self.begin_new();
            }
        });
    }

    /// 台上那一趟的进度与「停下」，摆在按下它的那个按钮旁边。
    ///
    /// **人是在这一屏点的，不该逼他先切去任务屏才知道跑到哪儿了。** 摆的是任务台那一份
    /// 快照（与任务屏上那一条同一个来源）；还排着队没轮到时说清它在等——不然按钮灰着、
    /// 屏上一个字没有，看着就像按坏了。
    fn live_ui(ui: &mut egui::Ui, tasks: &mut Tasks, id: Option<u64>, 干什么: &str) {
        let Some(id) = id else {
            return;
        };
        let Some(live) = tasks.running().filter(|live| live.id == id) else {
            if tasks.queued().iter().any(|(queued, _)| *queued == id) {
                ui.weak(format!("{干什么}排在任务台上等着（第 {id} 号）"));
                if look::buttons(ui, |ui| ui.button("撤掉")).clicked() {
                    tasks.stop(id);
                }
            }
            return;
        };
        ui.weak(format!(
            "正在{干什么}：{}（已用 {:.1} 秒）",
            live.progress.render(),
            live.elapsed.as_secs_f64(),
        ));
        if live.stopping {
            ui.colored_label(ui.visuals().warn_fg_color, "正在停……");
        } else if look::buttons(ui, |ui| ui.button("停下")).clicked() {
            tasks.stop(id);
        }
    }

    /// 「**新建子库**」：打开「新建子库」那层弹层，草稿换成一份空的，没有哪一张卡算摊开着。
    fn begin_new(&mut self) {
        self.picked = None;
        self.form = Form::default();
        self.invalidate();
        self.error = None;
        self.target_dialog = Some(TargetDialog::New);
    }

    /// 卡上「**目标设置…**」：草稿换成这一台的，打开那层弹层。**不摊开这张卡**：改目标设置与看差量是两件事，
    /// 弹层底下那张卡照旧是原来的样子（设计稿；第二段头一版在底下摊开了一张，露出了删除那一颗）。
    ///
    /// 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn edit_target(&mut self, name: &str) {
        if let Some(sublibrary) = self.list.iter().find(|row| row.name == name) {
            self.form = Form::of(sublibrary);
        }
        self.error = None;
        self.target_dialog = Some(TargetDialog::Of(name.to_string()));
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
        if self.list.is_empty() {
            // **空态，不是示例设备**（票 `gui-looks-like-the-design/20`）：设计稿脚本里那两台是给稿子
            // 看的数据。一台都没有时说清子库是什么，给一颗「新建子库」。
            if empty_ui(ui) {
                self.begin_new();
            }
            // 空态卡底下照稿也是那一行（稿上 `margin-top:14px`，间距档里最近的是 12）。
            ui.add_space(step(3));
            look::help(ui, TRIM_HELP);
            return;
        }
        let names: Vec<String> = self.list.iter().map(|row| row.name.clone()).collect();
        // **两列**（设计稿 `.devs`）；窄到并排放不下两张卡时一列。一张卡至少多宽取令牌里弹层的
        // 头一档——卡上那几行（规则、容量条图例、差量账）在那个宽度上摆得开。
        let columns = if ui.available_width() >= 2.0 * Tokens::builtin().layout.dialog_width[0] {
            2
        } else {
            1
        };
        // 竖着滚的是整块屏体（[`look::screen_body`]），这里不再套一层滚动区。
        ui.columns(columns, |cols| {
            for (at, name) in names.iter().enumerate() {
                let col = &mut cols[at % columns];
                self.card_ui(col, site, tasks, name);
                col.add_space(step(3));
            }
        });
        // 卡片底下那一行帮助字（设计稿子库屏）：超限只给建议，排除记成手动例外（ADR-0016）。
        look::help(ui, TRIM_HELP);
    }

    /// 一台设备那一张卡。
    fn card_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks, name: &str) {
        let Some(sublibrary) = self.list.iter().find(|row| row.name == name).cloned() else {
            return;
        };
        let open = self.picked.as_deref() == Some(name);
        // 卡片（设计稿 `.dev`：内边距 16）照 `look::card` 那一处画，底色、描边、圆角都从令牌来。
        look::card(ui, egui::Vec2::splat(step(3)), |ui| {
            ui.horizontal(|ui| {
                ui.label(font::strong(&sublibrary.name).size(Tokens::builtin().font.size_title));
                self.state_chip_ui(ui, &sublibrary);
                // 右边两颗照稿（设计稿 `devCard`）：`btn sm pri` 在左、`btn sm ghost`「目标设置…」靠右。
                // 主按钮的字照稿写「从浏览添加…」（拿主意的人定，挂单 `Q892`），行为不变：跳去浏览屏、
                // 这一台的规则预填进筛选器。
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    look::small_buttons(ui, |ui| {
                        if ghost_button(ui, "目标设置…")
                            .on_hover_text(
                                "改这台设备的名字、目标路径、前端格式、容量上限与能力档案。",
                            )
                            .clicked()
                        {
                            self.edit_target(name);
                        }
                        // 它带走的是「这一台的规则」，规则要等 `open` 读回来才在手上——没摊开的先摊开。
                        if primary_button(ui, "从浏览添加…")
                            .on_hover_text(
                                "跳去浏览屏，这个子库的规则预填进筛选器。\
                                 在那儿改得见它真的筛出了什么；调完按「更新到子库」原样带回。",
                            )
                            .clicked()
                        {
                            if !open {
                                self.open(site, name);
                            }
                            self.edit_selection();
                        }
                    });
                });
            });
            self.head_ui(ui, &sublibrary);
            ui.add_space(step(2));
            // 规则行尾按了「✎」就去浏览屏只改那一条；按了「×」先问一层（「移除规则」那层弹层）。那一层只画。
            match self.selection_ui(ui, name) {
                Some(RulePressed::Edit(ordinal)) => self.edit_rule(name, ordinal),
                Some(RulePressed::Remove(ordinal)) => self.ask_remove_rule(name, ordinal),
                None => {}
            }
            ui.add_space(step(2));
            // 「容量」那一段的抬头：左边小标题，右边「算一遍容量」——从屏头挪下来的（挂单 `Q857`）。
            // 按下去照旧一趟算全部设备；台上那一趟的进度与「停下」摆在它旁边。
            ui.horizontal(|ui| {
                look::section(ui, "容量");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let 按了 = look::small_buttons(ui, |ui| {
                        ui.add_enabled(self.evaluating.is_none(), egui::Button::new("算一遍容量"))
                            .on_hover_text(
                                "每台设备的选择集各选出多少、多大，装不装得下要看一眼那张卡——\
                                 卡不在手边的那台如实说算不出。\
                                 它排到任务台上跑，期间这一屏照常用。",
                            )
                            .clicked()
                    });
                    if 按了 {
                        self.evaluate(site, tasks);
                    }
                    Self::live_ui(ui, tasks, self.evaluating, "算容量");
                });
            });
            // 删减建议表上按了「排除」：写库与重算在这一层做，画容量条的那一层不动库。
            if let Some(key) = self.gauge_ui(ui, name) {
                self.exclude(site, tasks, name, &key);
            }
            ui.add_space(step(2));
            if open {
                self.delta_ui(ui, site, tasks);
                return;
            }
            // 没排过差量预览、目标又在位时，照稿先摆一条提示框（设计稿 `diffHTML` 的 `.note`）。
            if !self.absent.contains(name) {
                note_ui(ui, PREVIEW_NOTE);
                ui.add_space(step(2));
            }
            // **差量预览一次只摆一台的**（它是同步认的那一份计划，见 `prepared`）。别的卡底下照稿摆一排
            // 「生成差量预览 / 同步 / 删除子库」，按哪一颗都先换成这一台（[`Self::actions_ui`]）。
            self.actions_ui(ui, site, tasks, name, false);
        });
    }

    /// 卡底那一排（设计稿 `devCard` 底下）：「生成差量预览」「同步」，卡不在位时旁边一句「请先连接设备」，
    /// 右头一颗小号「删除子库」。
    ///
    /// **按哪一颗都先把这张卡摊开**（拿主意的人 2026-09-14 定，挂单 `Q859`）：差量预览、同步认的都是摊开那一台，
    /// 删除那层弹层问的也是它。**摊开那一张不在这一排摆「同步」**：它的「同步」在差量底下，与「我看过删除清单」
    /// 那一格摆在一起（[`Self::sync_ui`]）。
    ///
    /// 卡不在位时只写「请先连接设备」（照稿，拿主意的人定），路径与怎么办放进悬停；算过容量的话核心那句原话
    /// 也在悬停里（挂单 `Q851`）。**按钮不因为卡不在位就按不动**：拦在 `preview` 与 `sync` 里、只在屏上说
    /// （ADR-0005，票 `gui-looks-like-the-design/07`）。
    ///
    /// **包进横排**：卡片摆在 `ui.columns` 分出来的那一栏里，那一栏的版式是「撑满」，直接往里摆一颗按钮会被
    /// 拉成整栏宽（第二段头一版候选图上就是这样）。
    fn actions_ui(
        &mut self,
        ui: &mut egui::Ui,
        site: &mut Site,
        tasks: &mut Tasks,
        name: &str,
        open: bool,
    ) {
        /// 卡底按下去的是哪一颗。
        enum Pressed {
            /// 「生成差量预览」。
            Preview,
            /// 「同步」。
            Sync,
            /// 「删除子库」。
            Remove,
        }
        let mut pressed = None;
        let busy = self.syncing.is_some() || self.previewing.is_some();
        ui.horizontal(|ui| {
            // 默认那一档按钮照稿（设计稿 `.btn`，字取半号：[`look::buttons`]）。
            look::buttons(ui, |ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("生成差量预览"))
                    .on_hover_text(
                        "只读：中立库读一遍、目标设备看一遍，一个文件都不写。\
                     别的卡上摆着的那份差量会换成这一台的。它排到任务台上跑，期间这一屏照常用。",
                    )
                    .clicked()
                {
                    pressed = Some(Pressed::Preview);
                }
                if open {
                    // 正排着的时候把进度摆在按钮旁边：人是在这一屏点的，不该逼他先切去任务屏
                    // 才知道排到哪儿了。**停下也在这儿按得着。**
                    Self::live_ui(ui, tasks, self.previewing, "排差量");
                } else if ui
                    .add_enabled_ui(!busy, |ui| primary_button(ui, "同步"))
                    .inner
                    .on_hover_text(
                        "把这一台排过的那份差量真的落到目标设备上，只碰清单里记录过的文件。\
                     没排过差量预览的话先说一句，一个文件都不动。",
                    )
                    .clicked()
                {
                    pressed = Some(Pressed::Sync);
                }
            });
            if let Some(sublibrary) = self.list.iter().find(|row| row.name == name)
                && self.absent.contains(name)
            {
                let mut 悬停 = format!(
                    "未连接：{}。插上读卡器，或者按卡上「目标设置…」换一个目录。",
                    sublibrary.target
                );
                if let Some(Fit::Unknown { why }) =
                    self.evaluated.get(name).map(|report| &report.fit)
                {
                    悬停.push_str("\n\n");
                    悬停.push_str(why);
                }
                look::help(ui, "请先连接设备").on_hover_text(悬停);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if look::small_buttons(ui, |ui| {
                    ui.scope(|ui| {
                        look::warn_button(ui.visuals_mut());
                        ui.button("删除子库")
                    })
                    .inner
                })
                .on_hover_text(
                    "删掉这个子库的定义与它的规则、例外、清单；设备上的文件一个字节都不动。\
                         按下去先问一层。",
                )
                .clicked()
                {
                    pressed = Some(Pressed::Remove);
                }
            });
        });
        match pressed {
            Some(Pressed::Preview) => {
                if !open {
                    self.open(site, name);
                }
                self.preview(site, tasks);
            }
            Some(Pressed::Sync) => {
                if !open {
                    self.open(site, name);
                }
                self.sync(site, tasks);
            }
            Some(Pressed::Remove) => self.ask_remove(site, name),
            None => {}
        }
    }

    /// 卡底「**删除子库**」：先把这张卡摊开，再打开那层确认弹层（按下去真删走 [`Self::remove`]）。
    ///
    /// 界面上按那颗按钮走的就是它。
    pub fn ask_remove(&mut self, site: &Site, name: &str) {
        if self.picked.as_deref() != Some(name) {
            self.open(site, name);
        }
        self.delete_dialog = Some(name.to_string());
    }

    /// 「删除子库」那层确认弹层开着没有。
    #[must_use]
    pub fn delete_dialog_open(&self) -> bool {
        self.delete_dialog.is_some()
    }

    /// 卡头那枚**状态标签**（设计稿 `devState`），颜色照稿：
    ///
    /// - **未连接**——中性色、带圆点（稿上 `t-none`）。字照稿：拿主意的人裁了，词表里原来那个**连接**
    ///   （规则组里那几项怎么算数）改名（挂单 `Q853`）。**先看它**：稿上也是先问连没连着。
    /// - **待同步 N 步**——强调色、不带圆点（`t-acc plain`）。排过差量的那一台才有。
    /// - **已同步 · 清单 N 条**——放心色、不带圆点（`t-hi plain`）。排过差量、一步都不用做的那一台；N 是这一台
    ///   清单记着几条（[`Self::read_manifests`]）。清单读不动或者还是空的时候退回「已经对齐」，不编一个数。
    /// - **尚未生成差量预览**——强调色、不带圆点。没排过就说没排过，不摆一个 0——「一步都不用做」与
    ///   「还不知道要做什么」是两件事。
    fn state_chip_ui(&self, ui: &mut egui::Ui, sublibrary: &Sublibrary) {
        let name = sublibrary.name.as_str();
        if self.absent.contains(name) {
            look::chip(ui, look::Tone::Neutral, "未连接").on_hover_text(format!(
                "上一回看的时候 {} 不在。插上读卡器，或者按「目标设置…」换一个目录。",
                sublibrary.target
            ));
            return;
        }
        match self.plan_of(name) {
            Some(plan) if plan.touched() > 0 => {
                look::plain_chip(
                    ui,
                    look::Tone::Accent,
                    &format!("待同步 {} 个文件", thousands(plan.touched())),
                )
                .on_hover_text("排过差量预览：这一趟要动这么多个文件（一个文件一步）。");
            }
            Some(_) => match self.manifest_rows.get(name) {
                Some(rows) => {
                    look::plain_chip(
                        ui,
                        look::Tone::Good,
                        &format!("已同步 · 清单 {} 条", thousands(*rows as u64)),
                    )
                    .on_hover_text(
                        "排过差量预览：目标已经和选择集对齐，一步都不用做。\
                         清单记着上次同步放上去的这么多个文件。",
                    );
                }
                None => {
                    look::plain_chip(ui, look::Tone::Good, "已经对齐")
                        .on_hover_text("排过差量预览：目标已经和选择集对齐，一步都不用做。");
                }
            },
            None => {
                look::plain_chip(ui, look::Tone::Accent, "尚未生成差量预览").on_hover_text(
                    "同步前必须先看一遍它要做什么——那是硬要求，不是可以跳过的一步。",
                );
            }
        }
    }

    /// 卡头名字底下那一行：**路径 · 前端格式 · 文件系统 · 能力档案**（设计稿 `devCard`）。
    ///
    /// 档案与文件系统是名册解出来的那一份（[`Self::reload`]）。记着的名字在名册里没有时，
    /// 真会用上的是「不作声称」——照实写出来，并点名记着的是哪个：不说的话人会以为那份
    /// 档案生效了（ADR-0017：矩阵错了比不转换更糟）。
    fn head_ui(&self, ui: &mut egui::Ui, sublibrary: &Sublibrary) {
        let Some(profiled) = self.profiled.get(&sublibrary.name) else {
            ui.label(
                font::mono(format!("{} · {}", sublibrary.target, sublibrary.format))
                    .size(Tokens::builtin().font.size_small)
                    .weak(),
            );
            if let Some(why) = &self.roster_error {
                ui.colored_label(ui.visuals().warn_fg_color, why);
            }
            return;
        };
        ui.label(
            font::mono(format!(
                "{} · {} · {} · 能力档案：{}",
                sublibrary.target, sublibrary.format, profiled.filesystem, profiled.profile,
            ))
            // 设计稿 `.mono` 是正文的 0.92 倍，落在说明字号那一档。
            .size(Tokens::builtin().font.size_small)
            .weak(),
        );
        if let Some(recorded) = &profiled.missing {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!(
                    "名册里没有「{recorded}」这份能力档案，眼下按「{}」走。",
                    profiled.profile
                ),
            );
        }
    }

    /// 排出来那份计划，若它正好是这台设备的。
    fn plan_of(&self, name: &str) -> Option<&romcat_core::sync::Plan> {
        self.prepared
            .as_ref()
            .filter(|prepared| prepared.sublibrary.name == name)
            .map(|prepared| &prepared.plan)
    }

    /// **选择集**那一块（设计稿 `.rules`）：每条规则的名称、条件、命中多少、多大，例外那一行，
    /// 去重之后的合计。**每张卡都摆**，不只摊开那一张。
    ///
    /// 数整份来自核心折的 [`SelectionReport`]（与 `romcat sublibrary show` 印的是同一个值），
    /// 按过「算一遍容量」才有；没算过的那几格写「—」，不写 0——「还没算」与「一个都没选中」
    /// 是两件事。**名称是从条件拼出来的短名**（[`Rule::label`]，照设计稿 `autoName`）：中立库里一条规则
    /// 只存原文与序号，没有名字那一列（挂单 `Q811`）。第二行印规则原文。
    ///
    /// 改规则按卡头「从浏览添加…」去浏览屏：在那儿改得见它真的筛出了什么（票 `gui-redesign/11`）。
    fn selection_ui(&self, ui: &mut egui::Ui, name: &str) -> Option<RulePressed> {
        // 规则行尾「×」这一帧按了哪一条（序号）。**这一层只画**：问一层、删规则由卡片那一层做。
        let mut 按了 = None;
        let chosen = self.selections.get(name);
        let rules = chosen.map_or(&[][..], |chosen| &chosen.rules[..]);
        let broken = chosen.map_or(&[][..], |chosen| &chosen.broken[..]);
        let exceptions = chosen.map_or(&[][..], |chosen| &chosen.exceptions[..]);
        let report = self.evaluated.get(name);
        let caption = Tokens::builtin().font.size_caption;
        let visuals = ui.visuals().clone();
        egui::Frame::new()
            .stroke(visuals.widgets.noninteractive.bg_stroke)
            .corner_radius(visuals.window_corner_radius)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // 行与行之间不留缝：分隔线就是缝。
                ui.spacing_mut().item_spacing.y = 0.0;
                egui::Frame::new()
                    .fill(visuals.faint_bg_color)
                    .corner_radius(egui::CornerRadius {
                        sw: 0,
                        se: 0,
                        ..visuals.window_corner_radius
                    })
                    .inner_margin(block_margin())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let mut head = format!(
                                "选择集 · {} 条规则",
                                thousands((rules.len() + broken.len()) as u64)
                            );
                            if !exceptions.is_empty() {
                                head.push_str(&format!(
                                    " · {} 条例外",
                                    thousands(exceptions.len() as u64)
                                ));
                            }
                            look::section(ui, &head);
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                look::help(ui, "规则之间取并集");
                            });
                        });
                    });
                if rules.is_empty() && broken.is_empty() {
                    block_row(
                        ui,
                        "·",
                        |ui| {
                            ui.label(
                                egui::RichText::new("还没有规则。点「从浏览添加…」，按平台筛选后加入。")
                                    .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus))
                                    .weak(),
                            );
                        },
                        |_| {},
                    );
                }
                for stored in rules {
                    // **逐条各自算，不扣例外也不扣重叠**：这个数回答的是「我这条规则写对了吗」。
                    let line = report.and_then(|report| {
                        report
                            .rules
                            .iter()
                            .find(|line| line.ordinal == stored.ordinal)
                    });
                    block_row(
                        ui,
                        &stored.ordinal.to_string(),
                        |ui| {
                            ui.label(font::strong(
                                chosen
                                    .and_then(|chosen| chosen.labels.get(&stored.ordinal))
                                    .cloned()
                                    .unwrap_or_else(|| format!("规则 {}", stored.ordinal)),
                            ));
                            ui.label(font::mono(&stored.text).size(caption).weak());
                        },
                        |ui| {
                            // 行尾照稿两颗图标按钮「✎」「×」（设计稿 `devCard` 的 `.ra`）：右起先摆的在最右头，数量与容量在它
                            // 左边，再往左是「✎」（拿主意的人 2026-09-14 要求，挂单 `Q893`）。
                            if look::icon_button(ui, "×")
                                .on_hover_text("按了这条规则")
                                .clicked()
                            {
                                按了 = Some(RulePressed::Remove(stored.ordinal));
                            }
                            if look::pencil_button(ui)
                                .on_hover_text("在浏览中编辑这条规则")
                                .clicked()
                            {
                                按了 = Some(RulePressed::Edit(stored.ordinal));
                            }
                            match line {
                            Some(line) => {
                                ui.label(
                                    egui::RichText::new(format!(
                                        "{} 个 · {}",
                                        thousands(line.hits),
                                        human_bytes(line.bytes)
                                    ))
                                    .small(),
                                )
                                .on_hover_text(
                                    "这一条自己命中多少个变体、一共多大——不扣例外，也不扣与别条重叠的。",
                                );
                            }
                            None => {
                                ui.weak("—").on_hover_text("按「算一遍容量」之后才有。");
                            }
                            }
                        },
                    );
                }
                for row in broken {
                    block_row(
                        ui,
                        &row.ordinal.to_string(),
                        |ui| {
                            ui.label(font::strong(format!("规则 {}", row.ordinal)));
                            ui.colored_label(
                                visuals.error_fg_color,
                                format!(
                                    "{}. {}（读不懂：{}）——少选出来的东西全在它里面；\
                                     它没参与求值，「从浏览添加…」那一趟也不会碰它",
                                    row.ordinal, row.text, row.error,
                                ),
                            );
                        },
                        |_| {},
                    );
                }
                let (收入, 排除) = exception_tally(exceptions);
                block_row(
                    ui,
                    "+",
                    |ui| {
                        if exceptions.is_empty() {
                            ui.label(egui::RichText::new("没有手动例外").size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)).weak());
                            return;
                        }
                        let 顶用的 = report.map_or_else(String::new, |report| {
                            format!(
                                "（其中 {} 条收入是多余的、{} 条排除真起了作用）",
                                thousands(report.forced_in_redundant),
                                thousands(report.forced_out_effective),
                            )
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "手动例外：收入 {} 条、排除 {} 条{顶用的}",
                                thousands(收入),
                                thousands(排除),
                            ))
                            .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)),
                        )
                        .on_hover_text(
                            "优先于规则、永久记住：规则表达不了的个人口味。\
                             加减在浏览屏的详情面板里做。",
                        );
                    },
                    |_| {},
                );
                ui.add(egui::Separator::default().spacing(0.0));
                egui::Frame::new()
                    .inner_margin(block_margin())
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal_wrapped(|ui| match report {
                            Some(report) => {
                                ui.label(font::strong(format!(
                                    "合计 {} 个变体 · {}",
                                    thousands(report.picked),
                                    human_bytes(report.bytes),
                                )).size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)));
                                // 被几条规则同时选中、只算一次的有几个由核心数（`Selected::overlaps`），
                                // 这儿只照着写。话照设计稿（拿主意的人定）：这里的「重复」说的是几条规则
                                // 选中了同一个变体，与词表**重复拷贝**不是一回事。
                                ui.weak(if report.overlaps > 0 {
                                    format!(
                                        "已去除 {} 个被多条规则同时选中的变体",
                                        thousands(report.overlaps),
                                    )
                                } else {
                                    "规则之间没有重复".to_string()
                                });
                            }
                            None => {
                                ui.label(font::strong("合计 —").size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)));
                                ui.weak("按「算一遍容量」之后才有");
                            }
                        });
                    });
            });
        if !broken.is_empty() {
            // **处置它的那条路在浏览屏上**（票 `gui-redesign/14`，挂单 `Q86`）：筛选器摆的是一棵
            // 读得懂的树，一条读不回来的原文在那儿没有位置，所以它跟着「改选择」那一趟整条带过去，
            // 摆在**筛选栏顶上**那条横幅里逐条扔。
            ui.weak(format!(
                "读不懂的这 {} 条要扔掉：按上面「从浏览添加…」跳去浏览屏，\
                 筛选栏顶上那条横幅里逐条扔得掉。改对了再来一条，走那儿的筛选器。",
                broken.len(),
            ))
            .on_hover_text(
                "命令行那条路也还在：`romcat sublibrary rule <子库> --remove <序号>`，\
                 序号就是上面印着的那个。",
            );
        }
        let Some(report) = report else {
            return 按了;
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
                     缺数据，不是规则写错了。",
                    report.thin_dimensions.join("、"),
                ),
            );
        }
        按了
    }

    /// **容量条**：选中的、清单之外的、上限，三段各自标得出数；超限时底下是删减建议表。
    ///
    /// 返回删减建议表上这一帧按了「排除」的那个变体的键——**这一层只画**，写库与重算由卡片那一层
    /// 调 [`Self::exclude`]。
    fn gauge_ui(&self, ui: &mut egui::Ui, name: &str) -> Option<String> {
        let gauge = self.gauge(name);
        // 超没超由核心一处算（同步计划器），条子只拿它换选中那一段的颜色（`room_of`）。
        let room = self.room_of(name);
        let over = room.as_ref().and_then(|room| room.over_capacity).is_some();
        // 选中那一段**算过才有数**（算过容量或排过差量）：没算过写「还没算」，不写 0。
        let counted = room.is_some() || self.evaluated.contains_key(name);
        let visuals = ui.visuals().clone();
        let 选中色 = if over {
            visuals.error_fg_color
        } else {
            visuals.selection.stroke.color
        };
        let 之外色 = visuals.warn_fg_color;
        let 未知描边 = visuals.widgets.inactive.bg_stroke;
        // 条高照稿（设计稿 `.gauge` 的 10 点，令牌 `gauge-height`）。「容量」那个小标题摆在卡片那一层的抬头里。
        let height = Tokens::builtin().layout.gauge_height;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), height),
            egui::Sense::hover(),
        );
        let rounding = height / 2.0;
        let painter = ui.painter();
        painter.rect_filled(rect, rounding, visuals.extreme_bg_color);
        let picked = rect.width() * gauge.picked_share();
        if picked > 0.0 {
            painter.rect_filled(
                egui::Rect::from_min_size(rect.min, egui::vec2(picked, height)),
                rounding,
                选中色,
            );
        }
        let rest_left = rect.left() + picked;
        match gauge.strangers {
            Some(_) => {
                let width = rect.width() * gauge.stranger_share();
                if width > 0.0 {
                    painter.rect_filled(
                        egui::Rect::from_min_size(
                            egui::pos2(rest_left, rect.top()),
                            egui::vec2(width, height),
                        ),
                        rounding,
                        之外色,
                    );
                }
            }
            // **「还不知道」画成一段斜纹，不画成零**（设计稿 `.gauge .unk`）：卡不在手边时目标上
            // 有什么本来就没看过，一段也不画等于说「卡上是空的」。
            None => {
                let width = (rect.right() - rest_left).min(rect.width() * UNKNOWN_SHARE);
                hatch(
                    painter,
                    egui::Rect::from_min_size(
                        egui::pos2(rest_left, rect.top()),
                        egui::vec2(width, height),
                    ),
                    未知描边,
                );
            }
        }
        ui.horizontal_wrapped(|ui| {
            legend_swatch(ui, 选中色);
            // 图例带「（N 个变体）」（设计稿 `.legend`）。几个由核心数：算过容量的照报告，排过差量的照那份计划。
            let 几个 = self
                .evaluated
                .get(name)
                .map(|report| report.picked)
                .or_else(|| {
                    self.prepared
                        .as_ref()
                        .filter(|prepared| prepared.sublibrary.name == name)
                        .map(|prepared| prepared.selected.picked.len() as u64)
                });
            let 选中 = match (counted, 几个) {
                (true, Some(几个)) => format!(
                    "已选 {}（{} 个变体）",
                    human_bytes(gauge.picked),
                    thousands(几个)
                ),
                (true, None) => format!("已选 {}", human_bytes(gauge.picked)),
                (false, _) => "已选 还没算".to_string(),
            };
            // 图例的字与字号都照稿（设计稿 `.legend` 的 12px：「已选 / 清单外文件 / 容量上限」，拿主意的人
            // 照稿定，挂单 `Q891`）。
            ui.label(egui::RichText::new(选中).small()).on_hover_text(
                "这个子库在卡上占的地方。看过目标之后（算过容量或排过差量预览）算的是同步完的样子\
                 ——元数据与媒体也要占地方，转换又省下来一些，而卡上还留着那些\
                 「对不上、本次不动」的文件。卡不在手边时就是选择集选出来那批变体一共多大。",
            );
            ui.add_space(step(3));
            match gauge.strangers {
                None => {
                    legend_swatch(ui, 未知描边.color);
                    ui.label(egui::RichText::new("清单外文件：未知").small());
                }
                Some(bytes) => {
                    legend_swatch(ui, 之外色);
                    ui.label(
                        egui::RichText::new(format!("清单外文件 {}", human_bytes(bytes))).small(),
                    )
                    .on_hover_text(
                        "工具没放过的文件：维护者自己拷进去的存档、金手指、截图。连看都不看。",
                    );
                }
            }
            ui.add_space(step(3));
            match gauge.capacity {
                None => ui.label(egui::RichText::new("容量上限 不设限").small().weak()),
                Some(bytes) => ui.label(
                    egui::RichText::new(format!("容量上限 {}", decimal_bytes(bytes)))
                        .small()
                        .weak(),
                ),
            };
        });
        // 卡不在位时图例底下一行普通小字（设计稿 `devCard` 的 `.help`；逐字照稿、样式也照稿，拿主意的人定）。
        if self.absent.contains(name) {
            look::help(ui, ABSENT_NOTE);
        }
        if !counted {
            note_ui(ui, "还没算过：按上面「算一遍容量」，或者排一次差量预览。");
        }
        // 超出多少、砍谁由核心一处算（同步计划器），这儿只把它摆出来。
        let excluded = room.as_ref().and_then(|room| trim_ui(ui, room));
        // 算过容量却算不出装不装得下：**不给数**，不拿选中容量去冒充「装得下」。
        //
        // **卡上说这一层自己的话，核心那句原话放进悬停**：原话带着系统错误的原文（各平台不一样）与
        // 命令行命令，屏上不该出现（票 `gui-looks-like-the-design/03`；挂单 `Q851`）。卡不在位那一种不在这儿说：
        // 卡底按钮旁边照稿写「请先连接设备」，路径、怎么办与这句原话都在它的悬停里（[`Self::actions_ui`]）。
        if room.is_none()
            && !self.absent.contains(name)
            && let Some(Fit::Unknown { why }) = self.evaluated.get(name).map(|report| &report.fit)
        {
            look::help(ui, "装不装得下算不出（指针停在这儿看为什么）").on_hover_text(why.as_str());
        }
        excluded
    }

    /// 卡的下半截：**排差量、看步骤、按同步**。
    fn delta_ui(&mut self, ui: &mut egui::Ui, site: &mut Site, tasks: &mut Tasks) {
        // 头一排与没摊开的卡底下是同一排（[`Self::actions_ui`]）；摊开这一张的「同步」在差量底下。
        let name = self.picked.clone().unwrap_or_default();
        self.actions_ui(ui, site, tasks, &name, true);
        let Some(prepared) = self.prepared.clone() else {
            // 差量作废了、而同步还在台上跑着的话，这一句底下还得摆得出那一趟的进度
            // ——不然人一按下同步，屏上就什么都没有了。
            Self::live_ui(ui, tasks, self.syncing, "同步");
            note_ui(ui, PREVIEW_NOTE);
            return;
        };
        self.plan_ui(ui, site, tasks, &prepared);
    }

    /// 那份计划本身：账、要说出口的怪事、步骤，以及「真的传」。
    fn plan_ui(&mut self, ui: &mut egui::Ui, site: &Site, tasks: &mut Tasks, prepared: &Prepared) {
        let plan = &prepared.plan;
        ui.separator();
        tally_ui(ui, plan, self.prepare_ms);
        concerns_ui(ui, prepared);
        self.steps_ui(ui, plan);
        self.sync_ui(ui, site, tasks, plan);
    }

    /// 步骤那一段：收起来时摆头几条，摊开是那张虚拟化的表。
    fn steps_ui(&mut self, ui: &mut egui::Ui, plan: &romcat_core::sync::Plan) {
        ui.add_space(4.0);
        let header = ui
            .horizontal(|ui| {
                ui.label(font::strong(format!(
                    "这一趟要动的 {} 步（先删后传）",
                    thousands(plan.touched())
                )));
                if plan.steps.len() > STEP_SAMPLE {
                    let label = if self.expanded {
                        "收起来"
                    } else {
                        "全部展开"
                    };
                    if look::buttons(ui, |ui| ui.button(label)).clicked() {
                        self.expand(!self.expanded);
                    }
                }
            })
            .response;
        if self.expanded {
            // **表里不按滚动位置**：人自己滚。`scroll_to` 是给实测与测试的
            // （[`steps_table`]）。
            self.steps_drawn = steps_table(ui, plan, None);
            // 刚摊开：把卡片那一列滚到这张表的表头（`reveal_steps` 的文档）。**要在表画完之后问**：
            // 表自己也是一块滚动区，在它之前问的话，那一下被它先收走，滚的是表里头而不是卡片那一列。
            // **不带动画**：一下到位。带动画的话要跑上十几帧才滚到，而这一下是替人把刚摊开的表
            // 摆到眼前，不是一段要看的过渡。
            if self.reveal_steps {
                ui.scroll_to_rect_animation(
                    header.rect,
                    Some(Align::Min),
                    egui::style::ScrollAnimation::none(),
                );
                self.reveal_steps = false;
            }
            return;
        }
        self.steps_drawn = 0;
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
    fn sync_ui(
        &mut self,
        ui: &mut egui::Ui,
        site: &Site,
        tasks: &mut Tasks,
        plan: &romcat_core::sync::Plan,
    ) {
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
            let ready = self.syncing.is_none()
                && plan.touched() > 0
                && (plan.deletes.files == 0 || self.acknowledged);
            if look::buttons(ui, |ui| ui.add_enabled(ready, egui::Button::new("同步")))
                .on_hover_text(
                    "把上面这份差量真的落到目标设备上。只碰清单里记录过的文件。\
                     它排到任务台上跑：进度、已用时间、停下都在任务屏上，\
                     跑的是这一刻摆着的这一份计划——排上去之后改规则也改不了它。",
                )
                .clicked()
            {
                self.sync(site, tasks);
            }
            Self::live_ui(ui, tasks, self.syncing, "同步");
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
            ui.label(font::strong(""));
            ui.label(font::strong("文件"));
            ui.label(font::strong("变体"));
            ui.label(font::strong("容量"));
            ui.end_row();
            for (what, tally) in [
                ("新增", plan.adds),
                ("更新", plan.updates),
                ("删除", plan.deletes),
                ("原样留着", plan.keeps),
            ] {
                ui.label(what);
                // 数量与容量用等宽：四行账竖着比大小。
                ui.label(font::mono(thousands(tally.files)));
                ui.label(font::mono(thousands(tally.variants)));
                ui.label(font::mono(human_bytes(tally.bytes)));
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
                "{} 份放不进目标，这一趟既不新增也不删除——它们进不了卡，\
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
            ui.weak(format!("……另有 {} 份没列", plan.rejected.len() - TOP_NOTES));
        }
    }
    if !plan.unsupported.is_empty() {
        // **照搬，但点名说出口**（ADR-0017：矩阵错了比不转换更糟）。不说的话，
        // 人会以为工具已经替他处理妥当，直到在掌机上打不开才发现。
        ui.colored_label(
            ui.visuals().warn_fg_color,
            format!(
                "{} 份目标吃不下、而这一版转不了：照样传过去，但它在这台设备上多半打不开。",
                thousands(plan.unsupported.len() as u64),
            ),
        );
        for row in plan.unsupported.iter().take(TOP_NOTES) {
            ui.label(format!(
                "{}｜{}｜要的是 {}",
                row.path,
                human_bytes(row.bytes),
                row.want
            ))
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
            "{} 件对不上的事，本次一律不动它们：",
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
    /// 「**目标设置**」那层弹层（[`crate::dialog`]）：名字、目标路径、前端格式、容量上限、能力档案。
    ///
    /// 屏头「新建子库」开一份空草稿，卡上「目标设置…」开那一台的（[`Self::begin_new`] /
    /// [`Self::edit_target`]）。字段与存法照原先底下那块「配目标」面板原样搬来（票
    /// `gui-looks-like-the-design/20` 第二段）；逐平台列能力档案、路径三种校验、落点预览归票 `21`，
    /// 在这层弹层上接着做。标题、说明、按钮上的字与宽照稿（设计稿 `DLG.subform` 的 `w:720`：令牌第三档）。
    ///
    /// **中文输入在这层的内容区里**：内容区每帧把整份内容都摆一遍，正在组字的那一格不会凭空消失
    /// （[`crate::dialog`] 模块文档、ADR-0005）。
    fn target_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」。
            Cancel,
            /// 「创建子库」或「保存」。
            Save,
        }
        let Some(which) = self.target_dialog.clone() else {
            return;
        };
        let ready = !self.form.name.trim().is_empty() && !self.form.target.trim().is_empty();
        let (title, note, save_label) = match &which {
            TargetDialog::New => (
                "新建子库".to_string(),
                "子库对应一台设备（通常是掌机）。创建后在「浏览」中按平台筛选，把结果分几次加入它的选择集。",
                "创建子库",
            ),
            TargetDialog::Of(name) => (
                format!("目标设置 · {name}"),
                "修改目标路径、前端格式或能力档案后，已有的差量预览会失效，同步前需要重新生成。",
                "保存",
            ),
        };
        let footer = Footer::new(Button::new("取消", Pressed::Cancel)).button(
            Button::new(save_label, Pressed::Save)
                .primary()
                .enabled(ready)
                .hover("新建或改写这台设备。目标设备不在位也存得下——子库是持久实体。"),
        );
        let error = self.error.clone();
        let form = &mut self.form;
        let shown = Dialog::new("目标设置", title, footer)
            .note(note)
            .width(Width::Wide)
            .show(ctx, |ui| {
                if let Some(error) = &error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                let layout = &Tokens::builtin().layout;
                for (label, value, hint) in [
                    ("名称", &mut form.name, "例如设备型号：RG35XX Plus"),
                    ("目标路径", &mut form.target, "读卡器挂上来的那个目录"),
                    ("前端格式", &mut form.format, "空着就是 Pegasus"),
                    ("容量上限", &mut form.capacity, "如 512GB；空着不设限"),
                    ("能力档案", &mut form.capability, "空着就是不作声称"),
                ] {
                    ui.horizontal(|ui| {
                        // 名那一列靠左（设计稿弹层表单）：`add_sized` 会把字摆在格子正中。
                        ui.allocate_ui_with_layout(
                            egui::vec2(layout.kv_key_width, layout.input_height),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_min_size(egui::vec2(
                                    layout.kv_key_width,
                                    layout.input_height,
                                ));
                                ui.label(label);
                            },
                        );
                        let width = ui.available_width();
                        look::text_input(
                            ui,
                            width,
                            egui::TextEdit::singleline(value).hint_text(hint),
                        );
                    });
                }
            });
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => {
                self.target_dialog = None;
                self.error = None;
            }
            Some(Pressed::Save) => {
                if self.save(site) {
                    self.target_dialog = None;
                }
            }
        }
    }

    /// 存下这个子库。**界面上按那个按钮走的就是它**，实测与测试拿它当那一下。
    ///
    /// **目标落在主库里当场拦下**（ADR-0004、验收第 8 条）：判据在核心里
    /// （`sync::prepare::refuse_target_in_library`），与同步那一道是同一条。
    /// 拦在存下来这一步而不是等到同步，是因为一个指着主库的子库定义放在库里，
    /// 下一次点同步之前谁都不知道它错了。
    ///
    /// 交回存没存下来：「目标设置」那层弹层按它决定关不关——没存下来时弹层开着，那句话画在弹层里。
    pub fn save(&mut self, site: &mut Site) -> bool {
        let written = self.form.capacity.trim();
        let capacity = match &self.form.kept_capacity {
            // 字没改过：沿用原来的字节数，不重新解析（[`Form::kept_capacity`]）。
            Some((shown, bytes)) if written == shown.trim() => Some(*bytes),
            _ if written.is_empty() => None,
            _ => match rule::parse_size(written) {
                Some(bytes) => Some(bytes),
                None => {
                    self.error = Some(format!(
                        "看不懂容量「{written}」。写成 `512GB` 或 `476GiB` 那样，单位得写全。",
                    ));
                    return false;
                }
            },
        };
        let name = self.form.name.trim().to_string();
        let target = std::path::PathBuf::from(self.form.target.trim());
        if let Err(message) = sync::prepare::refuse_target_in_library(&site.catalog, &[], &target) {
            self.error = Some(message);
            return false;
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
                // **算过的那份跟着作废**：容量上限改了，报告里的「超出多少、砍谁」
                // 说的还是上一个上限——卡上会出现「上限写着 1 TB、旁边说超了 200 GiB」。
                // **台上那趟还没认领的也一样**（[`Self::drop_survey`]）：它折报告用的正是
                // 改之前那份子库，收回来等于把刚改掉的上限又摆回屏上。
                self.evaluated.remove(&name);
                let mut line = format!("存下了子库「{name}」。");
                if let Some(说一句) = self.drop_survey() {
                    line.push(' ');
                    line.push_str(说一句);
                }
                self.notice = Some(line);
                self.reload(site);
                self.open(site, &name);
                true
            }
            Err(error) => {
                self.error = Some(format!("中立库写不动：{error}"));
                false
            }
        }
    }

    /// 删掉摊开那一台的定义。**目标设备上的文件一个都不碰。** 「删除子库」那层弹层上按「删除子库」走的就是它，
    /// 实测与测试拿它当那一下。
    ///
    /// 删之前核心把这一台整份留下来交回
    /// （[`Catalog::take_sublibrary`](romcat_core::catalog::Catalog::take_sublibrary)），这一屏拿着它摆一条带
    /// 「撤销」的提示条（[`Self::undo_remove`]）。再删一台的话上一台那一份就丢了——提示条只摆一条。
    pub fn remove(&mut self, site: &mut Site) {
        let Some(name) = self.picked.clone() else {
            return;
        };
        self.delete_dialog = None;
        match site.catalog.take_sublibrary(&name) {
            Ok(Some(removed)) => {
                self.picked = None;
                self.evaluated.remove(&name);
                // 台上那趟还没认领的也不认了——认领是整份替换，刚删掉这一台的报告会
                // 又长回来（[`Self::drop_survey`]）。删掉了什么由提示条说，这儿只留那一句。
                self.notice = self.drop_survey().map(ToString::to_string);
                self.undo = Some(Undo {
                    toast: Toast::new(format!("已删除子库「{name}」，设备上的文件没有改动"))
                        .action("撤销"),
                    removed,
                });
                self.invalidate();
                self.reload(site);
            }
            Ok(None) => self.notice = Some("那个子库已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 提示条上按「**撤销**」：把刚删掉的那一台**原样放回去**——规则、例外、清单与删之前一样
    /// （[`Catalog::restore_sublibrary`](romcat_core::catalog::Catalog::restore_sublibrary)）。提示条跟着收起。
    ///
    /// 界面上按那颗按钮走的就是它。提示条已经收起、或者换过屏之后（[`Self::leave`]）什么都不做。
    pub fn undo_remove(&mut self, site: &mut Site) {
        let Some(undo) = self.undo.take() else {
            return;
        };
        let name = undo.removed.name().to_string();
        match site.catalog.restore_sublibrary(&undo.removed) {
            Ok(true) => {
                self.notice = None;
                self.reload(site);
            }
            // 删完之后又建了一个同名的：核心一行都没写，两份不揉在一起。
            Ok(false) => {
                self.error = Some(format!(
                    "放不回去：这会儿已经又有一个叫「{name}」的子库了。\
                     两份揉在一起谁的清单都说不清，所以一行都没写。"
                ));
            }
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 「**删除子库**」那层确认弹层（设计稿 `DLG.delsub`）：说清删的是这一份子库的定义——规则、例外、清单记录——
    /// 设备上的文件一个字节都不动；删之前把名字（标题上）与目标路径写明。按「删除子库」才真删（[`Self::remove`]）。
    fn delete_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」。
            Cancel,
            /// 「删除子库」。
            Remove,
        }
        let Some(name) = self.delete_dialog.clone() else {
            return;
        };
        let Some(target) = self
            .list
            .iter()
            .find(|row| row.name == name)
            .map(|row| row.target.clone())
        else {
            // 弹层开着的时候那一台已经没了（别处删掉、重新列一遍）：没东西可问。
            self.delete_dialog = None;
            return;
        };
        let chosen = self.selections.get(&name);
        let rules = chosen.map_or(0, |chosen| chosen.rules.len() + chosen.broken.len());
        let exceptions = chosen.map_or(0, |chosen| chosen.exceptions.len());
        let picked = self
            .evaluated
            .get(&name)
            .map(|report| (report.picked, report.bytes));
        let footer = Footer::new(Button::new("取消", Pressed::Cancel)).button(
            Button::new("删除子库", Pressed::Remove).danger().hover(
                "只删中立库里的这一份定义；设备上的文件一个字节都不动。\
                 删完之后底边提示条上按「撤销」原样放回来。",
            ),
        );
        let tokens = Tokens::builtin();
        let shown = Dialog::new("删除子库", format!("删除子库「{name}」"), footer)
            .width(Width::Narrow)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!(
                        "只删除这个子库的定义：{} 条规则、{} 条例外和清单记录。",
                        thousands(rules as u64),
                        thousands(exceptions as u64),
                    ))
                    .size(look::font_size(ui.ctx(), tokens.font.size_small_plus)),
                );
                ui.horizontal_wrapped(|ui| {
                    look::help(ui, "目标路径");
                    ui.label(font::mono(&target).size(tokens.font.size_small));
                });
                ui.add_space(step(2));
                impact_ui(
                    ui,
                    &[
                        ("设备上已经同步的文件", false),
                        ("不会删除", true),
                        ("。删除后它们成为清单外的文件，工具不再管理。", false),
                    ],
                );
                impact_ui(
                    ui,
                    &[(
                        "如果想同时清空设备：先移除全部规则并同步一次，再删除子库。",
                        false,
                    )],
                );
                impact_ui(ui, &[("主库和元数据不受影响。", false)]);
                if let Some((picked, bytes)) = picked {
                    ui.add_space(step(2));
                    look::help(
                        ui,
                        &format!(
                            "当前选择集：{} 个变体 · {}",
                            thousands(picked),
                            human_bytes(bytes)
                        ),
                    );
                }
            });
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => self.delete_dialog = None,
            Some(Pressed::Remove) => {
                if self.picked.as_deref() != Some(name.as_str()) {
                    self.open(site, &name);
                }
                self.remove(site);
            }
        }
    }

    /// 规则行上「**×**」：打开那层确认弹层，移除的是这一台的第 `ordinal` 条规则。界面上按那颗按钮走的就是它。
    pub fn ask_remove_rule(&mut self, name: &str, ordinal: i64) {
        self.rule_dialog = Some((name.to_string(), ordinal));
    }

    /// 「移除规则」那层确认弹层开着没有。
    #[must_use]
    pub fn rule_dialog_open(&self) -> bool {
        self.rule_dialog.is_some()
    }

    /// 「移除规则」那层弹层上按「移除这条规则」：从中立库里删掉这一台的第 `ordinal` 条规则（`Catalog::remove_rule`，
    /// 删掉的号不再发），这一台缓着的容量账与差量预览当场作废、规则列表重读（[`Self::forget`]）。
    /// **设备上的文件与主库一个字节都不动。** 界面上按那颗按钮走的就是它，实测与测试拿它当那一下。
    pub fn remove_rule(&mut self, site: &mut Site, name: &str, ordinal: i64) {
        self.rule_dialog = None;
        match site.catalog.remove_rule(name, ordinal) {
            Ok(true) => {
                self.forget(site, name);
                self.notice = Some(format!(
                    "从子库「{name}」移除了第 {ordinal} 条规则。选中的变体跟着变了：\
                     容量要重算，差量预览要重新生成。"
                ));
            }
            Ok(false) => self.notice = Some("那一条规则已经不在了。".to_string()),
            Err(error) => self.error = Some(format!("中立库写不动：{error}")),
        }
    }

    /// 「**移除规则**」那层确认弹层：写清移除的是哪一台的哪一条（短名与原文）、移除之后选中的变体会变（算过容量的话
    /// 写出这一条自己命中多少），差量预览跟着作废；设备上的文件不动。按「移除这条规则」才真删（[`Self::remove_rule`]）。
    fn rule_dialog_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        /// 页脚上按下去的是哪一颗。
        enum Pressed {
            /// 「取消」。
            Cancel,
            /// 「移除这条规则」。
            Remove,
        }
        let Some((name, ordinal)) = self.rule_dialog.clone() else {
            return;
        };
        let chosen = self.selections.get(&name);
        let Some(text) = chosen
            .and_then(|chosen| chosen.rules.iter().find(|rule| rule.ordinal == ordinal))
            .map(|rule| rule.text.clone())
        else {
            // 弹层开着的时候那一条已经没了（别处改过、重新列一遍）：没东西可问。
            self.rule_dialog = None;
            return;
        };
        let label = chosen
            .and_then(|chosen| chosen.labels.get(&ordinal))
            .cloned()
            .unwrap_or_else(|| format!("规则 {ordinal}"));
        let 命中 = self.evaluated.get(&name).and_then(|report| {
            report
                .rules
                .iter()
                .find(|line| line.ordinal == ordinal)
                .map(|line| (line.hits, line.bytes))
        });
        let footer = Footer::new(Button::new("取消", Pressed::Cancel))
            .button(Button::new("移除这条规则", Pressed::Remove).danger().hover(
            "只从中立库里删掉这一条规则；设备上的文件一个字节都不动，下一趟同步照新的选择集去对。",
        ));
        let tokens = Tokens::builtin();
        let shown = Dialog::new("移除规则", format!("移除规则「{label}」"), footer)
            .width(Width::Narrow)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("从子库「{name}」的选择集里移除第 {ordinal} 条规则："))
                        .size(look::font_size(ui.ctx(), tokens.font.size_small_plus)),
                );
                ui.horizontal_wrapped(|ui| {
                    ui.label(font::strong(&label));
                    ui.label(font::mono(&text).size(tokens.font.size_small));
                });
                ui.add_space(step(2));
                let 会怎样 = match 命中 {
                    Some((hits, bytes)) => format!(
                        "移除之后选中的变体会变：这一条自己命中 {} 个（{}），其中别的规则也选中的那几个照旧留着。",
                        thousands(hits),
                        human_bytes(bytes)
                    ),
                    None => "移除之后选中的变体会变；少多少，按「算一遍容量」之后才看得见。".to_string(),
                };
                impact_ui(ui, &[(会怎样.as_str(), false)]);
                impact_ui(ui, &[("排过的差量预览跟着作废，同步前要重新生成。", false)]);
                impact_ui(ui, &[("设备上的文件与主库都不受影响。", false)]);
            });
        match shown.pressed {
            None => {}
            Some(Pressed::Cancel) => self.rule_dialog = None,
            Some(Pressed::Remove) => self.remove_rule(site, &name, ordinal),
        }
    }

    /// 底边那条提示条（[`crate::toast`]）：删掉一台之后那一条，带「撤销」。停够了收起，撤销也跟着没了。
    fn toast_ui(&mut self, ctx: &egui::Context, site: &mut Site) {
        let Some(undo) = &mut self.undo else {
            return;
        };
        match undo.toast.show(ctx) {
            toast::Shown::Showing => {}
            toast::Shown::Pressed => self.undo_remove(site),
            toast::Shown::Expired => self.undo = None,
        }
    }
}

/// 一颗**主按钮**：强调色底。颜色只从 [`look::primary_button`] 那一处来，换在一个 `scope` 里，
/// 别的控件不受影响。
fn primary_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        look::primary_button(ui.visuals_mut());
        ui.button(label)
    })
    .inner
}

/// 一条**提示框**（设计稿 `.note`）：框照 [`look::note`] 画，字照稿是半号那一档（令牌 `size-small-plus`，稿 12.5）。
///
/// 不改共用的 [`look::note`]：开场那几张基线里也有它，字号跟着变就得重批那几张。
fn note_ui(ui: &mut egui::Ui, text: &str) {
    // 半号字按像素倍率取整（[`look::font_size`]）：1 倍屏上 12.5 的中文字距忽宽忽窄。
    let 字号 = look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus);
    look::note(ui, egui::RichText::new(text).size(字号));
}

/// 一颗**幽灵按钮**（设计稿 `.btn.ghost`）：没底没框、次要字色。颜色只从 [`look::ghost_button`] 那一处来。
fn ghost_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.scope(|ui| {
        look::ghost_button(ui.visuals_mut());
        ui.button(label)
    })
    .inner
}

/// **还没有子库**时那一块：一张居中的卡（[`look::card`]），说清子库是什么，给一颗「新建子库」。
/// 返回按没按。
///
/// 宽照设计稿那张空态卡（620 点），与弹层第二档同宽，取令牌里那一档；内边距照稿（令牌 `empty-card-padding`）。
fn empty_ui(ui: &mut egui::Ui) -> bool {
    let width = Tokens::builtin().layout.dialog_width[1].min(ui.available_width());
    let mut clicked = false;
    ui.add_space(step(4));
    ui.horizontal(|ui| {
        ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
        ui.vertical(|ui| {
            ui.set_width(width);
            look::card(
                ui,
                egui::Vec2::splat(Tokens::builtin().layout.empty_card_padding),
                |ui| {
                    ui.vertical_centered(|ui| {
                        // **字号直接取令牌，不取具名档 `look::TITLE`**：空库上窗口的头一帧就画到
                        // 这里，而具名档是 `App::prepare` 在那一帧里才装上的——这一帧的 `Ui`
                        // 拿的还是装之前那份样式，按名字找会当场 panic。
                        ui.label(
                            font::strong("还没有子库")
                                .size(Tokens::builtin().font.size_empty_title),
                        );
                        ui.add_space(step(1));
                        ui.label(
                            egui::RichText::new(
                                "子库是为一台设备（通常是掌机）挑出来的一部分内容。\
                                 新建一个子库，然后在「浏览」中按平台分几次加入。",
                            )
                            .weak(),
                        );
                        // 说明与按钮之间照稿（`margin:8px 0 16px`）。
                        ui.add_space(step(3));
                        clicked = look::buttons(ui, |ui| primary_button(ui, "新建子库")).clicked();
                    });
                },
            );
        });
    });
    clicked
}

/// 容量条上「还不知道」那一段的斜纹（设计稿 `.gauge .unk`）：强一级描边色的斜条，一个来回一道，一半有色。
///
/// 描边颜色取 `widgets.inactive.bg_stroke`，由 [`look::install`] 照令牌 `line-2` 装好；一个来回多宽取令牌
/// `gauge-hatch`，斜条宽是它的一半。
fn hatch(painter: &egui::Painter, rect: egui::Rect, stroke: egui::Stroke) {
    if rect.width() <= 0.0 {
        return;
    }
    let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let gap = Tokens::builtin().layout.gauge_hatch;
    let stroke = egui::Stroke::new(gap / 2.0, stroke.color);
    let mut x = rect.left() - rect.height();
    while x < rect.right() {
        painter.line_segment(
            [
                egui::pos2(x, rect.bottom()),
                egui::pos2(x + rect.height(), rect.top()),
            ],
            stroke,
        );
        x += gap;
    }
}

/// 容量条图例前那一小块颜色（设计稿 `.legend i`）。**颜色不是唯一线索**：后面一定跟着那一段的名字。
fn legend_swatch(ui: &mut egui::Ui, color: egui::Color32) {
    let layout = &Tokens::builtin().layout;
    let side = layout.legend_swatch;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, layout.legend_swatch_radius, color);
}

/// 规则块里一行的内边距（设计稿 `.rule` 的 8 × 12）。
fn block_margin() -> egui::Margin {
    egui::Margin::from(egui::vec2(step(2), step(1)))
}

/// 规则块里的一行（设计稿 `.rule`）：上面一道分隔线，行首一个序号圆，中间一段，右边一段。
///
/// **序号圆画在中间那一段的竖向中线上**（稿上 `.rule` 是 `align-items:center`）：横排按行高摆的话，中间只有
/// 一行字时圆比字低半截（第二段「没有手动例外」那一行就这么歪过）。所以圆先只占一格宽，等中间那一段摆完再画。
/// 这一行最矮是序号圆那么高，不是按钮那么高——行里没有按钮。
fn block_row(
    ui: &mut egui::Ui,
    badge: &str,
    middle: impl FnOnce(&mut egui::Ui),
    right: impl FnOnce(&mut egui::Ui),
) {
    ui.add(egui::Separator::default().spacing(0.0));
    egui::Frame::new()
        .inner_margin(block_margin())
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let side = Tokens::builtin().layout.rule_badge;
            ui.spacing_mut().interact_size.y = side;
            ui.horizontal(|ui| {
                let (slot, _) =
                    ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
                let middle = ui.vertical(middle).response.rect;
                ui.with_layout(Layout::right_to_left(Align::Center), right);
                badge_ui(ui, egui::pos2(slot.center().x, middle.center().y), badge);
            });
        });
}

/// 行首那个序号圆（设计稿 `.rule .rn`）：凹陷底、弱字色、等宽角标字，圆心在 `center`。
fn badge_ui(ui: &egui::Ui, center: egui::Pos2, text: &str) {
    let tokens = Tokens::builtin();
    let visuals = ui.visuals();
    ui.painter().circle_filled(
        center,
        tokens.layout.rule_badge / 2.0,
        visuals.extreme_bg_color,
    );
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::monospace(tokens.font.size_caption),
        visuals.weak_text_color(),
    );
}

/// 弹层里「会怎样」的一条（设计稿 `.impact li`）：行首一枚强调色圆点，后面一句半号字，`(字, 要不要强调)`
/// 一段段接起来（稿上 `<b>` 那几个字是强调字）。
///
/// 圆点多大、那一列多宽取令牌 `impact-dot` / `impact-column`；圆点对齐头一行的中线，字折行时不跟着往下挪。
fn impact_ui(ui: &mut egui::Ui, parts: &[(&str, bool)]) {
    let tokens = Tokens::builtin();
    let style = ui.style().clone();
    let mut job = egui::text::LayoutJob::default();
    for (text, strong) in parts {
        let rich = if *strong {
            font::strong(*text)
        } else {
            egui::RichText::new(*text)
        };
        rich.size(look::font_size(ui.ctx(), tokens.font.size_small_plus))
            .append_to(
                &mut job,
                &style,
                egui::FontSelection::Default,
                Align::Center,
            );
    }
    ui.horizontal_top(|ui| {
        let column = tokens.layout.impact_column;
        let width = (ui.available_width() - column - ui.spacing().item_spacing.x).max(0.0);
        let galley = egui::WidgetText::from(job).into_galley(
            ui,
            Some(egui::TextWrapMode::Wrap),
            width,
            egui::FontSelection::Default,
        );
        #[allow(clippy::cast_precision_loss)]
        let rows = galley.rows.len().max(1) as f32;
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(column, galley.size().y), egui::Sense::hover());
        let dot = tokens.layout.impact_dot;
        ui.painter().circle_filled(
            egui::pos2(
                rect.left() + dot / 2.0,
                rect.top() + galley.size().y / rows / 2.0,
            ),
            dot / 2.0,
            ui.visuals().selection.stroke.color,
        );
        ui.label(galley);
    });
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

/// 摊开之后那张步骤表。**虚拟化，而且一步都不截**；返回这一帧真的画了几行。
///
/// ## 为什么不再截在 2,000 步（挂账 `D158`）
///
/// 真机量级上一次同步动上万个文件（实测合成数据 **21,571 步**，`--bench-sublibrary`
/// 跑出来的；挂账 `D158` 记的 8,206 是票 `parking-3/17` 换合成数据形状之前那个数）。
/// 截断的代价不是「少看几行」——是「想确认第 5,000 步是什么就得转去命令行」，
/// 而**同步前必须看一遍它要做什么**是 ADR-0016 的硬要求，那一眼不该有一半落在界面之外。
///
/// 不截的代价是零：计划整份本来就在界面状态里（[`Screen::prepared`]），而
/// `TableBody::rows` 只调用视口里那几十行的闭包——**一帧画几行只跟视口有多高有关，
/// 与总步数无关**。返回的正是这个数，[`Screen::steps_drawn`] 把它交给实测与测试，
/// 于是这句话是被数出来的，不是被相信的。
///
/// `scroll_to` 把滚动位置强按到某个像素偏移，**只有量帧率与测试才给**。
pub fn steps_table(
    ui: &mut egui::Ui,
    plan: &romcat_core::sync::Plan,
    scroll_to: Option<f32>,
) -> usize {
    let shown = plan.steps.len();
    let mut drawn = 0;
    let mut builder = egui_extras::TableBuilder::new(ui)
        .striped(true)
        .resizable(true)
        .id_salt(format!("差量步骤 · {}", plan.sublibrary))
        .cell_layout(Layout::left_to_right(Align::Center))
        .column(egui_extras::Column::initial(60.0).at_least(50.0))
        .column(egui_extras::Column::initial(70.0).at_least(50.0))
        .column(egui_extras::Column::initial(90.0).at_least(70.0))
        .column(egui_extras::Column::remainder().at_least(160.0).clip(true));
    if let Some(offset) = scroll_to {
        builder = builder.vertical_scroll_offset(offset);
    }
    builder
        .header(22.0, |mut header| {
            for title in ["干什么", "类别", "容量", "目标上的路径"] {
                header.col(|ui| {
                    ui.label(font::strong(title));
                });
            }
        })
        .body(|body| {
            body.rows(ROW_HEIGHT, shown, |mut row| {
                let index = row.index();
                let Some(step) = plan.steps.get(index) else {
                    return;
                };
                drawn += 1;
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
    drawn
}

/// 「分不出第二份只读连接」那句话。
///
/// **不退到画帧那条线程上偷偷跑一趟**：那既会僵住窗口，又把真正的问题盖在一句
/// 「怎么卡了一下」底下。排差量预览与算一遍容量两处说的是同一件事，所以话也只写一处。
fn no_second_connection(什么活: &str, why: &CatalogError) -> String {
    format!(
        "{什么活}没开跑：读中立库要另开一份只读连接，这一下没开出来（{why}）。\n\
         先确认中立库那个文件还在、结构版本对得上，再按一次。"
    )
}

/// 一趟同步跑完之后摆在屏上的那句回执。
///
/// **中断、失败、主动停了，一样都不许吞。** 全成功的一趟与半数写失败的一趟若在界面上
/// 长得一样，那句「同步用了 X 秒」就是在骗人（命令行那一侧靠 `Outcome::render_text`
/// 把这几样印全）。
///
/// 耗时取的是**任务台记下的那个数**——与任务屏历史里那一行同一个来源，两处各掐一次表的话，
/// 同一趟活会在两屏上报出两个数。
fn sync_notice(outcome: &Outcome, elapsed: f64) -> String {
    let mut line = format!(
        // **说得出是哪一台**：这句话可能是几十分钟前排上去的那一趟交回来的，
        // 而那会儿摊开的多半是另一张卡了。
        "「{}」同步用了 {elapsed:.1} 秒：动了 {} 个文件（新增 {}、更新 {}、删除 {}）。",
        outcome.sublibrary,
        thousands(outcome.touched()),
        thousands(outcome.added.files),
        thousands(outcome.updated.files),
        thousands(outcome.deleted.files),
    );
    if outcome.interrupted {
        line.push_str(
            "\n⚠️ 这一趟部分完成：按停时目标上没留下写了一半的文件，清单记的是\
             停下那一刻目标上真实有什么，再跑一趟就接上。",
        );
    }
    if outcome.gave_up {
        line.push_str("\n⚠️ 连着失败太多次，主动停了：多半是卡拔了或者写满了。");
    }
    if !outcome.failures.is_empty() {
        line.push_str(&format!(
            "\n⚠️ 有 {} 步没做成：",
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
    line
}

/// 目标设备那个目录不在时那句话：为什么不行、去哪儿办；在就是 `None`。
///
/// **不是判断，是查一眼有没有**（ADR-0005 修订段「原料还没备齐」：盘上缺一样东西）。「在不在」
/// 照核心那一趟看目标时的口径（`sync::observe` 化不开这条路径就报目标不在位），话却是这一层
/// 自己说的：它要指向屏上的哪一处——卡上的「目标设置…」——而那是核心库不该知道的。
/// 排差量预览与同步两颗按钮说的是同一句。
///
/// **在、却不是目录**（那条路径上是一份文件）不归这里：那不是缺一样东西，是一件该去查的事
/// ——那一趟在「看一眼目标」真去列它时撞上（`sync::observe` 报「列不开」），照实记失败。
fn target_absent(root: &std::path::Path) -> Option<String> {
    (!root.exists()).then(|| {
        format!(
            "未连接：{}。插上读卡器再按；目标路径不对的话，按卡上「目标设置…」改。",
            romcat_core::path::display(root)
        )
    })
}

/// 任务台上那趟同步干的活：把计划落到目标上。
///
/// **它拿到的只有计划里那些步骤**——清单之外的路径连进来的门都没有（ADR-0015）。
/// 那份计划是**排它时**那一份：整份搬进了这个闭包，屏上那一份后来怎么变都够不着它。
fn run_sync(
    prepared: &Prepared,
    library_roots: Option<&romcat_core::catalog::Roots>,
    task: &Handle,
) -> Result<Outcome, Cutoff> {
    let sources = sync::Sources {
        library: &romcat_core::fs::RealFs,
        library_roots,
        // 真跑一律给真盘：那道可注入的接缝只为测试造得出的那两格而存在
        // （`sync::execute` 模块文档八）。
        target: &romcat_core::fs::RealFs,
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
        task,
    )
    // **同步被按停时不走这条**：它照旧返回 `Ok`，另报一句「停在半路」，
    // 好让那份**清单**留得下来（ADR-0015）。走到这儿的都是真出错了。
    .map_err(|error| Cutoff::failed(format!("目标写不了：{error}")))
}

/// **删减建议表**（设计稿 `trimHTML`）：超出多少，按体积排的每一项释放多少、累计多少，排除到哪一项
/// 就放得下，全排除也还差多少。没超限时什么都不画。返回这一帧按了「排除」的那个变体的键。
///
/// 只求选择集与排完差量预览两处摆的是同一段（[`Room`] 两处都抄自同步计划器），画法也就只写一处。
/// **四个数与两句判断全是核心说的**：释放与累计是 [`romcat_core::sublibrary::Trim`]，「放得下」
/// 「还差」是 [`Room::fits_after`] / [`Room::short_after_all`]，这儿只画（ADR-0005、ADR-0024）。
///
/// **只建议，绝不自动删减**（ADR-0016）：「排除」记成这个子库的一条手动例外，由调用方落库
/// （[`Screen::exclude`]）。
///
/// ## 画法照稿（`.trim`）
///
/// 一圈描边的框：头上一条 `lo-soft` 底（[`look::Tone::Bad`]）写超出多少；表每一行之间一道分隔线，
/// 格子内边距取令牌 `cell-padding`（稿子写的是 6，令牌里没有这一档，挂单 `Q854`），「排除」是小号按钮；
/// 放得下之后的那几行**整行**淡下去（按钮一起）；「排除到这一项就能放下」那一行垫 `hi-soft`
/// （[`look::Tone::Good`]）；全排除也不够时底下再一条 `lo-soft` 底说还差多少。
fn trim_ui(ui: &mut egui::Ui, room: &Room) -> Option<String> {
    let over = room.over_capacity?;
    let fits = room.fits_after();
    let visuals = ui.visuals().clone();
    let (超字, 超底) = look::tone_colors(look::Tone::Bad, &visuals);
    let (放下字, 放下底) = look::tone_colors(look::Tone::Good, &visuals);
    let tokens = Tokens::builtin();
    let [格上下, 格左右] = tokens.space.cell_padding;
    let 格 = egui::Margin::from(egui::vec2(格左右, 格上下));
    // 右边三栏各多宽：照这张表里最宽的那一格量（说明字号），按钮照小号按钮量。
    let 数字 = egui::FontId::proportional(tokens.font.size_small);
    let 量 = |ui: &egui::Ui, text: &str| {
        egui::WidgetText::from(text)
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                数字.clone(),
            )
            .size()
            .x
    };
    let 宽 = [
        room.trim_suggestions
            .iter()
            .map(|trim| 量(ui, &human_bytes(trim.bytes)))
            .fold(量(ui, "释放"), f32::max),
        room.trim_suggestions
            .iter()
            .map(|trim| 量(ui, &human_bytes(trim.cumulative)))
            .fold(量(ui, "累计"), f32::max),
        look::small_button_width(ui, "排除"),
    ];
    let 等宽 = |text: String| font::mono(text).size(tokens.font.size_small);
    // 释放与累计两栏照稿是界面字（设计稿 `.num`）；变体那一栏写的是键，照旧等宽。
    let 数目 = |text: String| egui::RichText::new(text).size(tokens.font.size_small);
    let mut clicked = None;
    egui::Frame::new()
        .stroke(visuals.widgets.noninteractive.bg_stroke)
        .corner_radius(visuals.window_corner_radius)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // 行与行之间不留缝：分隔线就是缝。
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new()
                .fill(超底)
                .corner_radius(egui::CornerRadius {
                    sw: 0,
                    se: 0,
                    ..visuals.window_corner_radius
                })
                .inner_margin(格)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            font::strong(format!("超出容量上限 {}", human_bytes(over)))
                                .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus))
                                .color(超字),
                        );
                        ui.label(
                            egui::RichText::new(
                                "按大小列出可以排除的变体；排除会记为这个子库的手动例外，不会自动删减。",
                            )
                            .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus))
                            .weak(),
                        );
                    });
                });
            look::divider(ui);
            trim_row(
                ui,
                None,
                宽,
                |ui| {
                    look::help(ui, "变体");
                },
                |ui| {
                    look::help(ui, "释放");
                },
                |ui| {
                    look::help(ui, "累计");
                },
                |_| {},
            );
            for (at, trim) in room.trim_suggestions.iter().enumerate() {
                look::divider(ui);
                // 已经放得下之后的那几项**整行**淡下去（按钮一起）：再往下排除就多砍了。
                let faded = fits.is_some_and(|fits| at > fits);
                ui.scope(|ui| {
                    if faded {
                        ui.multiply_opacity(visuals.disabled_alpha);
                    }
                    trim_row(
                        ui,
                        None,
                        宽,
                        |ui| {
                            ui.add(egui::Label::new(等宽(trim.variant.clone())).truncate());
                        },
                        |ui| {
                            ui.label(数目(human_bytes(trim.bytes)));
                        },
                        |ui| {
                            ui.label(数目(human_bytes(trim.cumulative)));
                        },
                        |ui| {
                            let 按了 = look::small_buttons(ui, |ui| {
                                ui.button("排除")
                                    .on_hover_text(
                                        "记成这个子库的一条排除例外：规则选中了也不带。优先于规则、\
                                         永久记住；盘上的文件一个都不动。",
                                    )
                                    .clicked()
                            });
                            if 按了 {
                                clicked = Some(trim.variant.clone());
                            }
                        },
                    );
                });
                if fits == Some(at) {
                    look::divider(ui);
                    trim_row(
                        ui,
                        Some(放下底),
                        宽,
                        |ui| {
                            ui.colored_label(放下字, "排除到这一项就能放下").on_hover_text(
                                "照计划器记的账估的：作品共用的媒体记在头一个变体名下，同一作品还有\
                                 别的变体留着时，排除它未必省下那几份。按下「排除」之后会当场重算。",
                            );
                        },
                        |_| {},
                        |_| {},
                        |_| {},
                    );
                }
            }
            if let Some(short) = room.short_after_all() {
                look::divider(ui);
                egui::Frame::new()
                    .fill(超底)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        ..visuals.window_corner_radius
                    })
                    .inner_margin(格)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            egui::RichText::new(format!(
                                "全部排除也还差 {}，需要移除规则或换一张更大的卡。",
                                human_bytes(short)
                            ))
                            .size(look::font_size(ui.ctx(), Tokens::builtin().font.size_small_plus)),
                        );
                    });
            }
        });
    clicked
}

/// 删减建议表里的一行：左边一栏占满余下的宽（放不下截断），右边三栏定宽（`宽`：释放、累计、按钮），
/// 数字右对齐。`底` 给了就垫一层底色。
///
/// 格子内边距取令牌 `cell-padding`；一行至少一颗小号按钮高（令牌 `button-small-height`），
/// 于是有按钮的行与没按钮的行一样高——稿子里按钮在格子里，行高不等于默认按钮高。
fn trim_row(
    ui: &mut egui::Ui,
    底: Option<egui::Color32>,
    宽: [f32; 3],
    左: impl FnOnce(&mut egui::Ui),
    释放: impl FnOnce(&mut egui::Ui),
    累计: impl FnOnce(&mut egui::Ui),
    按钮: impl FnOnce(&mut egui::Ui),
) {
    let tokens = Tokens::builtin();
    let [格上下, 格左右] = tokens.space.cell_padding;
    let 行高 = tokens.layout.button_small_height;
    egui::Frame::new()
        .fill(底.unwrap_or(egui::Color32::TRANSPARENT))
        .inner_margin(egui::Margin::from(egui::vec2(格左右, 格上下)))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let 缝 = ui.spacing().item_spacing.x;
                let 左宽 = (ui.available_width() - 宽.iter().sum::<f32>() - 3.0 * 缝).max(0.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(左宽, 行高),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_size(egui::vec2(左宽, 行高));
                        左(ui);
                    },
                );
                for (多宽, 这一格) in [
                    (宽[0], Box::new(释放) as Box<dyn FnOnce(&mut egui::Ui)>),
                    (宽[1], Box::new(累计)),
                    (宽[2], Box::new(按钮)),
                ] {
                    ui.allocate_ui_with_layout(
                        egui::vec2(多宽, 行高),
                        Layout::right_to_left(Align::Center),
                        |ui| {
                            ui.set_min_size(egui::vec2(多宽, 行高));
                            这一格(ui);
                        },
                    );
                }
            });
        });
}
