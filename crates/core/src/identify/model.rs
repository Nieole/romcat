//! **最后一档：模型推断兜底。**
//!
//! 前面每一层都还抓着点什么：CRC-32 抓着字节、序列号与卡带头抓着内容里明写的标识、
//! 文件名那一层抓着一份**离线的中文条目表**——撞上了就是撞上了，撞不上就是撞不上，
//! 一个字都不是编的。到这一层，那几样全落空了：手上只剩一个像
//! `033.動作：掃地雷.zip`、`ACGHH-0113-TP.7z`、`my_theme0.tar.zst` 这样的名字。
//!
//! 于是这一层拿这些名字、连同**已经解析出来的头部字段**与**同目录的别的东西**，
//! 打包问模型：这几条各自最可能是哪个游戏。
//!
//! ## 三条硬约束，都在代码里，不在文档里
//!
//! ### 一、只处理前面各层**全部落空**的变体
//!
//! 判据是 `candidates.is_empty()`——**一条候选都没有**，比文件名那一层的
//! 「没有自动通过的候选」严一档。已经有候选可看的变体不重复花钱：它们在**待确认队列**
//! 里已经有东西可裁了，而这一层的每一条都要付钱。**跳过**的那些（补丁、自制应用）
//! 更是一条都不问——词表里写着「拿补丁去撞 DAT 必然落空，会一路掉到模型推断白烧一遍」。
//!
//! ### 二、批量打包提问
//!
//! [`body_of`] 一个请求里装 [`Limits::batch`] 条（默认 20），让模型对每一条给出**排好序**
//! 的几个猜测。逐条问在几千条的规模上，是几千个请求、几千次往返、以及几千遍同一段
//! 系统提示的输入 token。**这件事只能对着请求正文验**：发了几个请求分不出打包没打包，
//! 一个正文里装着几条才分得出（[`CannedFetcher::posted`](crate::dat::CannedFetcher::posted)）。
//!
//! ### 三、**永不自动通过**（ADR-0002）
//!
//! [`candidates_of`] 产出的候选 `accepted` **恒为假**、置信度**恒为低置信**，
//! 而且**依据的第一句就写着这是模型推断的**。理由 ADR-0002 已经说尽了：模型会自信地
//! 编，而编出来的东西一旦与真实数据混在一起就再也分不开。这一层比文件名那一层还低
//! 一档——那一层至少撞上了一份真实存在的条目表，这一层连那个都没有。
//!
//! ## 它**不改变结论的状态**
//!
//! 这是与文件名那一层**故意不同**的一处。那一层的候选会把变体从「未命中」抬成
//! 「命中」（挂账 D127 记着这件事）；这一层不会——变体照旧是「未命中」或「无判据」，
//! 只是多了几条候选可裁。两个理由：
//!
//! 1. **「无判据」那一列的理由是真事实**（`rar 容器这一层还穿不透`、
//!    `zst 穿不透，而这一趟没为它付全量解压的代价`、`元数据读不到`）。真机上残渣里
//!    有 3,237 个是这一档。让一句猜测把这些理由覆盖掉，是拿库体检最有用的产出去换一个
//!    好看的百分比。
//! 2. 命中率里混进模型编的东西，这份报告就再也不能回答「这个库被**认出来**多少」。
//!
//! **进不进待确认队列与状态无关**：队列的判据是「没有自动通过的候选、而且不是跳过」
//! （`triage::in_queue`），所以这一层的候选照样进队列，一条不漏。
//!
//! ## 花钱这件事看得见、设得上限
//!
//! - [`Plan`] 在**发第一个请求之前**就算得出：问多少个变体、打成多少个请求、
//!   输入多少 token、花费的**下界与上界**。上界是硬的——一个请求的输出不可能超过
//!   `max_tokens`。
//! - [`Limits::budget`] 卡请求个数，[`Limits::spend_cap_micros`] 卡钱。
//! - 价目表是**数据不是代码**（`pricing.toml`）。**表里没有这个模型就整层不启动**——
//!   不知道 token 值多少钱，「花费上限」就是一句空话。
//!
//! ## 问过的答案落进中立库，重跑不再问一遍
//!
//! 这一层是识别管线里**唯一花钱**的一层，所以它必须只花一次。答案按
//! `(变体, 提问指纹)` 存进 `model_answer`（[`Answers`]），下一趟识别直接从缓存里
//! 把候选重建出来，一个请求都不发。**提问指纹里有模型名、力度、提示词版本与问题正文**
//! ——换一个模型或改一版提示词，指纹就变，于是自动重问；而只是重跑一遍识别，
//! 指纹一模一样，于是白拿（与 `zh::Tuning::fingerprint` 进输入指纹同源）。

pub mod net;

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::catalog::VariantRow;
use crate::catalog::identify::{Candidate, Confidence};
use crate::dat::Convention;

pub use net::{
    Credentials, ENV_KEYS, Failure, Halt, HaltKind, Inference, Usage, Verdict, classify,
};

/// 这一层在候选表的「数据源」那一栏里叫什么。
///
/// 与 `No-Intro`、`TOSEC`、`中文离线源` 平级地出现在报告的源表里——**这一层贡献了
/// 多少条候选、其中自动通过几条（永远是 0），要一眼看得出来**。
pub const SOURCE: &str = "模型推断";

/// 默认问哪个模型。
pub const DEFAULT_MODEL: &str = "claude-opus-5";

/// 落点。**只有这一个**，而且只 POST。
pub const ENDPOINT: &str = "https://api.anthropic.com/v1/messages";

/// API 版本头。
pub const API_VERSION: &str = "2023-06-01";

/// 提示词的版本号，进**提问指纹**。
///
/// **这个文件里凡是会出现在提问里的字，改了就把它加一**：[`SYSTEM`]、
/// [`Question::render`]、[`head_fields`] 拼的那几句、[`schema`] 的键名。
/// 不加的话，改完重跑会从缓存里把**旧提示词问出来的答案**原样端上来，
/// 而报告会显示「这一趟一个请求都没发」。
///
/// 反过来也成立，而且它才是这条纪律真正贵的那一半：**加一就是让几千条付过钱的答案
/// 当场作废、全部重问**。真机上那是三十几美元。所以提问里的措辞全部收在这一个文件里
/// ——散在两处的话，另一处改一个字，两种代价里一定有一种要付，而没有任何一句文档
/// 会提醒改的人。
pub const PROMPT_VERSION: &str = "1";

/// 这一层能出的置信度上限：**低置信**，没有例外。
///
/// 文件名那一层的天花板是中置信（它至少撞上了一份真实存在的条目表）。这一层连那个
/// 都没有，所以再往下一档（ADR-0002：低置信与模型推断结论进待确认队列）。
pub const CEILING: Confidence = Confidence::Low;

/// 系统提示。**它进提问指纹**（见 [`PROMPT_VERSION`]）。
const SYSTEM: &str = "\
你在帮一个 ROM 资源库做**最后一档**识别兜底。前面每一层（精确哈希、光盘序列号、\
卡带内部头、文件名规则加中文离线条目表）对下面这批东西全部落空了，手上只剩名字、\
已经解析出来的头部字段，以及同目录的别的文件。

请对**每一条**给出**按可能性排好序**的猜测：它最可能是哪一部**作品**。

规矩：

1. **说不出就给空的猜测列表。** 这批东西里混着模拟器、存档、主题、DLC 碎片、\
   字库包、工具——它们压根不是任何一部作品。给它们编一个名字，比留空糟得多：\
   这些结论会进人工队列，一条编出来的名字要浪费一个人的时间去否掉它。
2. **标题写你认为的那部作品的通行名字**（中文名优先，没有中文名就写原名），\
   不要把文件名原样抄回来。
3. **依据要短，而且必须说出你是看着名字里的哪一部分判的。** 事后复核靠它。
4. 每一条最多给指定条数的猜测，宁少勿多。
5. 只输出规定格式的 JSON，不要有别的话。";

/// 提问要按什么形状答回来。
///
/// 用**结构化输出**（`output_config.format`）而不是「请只输出 JSON」加正则去捞：
/// 后者要么捞不到、要么捞错，而这一层每问一次都花钱，重问一遍是真的再付一遍。
fn schema(guesses: usize) -> Value {
    json!({
        "type": "object",
        "properties": {
            "答案": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "编号": { "type": "integer" },
                        "猜测": {
                            "type": "array",
                            "maxItems": guesses,
                            "items": {
                                "type": "object",
                                "properties": {
                                    "标题": { "type": "string" },
                                    "平台": { "type": "string" },
                                    "依据": { "type": "string" }
                                },
                                "required": ["标题", "依据"],
                                "additionalProperties": false
                            }
                        }
                    },
                    "required": ["编号", "猜测"],
                    "additionalProperties": false
                }
            }
        },
        "required": ["答案"],
        "additionalProperties": false
    })
}

// ════════════════════════════════════════════════════════════════════════
// 价目表
// ════════════════════════════════════════════════════════════════════════

/// 一个模型的价钱，**每百万 token 多少美分**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Price {
    /// 输入。
    pub input_per_mtok: u64,
    /// 输出。
    pub output_per_mtok: u64,
}

impl Price {
    /// 这么多 token 值多少**微美元**（百万分之一美元）。
    ///
    /// 用整数一路算到底：一趟几百个请求要把花费一笔一笔加起来，而这个和正好是
    /// 「有没有超上限」那个判断的输入——浮点的累加误差落在这里就是上限失效。
    #[must_use]
    pub fn cost_micros(&self, input_tokens: u64, output_tokens: u64) -> u64 {
        // 单位一步一步走清楚，别在脑子里跳：
        //   token × (美分 / 百万 token) = 美分 × 一百万
        //   1 美分 = 10,000 微美元
        //   ⇒ 微美元 = token × 每百万美分 × 10,000 / 1,000,000 = 那个乘积 / 100
        // **两项先加再除**：分开除会各截断一次，几百笔累下来正好落在
        // 「有没有超上限」那个判断上。
        let scaled = input_tokens.saturating_mul(self.input_per_mtok)
            + output_tokens.saturating_mul(self.output_per_mtok);
        scaled / 100
    }
}

/// 价目表：哪个模型多少钱，以及这份表是哪天照哪儿抄的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pricing {
    checked: String,
    cite: String,
    prices: BTreeMap<String, Price>,
}

/// 价目表读不了。
#[derive(Debug, thiserror::Error)]
pub enum PricingError {
    /// TOML 解析不了。
    #[error("价目表 {path} 解析不了：{source}")]
    Parse {
        /// 哪一份。
        path: String,
        /// 为什么。
        source: toml::de::Error,
    },
    /// 读不到文件。
    #[error("价目表 {path} 读不了：{source}")]
    Io {
        /// 哪一份。
        path: String,
        /// 为什么。
        source: std::io::Error,
    },
}

/// TOML 里那份表的样子。
#[derive(serde::Deserialize)]
struct PricingFile {
    #[serde(rename = "核实日期")]
    checked: String,
    #[serde(rename = "出处")]
    cite: String,
    #[serde(rename = "模型")]
    models: Vec<PricedModel>,
}

#[derive(serde::Deserialize)]
struct PricedModel {
    #[serde(rename = "名字")]
    name: String,
    #[serde(rename = "输入每百万")]
    input: u64,
    #[serde(rename = "输出每百万")]
    output: u64,
}

impl Pricing {
    /// 内置那一份。
    ///
    /// # Panics
    /// 内置的那份 TOML 解析不了时——那是编译期就该发现的事，测试里钉着。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse(Self::BUILTIN, "内置价目表").expect("内置价目表必须解析得了")
    }

    /// 内置那份 TOML 的原文，`--model-dump-pricing` 导出的就是它。
    pub const BUILTIN: &'static str = include_str!("pricing.toml");

    /// 读一份 TOML。
    ///
    /// # Errors
    /// 解析不了时返回错误。
    pub fn parse(text: &str, path: &str) -> Result<Self, PricingError> {
        let file: PricingFile = toml::from_str(text).map_err(|source| PricingError::Parse {
            path: path.to_string(),
            source,
        })?;
        Ok(Self {
            checked: file.checked,
            cite: file.cite,
            prices: file
                .models
                .into_iter()
                .map(|one| {
                    (
                        one.name,
                        Price {
                            input_per_mtok: one.input,
                            output_per_mtok: one.output,
                        },
                    )
                })
                .collect(),
        })
    }

    /// 从文件读一份。
    ///
    /// # Errors
    /// 读不到或解析不了时返回错误。
    pub fn load(path: &std::path::Path) -> Result<Self, PricingError> {
        let text = std::fs::read_to_string(path).map_err(|source| PricingError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text, &path.display().to_string())
    }

    /// 这个模型多少钱。**表里没有就是 `None`，而 `None` 让整层不启动**。
    #[must_use]
    pub fn price(&self, model: &str) -> Option<Price> {
        self.prices.get(model).copied()
    }

    /// 表里有哪些模型，按名字排。报错时把它们列出来，好让人知道该写哪一个。
    #[must_use]
    pub fn known(&self) -> Vec<&str> {
        self.prices.keys().map(String::as_str).collect()
    }

    /// 这份表哪天核实的。
    #[must_use]
    pub fn checked(&self) -> &str {
        &self.checked
    }

    /// 这份表照哪儿抄的。
    #[must_use]
    pub fn cite(&self) -> &str {
        &self.cite
    }
}

// ════════════════════════════════════════════════════════════════════════
// 上限
// ════════════════════════════════════════════════════════════════════════

/// 这一层的每一道闸。**每一个都有默认值，而每一个默认值都往「少花钱」那边偏**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    /// 一个请求里装几条。
    pub batch: usize,
    /// 每一条最多要几个猜测。
    pub guesses: usize,
    /// 这一趟最多发几个请求。
    pub budget: u64,
    /// 这一趟最多花多少**微美元**。
    ///
    /// 卡的是**实际花费**：每收到一个响应就按 `usage` 里的真数字加一笔，加到头就停。
    /// 因此**最多超出一个请求的顶**（[`Plan::ceiling_per_request_micros`]），
    /// 而那个顶在计划里印得出来。
    pub spend_cap_micros: u64,
    /// 一个响应最多多少输出 token。**它同时是花费上界的那一半**。
    pub max_output_tokens: u64,
    /// 力度（`output_config.effort`）。
    pub effort: String,
    /// 两次请求之间至少隔多久。
    pub interval: std::time::Duration,
    /// 撞上「慢一点」之后等多久再试。可调只是为了让那条分支测得动。
    pub backoff: std::time::Duration,
}

/// 一个请求里装几条的默认值。
///
/// 20 是拍的，但拍得有理由：再小，那段系统提示的输入 token 就摊不薄；再大，一个响应
/// 里要同时管住的编号太多，而**一个响应答坏了就是这一批全丢**（重问要再付一遍）。
pub const DEFAULT_BATCH: usize = 20;
/// 每条最多要几个猜测。
pub const DEFAULT_GUESSES: usize = 3;
/// 一趟自设的请求上限。
pub const DEFAULT_BUDGET: u64 = 40;
/// 一趟自设的花费上限：**5 美元**。
pub const DEFAULT_SPEND_CAP_MICROS: u64 = 5_000_000;
/// 一个响应最多多少输出 token。
pub const DEFAULT_MAX_OUTPUT: u64 = 8_000;
/// 默认力度。
///
/// 不是 `high`（那是 API 的默认）：这一层的产出**反正要人裁决**，而它要跑几百个请求。
/// 觉得猜得不够准就 `--model-effort high` 再跑一趟——提问指纹里有力度，
/// **换一档会自动重问**，不必先把缓存清掉。
pub const DEFAULT_EFFORT: &str = "medium";
/// 两次请求之间的默认间隔。
pub const DEFAULT_INTERVAL: std::time::Duration = std::time::Duration::from_millis(1_000);
/// 撞上「慢一点」之后等多久。
pub const BACKOFF: std::time::Duration = std::time::Duration::from_secs(5);
/// 退避最多几次，到头就停。
pub const MAX_BACKOFF_TRIES: u32 = 3;
/// 连着几次传输失败就认定网断了。
pub const MAX_CONSECUTIVE_FAILURES: u32 = 5;

impl Default for Limits {
    fn default() -> Self {
        Self {
            batch: DEFAULT_BATCH,
            guesses: DEFAULT_GUESSES,
            budget: DEFAULT_BUDGET,
            spend_cap_micros: DEFAULT_SPEND_CAP_MICROS,
            max_output_tokens: DEFAULT_MAX_OUTPUT,
            effort: DEFAULT_EFFORT.to_string(),
            interval: DEFAULT_INTERVAL,
            backoff: BACKOFF,
        }
    }
}

impl Limits {
    /// 这几个参数里**会改变模型答什么**的那几个，折成一串进提问指纹。
    ///
    /// `batch` / `budget` / 花费上限 / 间隔**不在里面**：它们决定「问多少、问多快」，
    /// 不决定「这一条问出来是什么」。把它们编进去，改一次打包大小就要把几千条答案
    /// 全部作废重问一遍——那正好是这一层最不该发生的事。
    #[must_use]
    pub fn ask_fingerprint(&self) -> String {
        format!(
            "{}|{}|{}",
            self.guesses, self.effort, self.max_output_tokens
        )
    }
}

// ════════════════════════════════════════════════════════════════════════
// 一条提问
// ════════════════════════════════════════════════════════════════════════

/// 一个变体要问的那一条，连它的**上下文**。
///
/// 上下文是这一层唯一的弹药，票 12 点名了三样：文件名、已经解析出来的头部字段、
/// 同目录的别的文件。少给一样，模型就只能凭一个名字硬猜。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// 哪个变体。它是答案回来之后对得上号的那把钥匙。
    pub variant_key: String,
    /// 变体的主文件键，候选要挂在它上面。
    pub main_key: String,
    /// **内容说的**平台（卡带内部头读出来的）；读不出来就是 `None`。
    pub platform: Option<String>,
    /// **目录声明的**平台（ADR-0011 的强先验）。
    ///
    /// 两个都给，而且分开给：真库里 `psp/` 目录下混着整包的 FC / GB / SFC ROM，
    /// 只给一个的话模型没法知道这两句话在打架。
    pub declared: Option<String>,
    /// 拿得到的名字，连出处。
    pub names: Vec<(String, &'static str)>,
    /// 容器里的内部条目名。
    pub inner: Vec<String>,
    /// 同一个目录下**别的变体**的名字。
    pub siblings: Vec<String>,
    /// 已经解析出来的头部字段：光盘序列号、卡带游戏码、内部标题。
    pub facts: Vec<String>,
    /// 这个变体多大。
    pub bytes: u64,
}

impl Question {
    /// 把一条渲染成给模型看的那一段。`id` 是**这一批里**的编号，从 1 起。
    ///
    /// 编号按批次算而不是全局算，是有意的：于是每一批的编号都是 `1..=n`，
    /// 答案与问题对得上号这件事只依赖**这一批**，一批答坏了不会连累别批。
    #[must_use]
    pub fn render(&self, id: usize) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(out, "### 第 {id} 条");
        for (text, from) in &self.names {
            let _ = writeln!(out, "- 名字（{from}）：{text}");
        }
        match (&self.platform, &self.declared) {
            (Some(read), Some(dir)) if read != dir => {
                let _ = writeln!(
                    out,
                    "- 平台：内部头读出来是 {read}，而它躺在 {dir} 目录下（两句话打架，以内容为准）"
                );
            }
            (Some(read), _) => {
                let _ = writeln!(out, "- 平台：{read}（从内部头读出来的）");
            }
            (None, Some(dir)) => {
                let _ = writeln!(out, "- 平台：{dir}（目录声明的，没读出内部头）");
            }
            (None, None) => {
                let _ = writeln!(out, "- 平台：不知道");
            }
        }
        for fact in &self.facts {
            let _ = writeln!(out, "- 头部字段：{fact}");
        }
        if !self.inner.is_empty() {
            let _ = writeln!(out, "- 包里装着：{}", self.inner.join("、"));
        }
        if !self.siblings.is_empty() {
            let _ = writeln!(out, "- 同目录还有：{}", self.siblings.join("、"));
        }
        let _ = writeln!(out, "- 大小：{} 字节", self.bytes);
        out
    }

    /// **提问指纹**：同一个问题问同一个模型，指纹一样，于是不再问第二遍。
    ///
    /// 里面有：提示词版本、模型名、[`Limits::ask_fingerprint`]，以及这个变体的
    /// [身份](Self::identity)。任何一样变了都会重问；**只是重跑一遍识别，一样都没变**。
    #[must_use]
    pub fn ask(&self, model: &str, limits: &Limits) -> String {
        crate::scrape::fingerprint(&[
            PROMPT_VERSION,
            model,
            &limits.ask_fingerprint(),
            &self.identity(),
        ])
    }

    /// 这条问题**问的是谁**，折成一串。
    ///
    /// 它不是 [`render`](Self::render) 的全文，差在一处：**同目录别的文件的名字不算数**。
    ///
    /// 那一栏是上下文里唯一**会因为别人而变**的东西——往同一个目录里再放一个文件，
    /// 这个目录下每一条残渣的全文都会变。若拿全文当指纹，那一放就让它们**整批重问**，
    /// 而问的还是同样那几个东西。这一层每问一次都真的付一次钱，所以指纹只认
    /// 「这个变体自己是什么」：它的键、它自己的几个名字、从字节里读出来的头部字段、
    /// 平台，以及它有多大。
    ///
    /// 代价说清楚：**同目录的东西变了不会重问**。那是有意的——上下文变了不等于
    /// 答案该变，而真想重问，把 [`PROMPT_VERSION`] 加一或者换一档力度就是了。
    #[must_use]
    pub fn identity(&self) -> String {
        let mut out = String::new();
        out.push_str(&self.variant_key);
        for (text, from) in &self.names {
            out.push_str("\n名字|");
            out.push_str(from);
            out.push('|');
            out.push_str(text);
        }
        for fact in &self.facts {
            out.push_str("\n头部|");
            out.push_str(fact);
        }
        for name in &self.inner {
            out.push_str("\n包内|");
            out.push_str(name);
        }
        out.push_str("\n平台|");
        out.push_str(self.platform.as_deref().unwrap_or(""));
        out.push('|');
        out.push_str(self.declared.as_deref().unwrap_or(""));
        out.push_str(&format!("\n字节|{}", self.bytes));
        out
    }
}

/// **已经解析出来的头部字段**，写成给模型看的几句（票 12 点名要的那一样）。
///
/// 它们是这一层手上唯一来自**字节**的东西：光盘序列号与卡带游戏码撞不上 DAT
/// （那几个平台没有弹药，见 ADR-0014 的四修订），但 `SLPS-02330`、`ULJM05800`
/// 这样一串本身就说明了不少事——而一个人也正是这么认的。
///
/// **它长在这个文件里，不长在 `identify.rs` 里**：这几句是提示词的一部分，
/// 而 [`PROMPT_VERSION`] 那条纪律（改了提示词就把版本加一）盖得住的只有这个文件。
/// 措辞散在两处的话，在另一处改一个字就会让几千条**付过钱**的缓存当场作废，
/// 而没有任何一句文档会提醒改的人。
#[must_use]
pub fn head_fields<'a>(
    disc: impl Iterator<Item = &'a super::disc::Facts>,
    cart: impl Iterator<Item = &'a super::cart::Facts>,
) -> Vec<String> {
    let mut out = Vec::new();
    for facts in disc {
        for id in &facts.ids {
            out.push(format!("光盘标识 {}（{}）", id.shown, id.from));
            if let Some(title) = &id.title {
                out.push(format!("盘里写的标题「{title}」"));
            }
        }
    }
    for facts in cart {
        for id in &facts.ids {
            out.push(format!("卡带游戏码 {}（{}）", id.shown, id.from));
        }
        if let Some(title) = &facts.title {
            out.push(format!("卡带头里写的标题「{title}」"));
        }
        if let Some(maker) = &facts.maker {
            out.push(format!("发行商代码 {maker}"));
        }
        if let Some(region) = &facts.region {
            out.push(format!("地区码 {region}"));
        }
    }
    // 同一句不说两遍（一个变体里几份内容读出同一个游戏码是常态）。
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    out.retain(|line| seen.insert(line.clone()));
    out.truncate(CONTEXT_LIMIT * 2);
    out
}

/// 上下文里每一样最多列几条。
///
/// **上限不是省钱，是防一条把整批挤爆**：真库里一个目录底下躺着三千个 zip 是常态，
/// 把兄弟名字全列上就是几十万 token 一个请求。
pub const CONTEXT_LIMIT: usize = 8;

/// 一条**还没问出去**的提问：问题本身，连它的[提问指纹](Question::ask)。
///
/// 捏成一个类型而不是一个二元组：这两样要一起穿过「攒起来」「打包」「落库」三处，
/// 而二元组里写反了顺序编译器一个字都不会说——两个都是 `String` 打头的东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asking {
    /// 问什么。
    pub question: Question,
    /// **提问指纹**。答案按它落库，下一趟按它认出「这条问过了」。
    pub ask: String,
}

/// 模型给的一个猜测。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Guess {
    /// 它猜这是哪一部**作品**。
    pub title: String,
    /// 它猜是哪个平台；说不出就是 `None`。
    pub platform: Option<String>,
    /// 它凭什么这么猜。**这条候选的依据里要有它**——没有它，事后没人复核得了。
    pub basis: String,
}

/// 模型对一个变体的答复。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answer {
    /// 按可能性排好序的猜测。**空的是一条正当的答复**——那批模拟器、存档与主题包
    /// 本来就不是游戏，编一个名字比留空糟得多。
    pub guesses: Vec<Guess>,
}

impl Answer {
    /// 存进中立库的那一串 JSON。原样留着，事后复核靠它。
    #[must_use]
    pub fn to_json(&self) -> String {
        let guesses: Vec<Value> = self
            .guesses
            .iter()
            .map(|guess| {
                json!({
                    "标题": guess.title,
                    "平台": guess.platform,
                    "依据": guess.basis,
                })
            })
            .collect();
        json!({ "猜测": guesses }).to_string()
    }

    /// 从中立库里那一串 JSON 读回来。读不出来当作**没有答案**——
    /// 那会让它重新排进队里再问一次，而不是端上来一条空的。
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        Some(Self {
            guesses: guesses_of(value.get("猜测")?),
        })
    }
}

// ════════════════════════════════════════════════════════════════════════
// 请求正文与响应解析：两个纯函数
// ════════════════════════════════════════════════════════════════════════

/// 一批问题打成一个请求正文。
///
/// **凭据不在这里**——它在请求头上（[`net::Inference`]）。单元测试据此断言
/// 「正文里一个字的凭据都没有」。
#[must_use]
pub fn body_of(model: &str, limits: &Limits, batch: &[Question]) -> Value {
    let mut prompt = String::from("下面这批东西前面每一层都认不出来。请逐条给出排好序的猜测。\n\n");
    for (at, question) in batch.iter().enumerate() {
        prompt.push_str(&question.render(at + 1));
        prompt.push('\n');
    }
    prompt.push_str(&format!(
        "一共 {} 条，编号 1 到 {}。每条最多 {} 个猜测，说不出就给空列表。",
        batch.len(),
        batch.len(),
        limits.guesses
    ));
    json!({
        "model": model,
        "max_tokens": limits.max_output_tokens,
        "system": SYSTEM,
        "messages": [{ "role": "user", "content": prompt }],
        "output_config": {
            "effort": limits.effort,
            "format": { "type": "json_schema", "schema": schema(limits.guesses) }
        }
    })
}

/// 从响应里把答案拆出来，按**这一批的下标**归位。
///
/// 认不出的形状是**答不上来**而不是**答错**：编号对不上这一批的一律丢掉，
/// 少答的那几条留空（于是它们下一趟还会被问一遍），多答的那几条不知道该挂给谁。
/// 这一条与票 14 那句「认不出的形状是采不到而不是采错」同源。
#[must_use]
pub fn parse_answers(response: &Value, batch_len: usize) -> BTreeMap<usize, Answer> {
    let mut out = BTreeMap::new();
    let Some(blocks) = response.get("content").and_then(Value::as_array) else {
        return out;
    };
    for block in blocks {
        // 结构化输出仍然落在 `text` 块里；`thinking` 块要跳过（它可能是空的，
        // 也可能是一段摘要，两种都不该拿去解析）。
        if block.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = block.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Ok(parsed) = serde_json::from_str::<Value>(text) else {
            continue;
        };
        let Some(rows) = parsed.get("答案").and_then(Value::as_array) else {
            continue;
        };
        for row in rows {
            let Some(id) = row.get("编号").and_then(Value::as_i64) else {
                continue;
            };
            let Ok(id) = usize::try_from(id) else {
                continue;
            };
            // 编号是**这一批里**的 1..=n。越界的一律丢——挂错变体比不挂糟得多。
            if id == 0 || id > batch_len {
                continue;
            }
            let guesses = row.get("猜测").map(guesses_of).unwrap_or_default();
            out.insert(id - 1, Answer { guesses });
        }
    }
    out
}

fn guesses_of(value: &Value) -> Vec<Guess> {
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let title = row.get("标题").and_then(Value::as_str)?.trim();
            if title.is_empty() {
                return None;
            }
            Some(Guess {
                title: title.to_string(),
                platform: row
                    .get("平台")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|text| !text.is_empty())
                    .map(ToString::to_string),
                basis: row
                    .get("依据")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
            })
        })
        .collect()
}

// ════════════════════════════════════════════════════════════════════════
// 折成候选
// ════════════════════════════════════════════════════════════════════════

/// 把一条答复折成**候选**。
///
/// 每一条的 `accepted` 恒为假、置信度恒为 [`CEILING`]（低置信），依据的**第一句**
/// 就写着这是模型推断的。ADR-0002 定死的那一条在这里，不在别处。
#[must_use]
pub fn candidates_of(
    variant: &VariantRow,
    question: &Question,
    answer: &Answer,
    model: &str,
) -> Vec<Candidate> {
    let total = answer.guesses.len();
    answer
        .guesses
        .iter()
        .enumerate()
        .map(|(at, guess)| {
            let mut evidence = format!(
                "**这一条是模型推断的**（{model}）：在同一批问题里，它把这个变体的第 {} 个猜测\
                 （一共 {total} 个）给了「{}」，依据「{}」。",
                at + 1,
                guess.title,
                if guess.basis.is_empty() {
                    "（没说）"
                } else {
                    &guess.basis
                }
            );
            evidence.push_str(&format!(
                "提问只给了它{}——**一个字节的内容都没有看过**，也没有撞任何数据库。",
                asked_with(question)
            ));
            evidence.push_str(
                "前面每一层（精确哈希、光盘序列号、卡带内部头、文件名规则加中文离线条目表）\
                 对这个变体全部落空，才轮到这一层。**它永不自动通过**，一律进待确认队列\
                 （ADR-0002）。",
            );
            Candidate {
                member_key: variant.main_key.clone(),
                inner: String::new(),
                // 没有例外：这一层连一份真实存在的条目表都没撞上。
                confidence: CEILING,
                // **永不自动通过。**
                accepted: false,
                source: SOURCE.to_string(),
                // 这一列在别的层里是「哪一份 DAT」。这一层没有数据库，写**是哪个模型说的**
                // ——报告的源表按它分组时，换过模型这件事就看得见。
                dat: model.to_string(),
                platform: guess
                    .platform
                    .clone()
                    .or_else(|| question.platform.clone())
                    .or_else(|| variant.platform.clone())
                    .unwrap_or_default(),
                game: guess.title.clone(),
                // **留空**。别的层里这一列是「DAT 里那条 `<rom>` 记录的名字」；模型没有
                // 指向任何一条记录，编一个文件名顶上去，事后复核的人会以为真有那么一条
                // （与文件名那一层不在这一列编东西同一个道理）。
                rom: String::new(),
                hashed_as: Convention::AsIs,
                dat_convention: Convention::AsIs,
                evidence,
                // **中文记号留空**：这一列说的是「DAT 那条条目说自己是汉化版还是官中版」
                // （ADR-0012），模型对这件事一个字都没说，灌进去会让中文占比统计当场失真。
                chinese: None,
                serial: None,
                release_id: None,
            }
        })
        .collect()
}

/// 提问时到底给了哪几样上下文，写成一句进依据。
fn asked_with(question: &Question) -> String {
    let mut parts: Vec<&str> = vec!["文件名"];
    if !question.facts.is_empty() {
        parts.push("已经解析出来的头部字段");
    }
    if !question.inner.is_empty() {
        parts.push("包里装着的文件名");
    }
    if !question.siblings.is_empty() {
        parts.push("同目录别的文件的名字");
    }
    if question.platform.is_some() || question.declared.is_some() {
        parts.push("平台");
    }
    parts.join("、")
}

// ════════════════════════════════════════════════════════════════════════
// 计划：问多少、花多少
// ════════════════════════════════════════════════════════════════════════

/// 这一趟会问多少、花多少——**在发第一个请求之前就算得出来**。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Plan {
    /// 待问的变体数（缓存里已经有答案的不算在内）。
    pub variants: u64,
    /// 缓存里已经有答案、这一趟白拿的变体数。
    pub cached: u64,
    /// 打成多少个请求。
    pub batches: u64,
    /// 上限卡下来**真的会发**多少个请求。
    pub requests: u64,
    /// 输入 token 的估计。**是估的**，见 [`estimate_tokens`]。
    pub input_tokens: u64,
    /// 花费下界（微美元）：输出只用掉猜测本身那么多。
    pub floor_micros: u64,
    /// 花费上界（微美元）：每个请求的输出都把 `max_tokens` 用光。
    pub ceiling_micros: u64,
    /// 一个请求的顶（微美元）。**花费上限最多超出这么多**。
    pub ceiling_per_request_micros: u64,
    /// 问哪个模型。
    pub model: String,
    /// 价目表哪天核实的。
    pub priced_at: String,
    /// **第一条问题长什么样**，原样一段。
    ///
    /// 花三十几美元之前，「要问多少条、花多少钱」还不够——**还得看得见到底问的是什么**。
    /// 上下文拼错了（名字剥岔了、头部字段串到别的变体上）在数字上一点都看不出来，
    /// 在这一段上一眼就看得出来。
    pub sample: Option<String>,
}

/// 一串字大约几个 token。
///
/// **这是个启发式，不是真的分词。** 真数字要么去调 `count_tokens`（那是另一个请求，
/// 而计划这一步的全部意义是「一个请求都不发就说得出要花多少」），要么等响应回来看
/// `usage`（那时钱已经花了）。
///
/// 口径：ASCII 四个字符算一个 token，其余（汉字、假名、全角标点）一个字符算 1.1 个。
/// 报告会把**估的**与**实际的**并排印出来，估偏了多少一眼看得见——下一趟照着调
/// 打包大小就行。
#[must_use]
pub fn estimate_tokens(text: &str) -> u64 {
    let mut ascii = 0_u64;
    let mut wide = 0_u64;
    for ch in text.chars() {
        if ch.is_ascii() {
            ascii += 1;
        } else {
            wide += 1;
        }
    }
    ascii.div_ceil(4) + wide * 11 / 10
}

/// 算一份计划。
#[must_use]
pub fn plan(
    model: &str,
    price: Price,
    limits: &Limits,
    pending: &[Question],
    cached: u64,
    priced_at: &str,
) -> Plan {
    let batch = limits.batch.max(1);
    let batches = pending.len().div_ceil(batch);
    let requests = u64::try_from(batches)
        .unwrap_or(u64::MAX)
        .min(limits.budget);
    // 输入按**真的会发出去的那几批**估：上限卡掉的那些一个字节都不发。
    let mut input_tokens = 0_u64;
    let mut floor_output = 0_u64;
    for (at, chunk) in pending.chunks(batch).enumerate() {
        if u64::try_from(at).unwrap_or(u64::MAX) >= requests {
            break;
        }
        input_tokens += estimate_tokens(&body_of(model, limits, chunk).to_string());
        // 下界：每条给满 `guesses` 个猜测，每个猜测算 40 个 token（标题加一句依据），
        // 再给这一批的 JSON 骨架留 20 个。思考的 token 一个都没算在下界里——
        // 下界就是「一分钱都不多花」的那个数。
        floor_output += u64::try_from(chunk.len()).unwrap_or(0)
            * (u64::try_from(limits.guesses).unwrap_or(0) * 40 + 20);
    }
    let per_request_input = input_tokens.checked_div(requests).unwrap_or(0);
    Plan {
        sample: pending.first().map(|one| one.render(1)),
        variants: u64::try_from(pending.len()).unwrap_or(u64::MAX),
        cached,
        batches: u64::try_from(batches).unwrap_or(u64::MAX),
        requests,
        input_tokens,
        floor_micros: price.cost_micros(input_tokens, floor_output),
        // 上界是**硬的**：一个响应的输出不可能超过 `max_tokens`（思考的 token 也在里面）。
        ceiling_micros: price.cost_micros(input_tokens, requests * limits.max_output_tokens),
        ceiling_per_request_micros: price.cost_micros(per_request_input, limits.max_output_tokens),
        model: model.to_string(),
        priced_at: priced_at.to_string(),
    }
}

/// 把微美元写成人看的美元。
#[must_use]
pub fn dollars(micros: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let value = micros as f64 / 1_000_000.0;
    format!("{value:.2} 美元")
}

// ════════════════════════════════════════════════════════════════════════
// 缓存与装弹
// ════════════════════════════════════════════════════════════════════════

/// 已经问过的答案。**这一层唯一花过钱的东西，所以它只花一次。**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Answers {
    /// `(变体, 提问指纹) → (真答话的那个模型, 答案)`。
    ///
    /// **模型名要跟着答案一起存**：依据里印的必须是当时真答话的那一个，而不是这一趟
    /// 命令行上写的那一个——不然同一条候选两趟印出两个模型名，而 ADR-0002 要的正是
    /// 「事后判断得出一条元数据是可信的还是模型编的」。
    by_ask: BTreeMap<(String, String), (String, Answer)>,
}

impl Answers {
    /// 从中立库读回来的那些行建一份。
    #[must_use]
    pub fn build(rows: Vec<crate::catalog::identify::ModelAnswerRow>) -> Self {
        let mut by_ask = BTreeMap::new();
        for row in rows {
            // 读不出来的当作没有：它会重新排进队里再问一次，而不是端上来一条空的。
            if let Some(answer) = Answer::from_json(&row.answer) {
                by_ask.insert((row.variant_key, row.ask), (row.model, answer));
            }
        }
        Self { by_ask }
    }

    /// 这个变体的这个提问，问过没有；问过的话是**谁**答的、答了什么。
    #[must_use]
    pub fn get(&self, variant_key: &str, ask: &str) -> Option<(&str, &Answer)> {
        self.by_ask
            .get(&(variant_key.to_string(), ask.to_string()))
            .map(|(model, answer)| (model.as_str(), answer))
    }

    /// 一共存着几条。
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_ask.len()
    }

    /// 空的吗。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_ask.is_empty()
    }
}

/// 这一层的**弹药**：缓存、价钱、上限，以及（要真问时）网络句柄。
///
/// 与 [`Naming`](super::fuzzy::Naming) 同一个形状，理由也同一个：它们同进同出，
/// 散成四个参数迟早会漏传一个。
pub struct Guessing<'a> {
    /// 问过的答案。**没有网络句柄时它照样用**——那正是「重跑一遍不再花钱」。
    pub answers: &'a Answers,
    /// 网络句柄；`None` 表示这一趟**只用缓存、一个请求都不发**。
    pub net: Option<&'a Inference<'a>>,
    /// **只算计划**：残渣照样一个一个认出来、[`Plan`] 照样算得出，但一个请求都不发。
    ///
    /// 它必须是独立的一档，不能靠「有没有网络句柄」推出来：`--model-plan` 的整个
    /// 意义就是**花钱之前先看一眼**，而那时既没有网络句柄、缓存也多半是空的——
    /// 只按那两样判，这一层会整个不跑，然后报告一声不响地什么都不说。
    pub planning: bool,
    /// 问哪个模型。
    pub model: String,
    /// 这个模型多少钱。**它是从价目表里查出来的**——查不到就整层不启动，
    /// 所以到了这里它一定存在（[`Pricing::price`]）。
    pub price: Price,
    /// 价目表哪天核实的。报告原样印出来——价目表过时是这一层唯一会安静算错钱的地方。
    pub checked: String,
    /// 上限。
    pub limits: Limits,
    /// **计划算出来之后、第一个请求发出去之前**，把它交给调用方摆出来。
    ///
    /// 这一层唯一花钱，而「花费可预估」这条验收的意思**不是**「跑完之后报告里有个数」
    /// ——那时钱已经花了。计划只有在残渣全部确定之后才算得出来（那是主循环跑完的
    /// 时刻），所以调用方没法在开跑前自己算一份；于是把它从这里递出去。
    pub announce: Option<&'a dyn Fn(&Plan)>,
}

impl Guessing<'_> {
    /// **这一层整个关掉的那一份**：缓存空的、网络句柄没有，于是既不问也没得可用。
    #[must_use]
    pub fn off() -> Guessing<'static> {
        static EMPTY: std::sync::OnceLock<Answers> = std::sync::OnceLock::new();
        Guessing {
            answers: EMPTY.get_or_init(Answers::default),
            net: None,
            announce: None,
            planning: false,
            model: DEFAULT_MODEL.to_string(),
            price: Price {
                input_per_mtok: 0,
                output_per_mtok: 0,
            },
            checked: String::new(),
            limits: Limits::default(),
        }
    }

    /// 这一层这一趟有事可做吗——要算计划、有缓存可用，或者真的能发请求。
    #[must_use]
    pub fn ready(&self) -> bool {
        self.planning || self.net.is_some() || !self.answers.is_empty()
    }

    /// 这一趟会真的发请求吗。
    #[must_use]
    pub fn asking(&self) -> bool {
        self.net.is_some()
    }
}

/// **模型推断那一层**这一趟干了什么。
///
/// 与 [`CartCount`](super::CartCount)、[`FuzzyCount`](super::FuzzyCount) 同一个道理：
/// 这些数永远同进同出，散成一堆字段的话，加一个计数器要改三处。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ModelCount {
    /// 有几个变体落到了这一层（前面各层一条候选都没有）。
    pub residue: u64,
    /// 其中直接从缓存里取到答案的。**这一趟没为它们花一分钱。**
    pub from_cache: u64,
    /// 这一趟真的问出去的变体数。**按发出去的条数算，不按答回来的条数算**——
    /// 少答的那几条钱一样付了。
    pub asked: u64,
    /// 产出的候选条数。
    pub candidates: u64,
    /// 拿到候选的变体数。
    pub with_candidates: u64,
    /// **模型自己说不出**的变体数（答复里一个猜测都没有）。它们是这一层的正当产出。
    pub speechless: u64,
    /// 这一趟在网络上的账（请求数、token、花费），照抄响应里 `usage` 的真数字。
    ///
    /// **整个嵌进来而不是一个字段一个字段抄**：它们同进同出，抄的那一版每加一个
    /// 计数器都要在两处各改一遍，而漏掉一处只表现成一个偏小的数字。
    pub usage: Usage,
    /// 计划里估的输入 token。摆在实际旁边，估偏了多少一眼看得见。
    pub estimated_input_tokens: u64,
    /// 一个请求里打包了几条。**「批量打包」这件事在报告里得是个数字**，
    /// 不能只是一句话——一条一发与二十条一发，报告读起来该不一样。
    pub batch_size: u64,
    /// 这一趟的计划。
    pub plan: Option<Plan>,
    /// 停了的话，那一句给人看的话。
    pub halted: Option<String>,
    /// 停了的话，是**哪一种**停。退出码按它分档。
    pub halt: Option<HaltKind>,
}

impl ModelCount {
    /// 问出去了、却**一个字都没答回来**的变体数。
    ///
    /// 它与 [`speechless`](Self::speechless) 是两件事，而混起来会瞒掉一笔真花销：
    /// 「说不出」是一条答复（模型看过了，说这不是游戏），「没答回来」是这一条根本
    /// 没出现在响应里（一批被截断在半截 JSON 上，或者形状认不出来）。
    /// 前者下一趟不必再问，**后者会被再问一遍、再付一次钱**。
    #[must_use]
    pub fn unanswered(&self) -> u64 {
        self.asked
            .saturating_sub(self.with_candidates + self.speechless)
    }

    /// 估的输入 token 比实际偏了百分之多少（正数是估多了）。
    ///
    /// 报告把它印出来，是为了让**下一趟的计划可信**：这一层的估算是个启发式
    /// （[`estimate_tokens`]），偏多少只有跑过才知道，而跑过之后不说出来，
    /// 那个启发式就永远没有被校准的机会。
    #[must_use]
    pub fn estimate_skew(&self) -> f64 {
        if self.usage.input_tokens == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)]
        {
            (self.estimated_input_tokens as f64 - self.usage.input_tokens as f64) * 100.0
                / self.usage.input_tokens as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::VariantRow;

    fn 变体(key: &str, platform: Option<&str>) -> VariantRow {
        VariantRow {
            key: key.to_string(),
            platform: platform.map(ToString::to_string),
            rule: "同名成组".to_string(),
            main_key: key.to_string(),
            files: 1,
            bytes: 4096,
            unreadable_files: 0,
            manual: false,
            work_id: None,
            release_id: None,
        }
    }

    fn 问题(key: &str) -> Question {
        Question {
            variant_key: key.to_string(),
            main_key: key.to_string(),
            platform: Some("NDS".to_string()),
            declared: Some("psp".to_string()),
            names: vec![("ACGHH-0113-TP.7z".to_string(), "变体自己的名字")],
            inner: vec!["ACGHH.nds".to_string()],
            siblings: vec!["合金弹头7.7z".to_string()],
            facts: vec!["卡带游戏码 ACGHH（NDS 头 0x00C）".to_string()],
            bytes: 4096,
        }
    }

    fn 一批(n: usize) -> Vec<Question> {
        (0..n).map(|at| 问题(&format!("nds/第{at}个.7z"))).collect()
    }

    // ════════════════════════════════════════════════════════════
    // 硬约束一：批量打包
    // ════════════════════════════════════════════════════════════

    #[test]
    fn 一个请求里装着一整批而不是一条() {
        let limits = Limits::default();
        let batch = 一批(20);
        let body = body_of(DEFAULT_MODEL, &limits, &batch);
        let prompt = body["messages"][0]["content"]
            .as_str()
            .expect("正文里该有那一段");
        // 二十条各占一段，编号从 1 数到 20——**这才是「打包」，不是发二十个请求**。
        for id in 1..=20 {
            assert!(prompt.contains(&format!("### 第 {id} 条")), "缺第 {id} 条");
        }
        assert!(prompt.contains("一共 20 条，编号 1 到 20"));
    }

    #[test]
    fn 上下文里三样都在() {
        // 票 12 点名的三样：文件名、已经解析出来的头部字段、同目录的别的文件。
        let prompt = 问题("nds/x.7z").render(1);
        assert!(prompt.contains("ACGHH-0113-TP.7z"));
        assert!(prompt.contains("卡带游戏码 ACGHH"));
        assert!(prompt.contains("同目录还有：合金弹头7.7z"));
        assert!(prompt.contains("包里装着：ACGHH.nds"));
        // 平台两句话打架时，两个都说出来——只给一个的话模型没法知道它们不一致。
        assert!(prompt.contains("内部头读出来是 NDS"));
        assert!(prompt.contains("躺在 psp 目录下"));
    }

    #[test]
    fn 编号按批次算所以同一条落在哪一批都是同一个指纹() {
        // 指纹与打包大小无关：改一次 --model-batch 不该把几千条答案全部作废重问。
        let limits = Limits::default();
        let 大批 = Limits {
            batch: 50,
            ..limits.clone()
        };
        let one = 问题("nds/x.7z");
        assert_eq!(
            one.ask(DEFAULT_MODEL, &limits),
            one.ask(DEFAULT_MODEL, &大批)
        );
    }

    #[test]
    fn 同目录多放一个文件不会让整批重问() {
        // 那一栏是上下文里唯一会因为别人而变的东西。拿全文当指纹的话，往目录里放一个
        // 文件就让这个目录下每一条残渣整批重问——而问的还是同样那几个东西，钱是真付。
        let 原样 = 问题("nds/x.7z");
        let mut 多了一个 = 原样.clone();
        多了一个.siblings.push("刚拷进来的东西.7z".to_string());
        let limits = Limits::default();
        assert_eq!(
            原样.ask(DEFAULT_MODEL, &limits),
            多了一个.ask(DEFAULT_MODEL, &limits)
        );
        // 但**它自己**的名字变了就是另一个问题了。
        let mut 换了名字 = 原样.clone();
        换了名字.names[0].0 = "别的名字.7z".to_string();
        assert_ne!(
            原样.ask(DEFAULT_MODEL, &limits),
            换了名字.ask(DEFAULT_MODEL, &limits)
        );
        // 头部字段是从字节里读出来的，它变了当然要重问。
        let mut 读出了头 = 原样.clone();
        读出了头.facts.push("光盘标识 SLPS-02330".to_string());
        assert_ne!(
            原样.ask(DEFAULT_MODEL, &limits),
            读出了头.ask(DEFAULT_MODEL, &limits)
        );
    }

    #[test]
    fn 换模型或换力度会重问() {
        let one = 问题("nds/x.7z");
        let 基准 = Limits::default();
        let 换力度 = Limits {
            effort: "max".to_string(),
            ..Limits::default()
        };
        assert_ne!(
            one.ask(DEFAULT_MODEL, &基准),
            one.ask(DEFAULT_MODEL, &换力度)
        );
        assert_ne!(
            one.ask(DEFAULT_MODEL, &基准),
            one.ask("claude-haiku-4-5", &基准)
        );
    }

    // ════════════════════════════════════════════════════════════
    // 硬约束三：永不自动通过
    // ════════════════════════════════════════════════════════════

    #[test]
    fn 这一层永不自动通过而且封顶低置信() {
        let answer = Answer {
            guesses: vec![
                Guess {
                    title: "合金弹头7".to_string(),
                    platform: Some("NDS".to_string()),
                    basis: "包里那个 ACGHH 是 NDS 的游戏码".to_string(),
                },
                Guess {
                    title: "合金弹头6".to_string(),
                    platform: None,
                    basis: "同系列".to_string(),
                },
            ],
        };
        let built = candidates_of(
            &变体("nds/x.7z", Some("NDS")),
            &问题("nds/x.7z"),
            &answer,
            DEFAULT_MODEL,
        );
        assert_eq!(built.len(), 2);
        for candidate in &built {
            // ADR-0002：模型推断的结论永远进队列。
            assert!(!candidate.accepted, "这一层永不自动通过");
            assert_eq!(candidate.confidence, CEILING);
            assert_eq!(candidate.confidence, Confidence::Low);
            assert_eq!(candidate.source, SOURCE);
            // **依据的第一句就写着它是模型推断的。**
            assert!(
                candidate.evidence.starts_with("**这一条是模型推断的**"),
                "{}",
                candidate.evidence
            );
            assert!(candidate.evidence.contains(DEFAULT_MODEL));
            assert!(candidate.evidence.contains("一个字节的内容都没有看过"));
            assert!(candidate.evidence.contains("永不自动通过"));
        }
        // 排序保住了：模型排第几，候选就写第几。
        assert!(built[0].evidence.contains("第 1 个猜测"));
        assert!(built[1].evidence.contains("第 2 个猜测"));
        assert_eq!(built[0].game, "合金弹头7");
    }

    #[test]
    fn 依据里说得出提问时给了哪几样上下文() {
        let answer = Answer {
            guesses: vec![Guess {
                title: "合金弹头7".to_string(),
                platform: None,
                basis: "".to_string(),
            }],
        };
        let built = candidates_of(
            &变体("nds/x.7z", Some("NDS")),
            &问题("nds/x.7z"),
            &answer,
            DEFAULT_MODEL,
        );
        let evidence = &built[0].evidence;
        assert!(evidence.contains("已经解析出来的头部字段"));
        assert!(evidence.contains("同目录别的文件的名字"));
        // 依据说不出时也得说清「没说」，不能留一个空引号让人以为丢了东西。
        assert!(evidence.contains("（没说）"));
    }

    #[test]
    fn 中文记号与那条记录名都留空() {
        // `chinese` 这一列说的是「DAT 那条条目说自己是汉化版还是官中版」（ADR-0012）；
        // `rom` 是「DAT 里那条 <rom> 记录的名字」。模型两样都没有，编一个上去
        // 会让事后复核的人以为真有那么一条记录。
        let answer = Answer {
            guesses: vec![Guess {
                title: "合金弹头7".to_string(),
                platform: None,
                basis: "名字".to_string(),
            }],
        };
        let built = candidates_of(
            &变体("nds/x.7z", Some("NDS")),
            &问题("nds/x.7z"),
            &answer,
            DEFAULT_MODEL,
        );
        assert_eq!(built[0].chinese, None);
        assert_eq!(built[0].rom, "");
    }

    #[test]
    fn 说不出的时候一条候选都不产出() {
        // 这批东西里混着模拟器、存档、主题包——留空比编一个名字好。
        let built = candidates_of(
            &变体("nds/x.7z", Some("NDS")),
            &问题("nds/x.7z"),
            &Answer::default(),
            DEFAULT_MODEL,
        );
        assert!(built.is_empty());
    }

    // ════════════════════════════════════════════════════════════
    // 响应解析：认不出的形状是「答不上来」而不是「答错」
    // ════════════════════════════════════════════════════════════

    #[test]
    fn 答案按这一批的编号归位() {
        let response = serde_json::json!({
            "content": [{
                "type": "text",
                "text": "{\"答案\":[\
                    {\"编号\":1,\"猜测\":[{\"标题\":\"甲\",\"平台\":\"NDS\",\"依据\":\"因为\"}]},\
                    {\"编号\":3,\"猜测\":[]}]}"
            }]
        });
        let answers = parse_answers(&response, 3);
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[&0].guesses[0].title, "甲");
        assert!(answers[&2].guesses.is_empty());
        // 第 2 条没答——它不会被编一个空答案顶上去，于是下一趟还会被问一遍。
        assert!(!answers.contains_key(&1));
    }

    #[test]
    fn 编号越界的一条都不收() {
        // 挂错变体比不挂糟得多：一条挂到别人身上的候选，人在队列里看不出它挂错了。
        let response = serde_json::json!({
            "content": [{
                "type": "text",
                "text": "{\"答案\":[{\"编号\":9,\"猜测\":[{\"标题\":\"甲\",\"依据\":\"x\"}]},\
                          {\"编号\":0,\"猜测\":[{\"标题\":\"乙\",\"依据\":\"x\"}]}]}"
            }]
        });
        assert!(parse_answers(&response, 3).is_empty());
    }

    #[test]
    fn 思考块跳过去不当成答案解析() {
        let response = serde_json::json!({
            "content": [
                { "type": "thinking", "thinking": "" },
                { "type": "text", "text": "{\"答案\":[{\"编号\":1,\"猜测\":[]}]}" }
            ]
        });
        assert_eq!(parse_answers(&response, 1).len(), 1);
    }

    #[test]
    fn 认不出的形状是答不上来而不是答错() {
        for 怪 in [
            serde_json::json!({}),
            serde_json::json!({ "content": [] }),
            serde_json::json!({ "content": [{ "type": "text", "text": "这不是 JSON" }] }),
            serde_json::json!({ "content": [{ "type": "text", "text": "{\"别的\":1}" }] }),
        ] {
            assert!(parse_answers(&怪, 5).is_empty(), "{怪}");
        }
    }

    #[test]
    fn 标题空着的猜测不算一条() {
        let response = serde_json::json!({
            "content": [{
                "type": "text",
                "text": "{\"答案\":[{\"编号\":1,\"猜测\":[{\"标题\":\"  \",\"依据\":\"x\"}]}]}"
            }]
        });
        assert!(parse_answers(&response, 1)[&0].guesses.is_empty());
    }

    #[test]
    fn 答案存进库再读回来是同一份() {
        let answer = Answer {
            guesses: vec![Guess {
                title: "合金弹头7".to_string(),
                platform: Some("NDS".to_string()),
                basis: "游戏码".to_string(),
            }],
        };
        assert_eq!(Answer::from_json(&answer.to_json()), Some(answer));
        // 读不出来的当作**没有**：它会重新排进队里再问一次，而不是端上来一条空的。
        assert_eq!(Answer::from_json("坏掉的"), None);
    }

    // ════════════════════════════════════════════════════════════
    // 价目表与计划：花费可预估、可设上限
    // ════════════════════════════════════════════════════════════

    #[test]
    fn 内置价目表解析得了而且有默认那个模型() {
        let pricing = Pricing::builtin();
        let price = pricing.price(DEFAULT_MODEL).expect("默认模型必须有价");
        // $5 / $25 每百万 token。
        assert_eq!(price.input_per_mtok, 500);
        assert_eq!(price.output_per_mtok, 2500);
        assert!(!pricing.checked().is_empty(), "核实日期不许空着");
        assert!(!pricing.cite().is_empty(), "出处不许空着");
        // 表里没有的模型是 `None`，而 `None` 让整层不启动。
        assert_eq!(pricing.price("没见过的模型"), None);
    }

    #[test]
    fn 算钱的单位一步都没跳() {
        let price = Pricing::builtin().price(DEFAULT_MODEL).expect("有价");
        // 一百万输入 token = 5 美元 = 5,000,000 微美元。
        assert_eq!(price.cost_micros(1_000_000, 0), 5_000_000);
        // 一百万输出 token = 25 美元。
        assert_eq!(price.cost_micros(0, 1_000_000), 25_000_000);
        assert_eq!(dollars(price.cost_micros(1_000_000, 0)), "5.00 美元");
    }

    #[test]
    fn 计划在发第一个请求之前就说得出问多少花多少() {
        let price = Pricing::builtin().price(DEFAULT_MODEL).expect("有价");
        let limits = Limits {
            batch: 20,
            budget: 40,
            ..Limits::default()
        };
        let plan = plan(DEFAULT_MODEL, price, &limits, &一批(45), 7, "2026-09-02");
        assert_eq!(plan.variants, 45);
        assert_eq!(plan.cached, 7);
        assert_eq!(plan.batches, 3);
        assert_eq!(plan.requests, 3);
        assert!(plan.input_tokens > 0);
        // 上界是**硬的**：一个响应的输出不可能超过 max_tokens。
        assert!(plan.ceiling_micros > plan.floor_micros);
        assert!(plan.ceiling_micros >= price.cost_micros(0, 3 * limits.max_output_tokens));
        assert!(plan.ceiling_per_request_micros > 0);
    }

    #[test]
    fn 请求上限先把计划卡下来_不算那些不会发的批次() {
        let price = Pricing::builtin().price(DEFAULT_MODEL).expect("有价");
        let 卡住 = Limits {
            batch: 20,
            budget: 1,
            ..Limits::default()
        };
        let 放开 = Limits {
            budget: 40,
            ..卡住.clone()
        };
        let 少 = plan(DEFAULT_MODEL, price, &卡住, &一批(45), 0, "x");
        let 多 = plan(DEFAULT_MODEL, price, &放开, &一批(45), 0, "x");
        assert_eq!(少.batches, 3, "打得成三批");
        assert_eq!(少.requests, 1, "但上限只让发一个");
        // 不会发的那两批**一个 token 都不该算进花费**——算进去，用户会被一个
        // 根本不会发生的数字吓退。
        assert!(少.input_tokens > 0);
        assert!(
            少.input_tokens * 2 < 多.input_tokens,
            "只发一批的输入该远小于发三批：{} vs {}",
            少.input_tokens,
            多.input_tokens
        );
        assert!(少.floor_micros < 多.floor_micros);
    }

    #[test]
    fn token_估算对中文与_ascii_分开算() {
        // 启发式：ASCII 四个字符一个 token，汉字一个字符 1.1 个。
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("汉字"), 2);
        assert!(estimate_tokens("") == 0);
    }

    #[test]
    fn 力度与猜测条数进提问指纹_打包大小不进() {
        let a = Limits::default();
        let b = Limits {
            guesses: 5,
            ..Limits::default()
        };
        let c = Limits {
            batch: 5,
            budget: 1,
            spend_cap_micros: 1,
            interval: std::time::Duration::from_secs(9),
            ..Limits::default()
        };
        assert_ne!(a.ask_fingerprint(), b.ask_fingerprint());
        assert_eq!(a.ask_fingerprint(), c.ask_fingerprint());
    }

    // ════════════════════════════════════════════════════════════
    // 缓存
    // ════════════════════════════════════════════════════════════

    #[test]
    fn 缓存按变体加提问指纹存取() {
        let answer = Answer {
            guesses: vec![Guess {
                title: "甲".to_string(),
                platform: None,
                basis: "x".to_string(),
            }],
        };
        let 行 = |key: &str, ask: &str, model: &str, json: &str| {
            crate::catalog::identify::ModelAnswerRow {
                variant_key: key.to_string(),
                ask: ask.to_string(),
                model: model.to_string(),
                answer: json.to_string(),
            }
        };
        let answers = Answers::build(vec![
            行("nds/x.7z", "指纹甲", "claude-opus-5", &answer.to_json()),
            // 读不出来的那条当作没有，不进索引。
            行("nds/y.7z", "指纹乙", "claude-opus-5", "坏掉的"),
        ]);
        assert_eq!(answers.len(), 1);
        // **谁答的**跟着答案一起取回来：依据里印的必须是当时真答话的那一个。
        let (谁答的, 答案) = answers.get("nds/x.7z", "指纹甲").expect("问过了");
        assert_eq!(谁答的, "claude-opus-5");
        assert_eq!(答案, &answer);
        // **指纹对不上就是没问过**——换了模型或改了提示词，那条答案不该被端上来。
        assert!(answers.get("nds/x.7z", "别的指纹").is_none());
        assert!(answers.get("nds/y.7z", "指纹乙").is_none());
    }

    #[test]
    fn 关掉的那一份既不问也没得可用() {
        let off = Guessing::off();
        assert!(!off.ready());
        assert!(!off.asking());
    }

    #[test]
    fn 停下来分得出是自己收的手还是对面不让() {
        // 退出码按它分档：一个立刻重跑就接着问，一个重跑没用。
        assert_eq!(Halt::Budget { spent: 40, cap: 40 }.kind(), HaltKind::Self_);
        assert_eq!(Halt::Spend { spent: 1, cap: 1 }.kind(), HaltKind::Self_);
        assert_eq!(Halt::Fatal { why: "x" }.kind(), HaltKind::Refused);
        assert_eq!(Halt::Throttled { tries: 3 }.kind(), HaltKind::Refused);
        assert_eq!(Halt::Offline { tries: 5 }.kind(), HaltKind::Refused);
    }

    #[test]
    fn 落点在取数闸门的白名单上() {
        // 这一层与真服务共用同一道闸门（`dat::guard`），不另开一条路。
        assert!(crate::dat::guard::check(ENDPOINT).is_ok());
    }
}
