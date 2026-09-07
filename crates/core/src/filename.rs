//! **文件名规则**：把磁盘上那个名字剥成**正题**、**汉化组**与版本。
//!
//! 数据库覆盖不到的那批变体，手上只剩一个文件名。而这个库里的文件名不是标题——
//! 它是标题**加上**十几年间几十个汉化组、几十个整理者各自的记号：
//!
//! ```text
//! gba/【全部汉化】/超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip
//! psp/…/ROM/我的暑假[简体汉化版][ACG汉化组]/ACG_Summer_Holiday.7z
//! GB/…/【其他整理】/GBC日文汉字替换完美显示-595个/中文版/71.智力：仙界異聞録.zip
//! ```
//!
//! 拿这三行原样去撞任何数据源都是零命中。剥完剩下的 `超级机器人大战R`、`我的暑假`、
//! `仙界異聞録` 才是能撞的东西——[中文离线数据源](crate::zh)那一层要的正是它。
//!
//! ## 剥离规则是**配置**
//!
//! 规律只会不断冒出新的（这个库里有三百多个汉化组署名，而且还在长）。所以规则住在
//! [`rules.toml`](Rules::BUILTIN) 里，与平台清单同一条道理（`docs/platforms.md`：
//! 清单是数据不是代码）。`--name-rules <文件>` 换成自己的一份，而且**默认是补充不是
//! 换掉**：`"继承内置" = true` 时每张表都接在内置那份后面，维护者遇到一个新汉化组写
//! 三行就够——抄一份完整的内置表出来，只会在下次升级时悄悄落后。
//!
//! ## 认不出的记号**要说出来**
//!
//! [`Parsed::unknown`] 收着所有归不了类的记号组。它不是调试信息，是这一层的**主要
//! 产出之一**：维护者照着报告里那张「认不出的记号 Top N」往配置里加，加完重跑一遍就
//! 看得见效果——这正是「规则是配置」真正兑现的地方。悄悄剥掉认不出的记号也能跑，
//! 但那样谁也不知道该往配置里补什么。
//!
//! ## 剥掉的每一样都记下来
//!
//! [`Parsed::stripped`] 逐条记着「剥掉了什么、按哪条规则」，因为这一层产出的候选
//! **一律进待确认队列**（ADR-0002），而人在队列里要判断的第一件事就是「它是从哪个
//! 名字剥出来的」。没有这一栏，一条 `仙界異聞録` 的候选看上去像是凭空出现的。

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

use crate::classify::{is_cjk, is_han};
use crate::path::fold;
use crate::platform::pattern::{Pattern, PatternError};

/// 本程序认得的规则文件版本。
pub const RULES_VERSION: u32 = 1;

/// 规则文件读不进来的原因。
#[derive(Debug, thiserror::Error)]
pub enum RulesError {
    /// 文件读不到。
    #[error("剥离规则读不到：{path}（{source}）")]
    Io {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// TOML 解析失败。
    #[error("剥离规则 {path} 解析失败：{detail}")]
    Toml {
        /// 出问题的文件。
        path: String,
        /// 解析器说了什么。
        detail: String,
    },
    /// 版本对不上。
    #[error("剥离规则 {path} 的版本是 {found}，本程序认得的是 {RULES_VERSION}")]
    Version {
        /// 出问题的文件。
        path: String,
        /// 文件里写的版本。
        found: u32,
    },
    /// 某条模式写坏了。
    #[error("剥离规则 {path} 里的模式有问题：{source}")]
    Pattern {
        /// 出问题的文件。
        path: String,
        /// 哪条模式、坏在哪。
        source: PatternError,
    },
}

/// TOML 里那份原样的规则。
#[derive(Debug, Default, Deserialize)]
struct Raw {
    #[serde(rename = "版本")]
    version: u32,
    #[serde(rename = "继承内置", default = "yes")]
    inherit: bool,
    #[serde(rename = "汉化组词", default)]
    team_words: Vec<String>,
    #[serde(rename = "汉化组名", default)]
    team_names: Vec<String>,
    #[serde(rename = "语言地区", default)]
    languages: Vec<String>,
    #[serde(rename = "版本模式", default)]
    versions: Vec<String>,
    #[serde(rename = "容量模式", default)]
    sizes: Vec<String>,
    #[serde(rename = "记号噪音", default)]
    marker_noise: Vec<String>,
    #[serde(rename = "正题噪音词", default)]
    title_noise: Vec<String>,
    #[serde(rename = "分类词", default)]
    genres: Vec<String>,
    #[serde(rename = "平台前缀", default)]
    platform_prefixes: Vec<String>,
    #[serde(rename = "编号分隔", default)]
    numbering: Vec<String>,
    #[serde(rename = "中文源平台别名", default)]
    aliases: Vec<RawAlias>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct RawAlias {
    #[serde(rename = "叫")]
    called: String,
    #[serde(rename = "是")]
    is: String,
}

/// 一份编译好的剥离规则。
#[derive(Debug, Clone)]
pub struct Rules {
    team_words: Vec<String>,
    team_names: BTreeSet<String>,
    languages: BTreeSet<String>,
    versions: Vec<Pattern>,
    sizes: Vec<Pattern>,
    marker_noise: BTreeSet<String>,
    /// 正题噪音词，**按长度从长到短**排好——`简体中文版` 要在 `中文` 之前试，
    /// 不然剥完只剩个 `简体` 挂在标题尾巴上。
    title_noise: Vec<String>,
    genres: BTreeSet<String>,
    platform_prefixes: Vec<String>,
    numbering: Vec<char>,
    aliases: Vec<(String, String)>,
    /// 这份规则**剥出正题**那一套的指纹，编译时算一次（见 [`Rules::fingerprint`]）。
    ///
    /// 存下来而不是每次现算：一趟刮削要问它十万次量级（变体数 × 两个中文源 + 作品数，
    /// `docs/scale-reference.md`），而内置那份规则的表加起来五千多字节，现算等于把这
    /// 五千字节逐字节折十万遍、再分配十万个字符串。索引那一侧的 `zh::Index::platform_fold`
    /// 存成一串，也是这个理由。
    fingerprint: String,
}

impl Rules {
    /// 内置的那一份规则文件，原样的文本。`--dump-builtin` 导出它当底稿。
    pub const BUILTIN: &'static str = include_str!("filename/rules.toml");

    /// 内置的那一份。
    ///
    /// # Panics
    /// 内置文件写坏了才会 panic——那是编译期就该发现的事，有一条测试盯着。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse_text(Self::BUILTIN, "内置").expect("内置的剥离规则必须自洽")
    }

    /// 从文件读一份。
    ///
    /// **默认是补充**：文件里 `"继承内置"` 不写或写 `true` 时，每张表都接在内置那份
    /// 后面；写 `false` 才是整份换掉。
    ///
    /// # Errors
    /// 读不到、解析不了、版本对不上、或者某条模式写坏了时返回错误。
    pub fn load(path: &Path) -> Result<Self, RulesError> {
        let text = std::fs::read_to_string(path).map_err(|source| RulesError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse_text(&text, &path.display().to_string())
    }

    fn parse_text(text: &str, whence: &str) -> Result<Self, RulesError> {
        let raw: Raw = toml::from_str(text).map_err(|error| RulesError::Toml {
            path: whence.to_string(),
            detail: error.to_string(),
        })?;
        if raw.version != RULES_VERSION {
            return Err(RulesError::Version {
                path: whence.to_string(),
                found: raw.version,
            });
        }
        let base = if raw.inherit && whence != "内置" {
            Some(Self::builtin())
        } else {
            None
        };
        let compile = |list: &[String]| -> Result<Vec<Pattern>, RulesError> {
            list.iter()
                .map(|text| {
                    Pattern::new(text).map_err(|source| RulesError::Pattern {
                        path: whence.to_string(),
                        source,
                    })
                })
                .collect()
        };
        let mut versions = compile(&raw.versions)?;
        let mut sizes = compile(&raw.sizes)?;
        let mut team_words: Vec<String> = raw.team_words.iter().map(|it| fold(it)).collect();
        let mut team_names: BTreeSet<String> = raw.team_names.iter().map(|it| fold(it)).collect();
        let mut languages: BTreeSet<String> = raw.languages.iter().map(|it| fold(it)).collect();
        let mut marker_noise: BTreeSet<String> =
            raw.marker_noise.iter().map(|it| fold(it)).collect();
        let mut title_noise: Vec<String> = raw.title_noise.iter().map(|it| fold(it)).collect();
        let mut genres: BTreeSet<String> = raw.genres.iter().map(|it| fold(it)).collect();
        let mut platform_prefixes: Vec<String> =
            raw.platform_prefixes.iter().map(|it| fold(it)).collect();
        let mut numbering: Vec<char> = raw
            .numbering
            .iter()
            .filter_map(|it| it.chars().next())
            .collect();
        let mut aliases: Vec<(String, String)> = raw
            .aliases
            .iter()
            .map(|it| (fold_platform(&it.called), it.is.clone()))
            .collect();
        if let Some(base) = base {
            team_words.extend(base.team_words);
            team_names.extend(base.team_names);
            languages.extend(base.languages);
            versions.extend(base.versions);
            sizes.extend(base.sizes);
            marker_noise.extend(base.marker_noise);
            title_noise.extend(base.title_noise);
            genres.extend(base.genres);
            platform_prefixes.extend(base.platform_prefixes);
            numbering.extend(base.numbering);
            // **用户那几条排在前面，内置的接在后面**，然后按键去重留先出现的那条——
            // 于是用户既补得了新条目，也**改得动内置那一条**。都塞进去再排序的话，
            // 同一个键上内置那条会按字典序赢，而用户的更正静默失效。
            aliases.extend(base.aliases);
        }
        // 长的先试：`简体中文版` 要排在 `中文` 前面，不然剥完还剩半截。
        title_noise.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()).then(a.cmp(b)));
        title_noise.dedup();
        // 平台前缀同理：`gbavc` 要排在 `gba` 前面。
        platform_prefixes.sort_by(|a, b| b.len().cmp(&a.len()).then(a.cmp(b)));
        platform_prefixes.dedup();
        team_words.sort();
        team_words.dedup();
        numbering.sort_unstable();
        numbering.dedup();
        let mut seen: BTreeSet<String> = BTreeSet::new();
        aliases.retain(|(called, _)| seen.insert(called.clone()));
        let mut rules = Self {
            team_words,
            team_names,
            languages,
            versions,
            sizes,
            marker_noise,
            title_noise,
            genres,
            platform_prefixes,
            numbering,
            aliases,
            fingerprint: String::new(),
        };
        rules.fingerprint = rules.compute_fingerprint();
        Ok(rules)
    }

    /// 中文数据源写的那个平台名，在本工具里叫什么。认不出就是 `None`。
    ///
    /// 只补平台清单那张 `目录` 别名表折不动的那些（`Nintendo Switch`、
    /// `PlayStation Vita`）——`GBA` / `NDS` / `PSP` 两边写法一模一样，那张表就够了。
    #[must_use]
    pub fn platform_alias(&self, text: &str) -> Option<&str> {
        let key = fold_platform(text);
        self.aliases
            .iter()
            .find(|(called, _)| *called == key)
            .map(|(_, is)| is.as_str())
    }

    /// 这份规则**剥出正题**那一套的指纹。
    ///
    /// 剥离规则**是配置不是代码**（模块开头那一段）：用户补一条自己遇到的模式，剥出来的
    /// [正题](Parsed::title)就变了，正题一变撞出来的中文条目就可能变。所以它进
    /// [刮削那一侧两层锚点的输入指纹](crate::scrape::zh)——不进的话，用户调完规则重跑
    /// 一趟，缓存会一口咬定「输入没变」而整条跳过：**什么都不重采，而且不报错**。
    /// 那比崩一次难发现得多，用户只会以为规则没写对。
    ///
    /// 算的是**编译完那份规则的语义**，不是文件的字节：给 `rules.toml` 加一行注释、
    /// 换个缩进、调换 `版本模式` 的次序、把一条模式写成 `V#*` 还是 `v#*`（模式的字面量
    /// 大小写不敏感），都不该让全库重采一遍。
    ///
    /// **`中文源平台别名` 不在里面，这是有意的。** 那张表只在**建索引**时用得着
    /// （[`zh::sync`](crate::zh::sync)），它对刮削的全部影响都由建索引那一刻折出来的
    /// 那一串（[`zh::Index::platform_fold`](crate::zh::Index::platform_fold)）盖着，
    /// 而那一串已经进了两层指纹。再盖一遍的话，「改了别名但还没重建索引」会让指纹变、
    /// 可这一趟的输入一个字都没变——那是白重采一整趟。
    ///
    /// **编译时算一次就存下来**（见 [`Rules::fingerprint`] 那一格的说明），这里只是把它
    /// 交出去。
    ///
    /// **剥一个名字这件事**折成一串，进[输入指纹](crate::scrape::Source::probe)。
    ///
    /// 规则是**配置不是代码**（模块文档开头那一节），所以它是刮削的一样**输入**：
    /// 补一条正题噪音词重跑一趟，同一个变体从「撞不上」变成「撞得上」，那一趟就该
    /// 真的重采。不进指纹的话，缓存会一口咬定「输入没变」而整条跳过，用户照着
    /// [`crate::zh`] 的模块文档补完配置重跑，一条新产出都没有。
    ///
    /// ## 折的是**合并之后**那几张表，不是规则文件的文本
    ///
    /// - **真正生效的是合并完的那一份。** 内置那份（[`BUILTIN`](Self::BUILTIN)）改一个字，
    ///   同样该重采，而文件文本对它一无所知——`--name-rules` 压根没给时更是连文本都没有。
    /// - **「没有规则文件」与「有一份什么都不补的规则文件」行为完全一样**，指纹也就该
    ///   一样。按文本折，两者分家，白重采一遍。空表与缺表同理：合并完都是空的。
    /// - **TOML 里键怎么排、表怎么分块与结果无关。** 按文本折会把排版当成输入。
    ///
    /// ## 平台别名那一张**不在里面**
    ///
    /// 它不参加 [`parse`](Self::parse)，只在**建中文索引那一刻**起作用，而那件事由索引
    /// 自己记着的那张折叠表盖住（`zh::Index::platform_fold`）。搬到这里来会把
    /// 「改了别名但还没重建索引」说成「这份索引变了」——而它一个字都没变。
    #[must_use]
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 折出上面那一串。只在 [`parse_text`](Self::parse_text) 末尾跑一次。
    ///
    /// 只要「不同的规则大概率不同指纹」，不需要密码学强度，用的是 FNV-1a，与
    /// [`Manifest::fingerprint`](crate::platform::Manifest::fingerprint) 同一条道理。
    fn compute_fingerprint(&self) -> String {
        /// 一条**内容**吃完的收尾符。
        const ITEM: u64 = 0xff;
        /// 一个**表名**吃完的收尾符。**与内容那个不是同一个**：共用一个的话，
        /// 「`汉化组词` 这张表里有一条 `汉化组名`」与「`汉化组名` 那张表里有一条
        /// `汉化组名`」喂进去是同一串，而前者走子串匹配、后者走整组相等，剥出来的
        /// 正题不是一回事。
        const TABLE: u64 = 0xfe;
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |text: &str, end: u64| {
            for byte in text.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
            // 收尾符不能省：`["ab", "c"]` 与 `["a", "bc"]` 拼起来是同一串。
            hash ^= end;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        };
        // **每张表报一次名**：不然「`汉化组词` 里有一条 `汉化`、`记号噪音` 里没有」与
        // 反过来那一份，折出来是同一串，而它们剥出来的正题不是同一回事。
        //
        // **版本模式与容量模式折小写、按内容排序再吃**：这两张表只拿来判断「有没有一条
        // 对得上」（`classify_marker`），谁排在前面不影响剥出来的正题；而模式里的字面量
        // 本来就大小写不敏感（`platform::pattern` 编译时逐字折小写），`V#*` 与 `v#*`
        // 是同一条模式，指纹跟着大小写变就是白重采一整趟。别的几张表要么编译时本来就
        // 排过序（`汉化组词`、`编号分隔`），要么次序自己有意义（`正题噪音词` 长的在前、
        // `平台前缀` 长的在前），照它们现在的样子走。
        eat("汉化组词", TABLE);
        for word in &self.team_words {
            eat(word, ITEM);
        }
        eat("汉化组名", TABLE);
        for name in &self.team_names {
            eat(name, ITEM);
        }
        eat("语言地区", TABLE);
        for language in &self.languages {
            eat(language, ITEM);
        }
        eat("版本模式", TABLE);
        for source in 折小写(&self.versions) {
            eat(&source, ITEM);
        }
        eat("容量模式", TABLE);
        for source in 折小写(&self.sizes) {
            eat(&source, ITEM);
        }
        eat("记号噪音", TABLE);
        for noise in &self.marker_noise {
            eat(noise, ITEM);
        }
        eat("正题噪音词", TABLE);
        for noise in &self.title_noise {
            eat(noise, ITEM);
        }
        eat("分类词", TABLE);
        for genre in &self.genres {
            eat(genre, ITEM);
        }
        eat("平台前缀", TABLE);
        for prefix in &self.platform_prefixes {
            eat(prefix, ITEM);
        }
        eat("编号分隔", TABLE);
        for separator in &self.numbering {
            eat(separator.encode_utf8(&mut [0u8; 4]), ITEM);
        }
        format!("{hash:016x}")
    }

    /// 剥一个文件名。
    #[must_use]
    pub fn parse(&self, file_name: &str) -> Parsed {
        let mut out = Parsed::default();
        let stem = self.strip_extensions(file_name, &mut out);
        let body = self.take_markers(&stem, &mut out);
        let title = self.clean_title(&body, &mut out);
        out.chinese = han_part(&title);
        out.latin = latin_part(&title);
        out.title = title;
        out
    }
}

/// 一个文件名剥完之后剩下什么。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parsed {
    /// **正题**：剥掉记号、编号、平台前缀与噪音词之后剩下的那一段。
    pub title: String,
    /// 正题里的中文部分；正题里没有汉字时是 `None`。
    pub chinese: Option<String>,
    /// 正题里的拉丁字母部分；短到没有意义（少于三个字符）时是 `None`。
    pub latin: Option<String>,
    /// **汉化组**：署名那一组记号，原样。
    pub team: Option<String>,
    /// 版本记号，原样。
    pub version: Option<String>,
    /// 认出来的语言与地区记号。
    pub languages: Vec<String>,
    /// 名字里明写着的四位年份。**只认整组就是四位数字的那种**（`(1990)`）——
    /// `2021汉化修复版` 里那个 2021 是汉化的年份，不是发行的年份，认了就是编数据。
    pub year: Option<u16>,
    /// 剥掉了什么、按哪条规则。**依据**要靠它写。
    pub stripped: Vec<Strip>,
    /// **归不了类的记号组。** 维护者照着它往配置里补规则。
    pub unknown: Vec<String>,
}

impl Parsed {
    /// 拿去撞中文数据源的那几串字，按可信程度排好。
    ///
    /// 去重后最多两条：正题本身，加上正题里的中文部分（`PCSE00769-超级食肉男` 的中文
    /// 部分才撞得上）。整条正题里**一个汉字假名都没有**时，那一条就是拉丁名本身
    /// （`ACG_Summer_Holiday`）。
    ///
    /// **拉丁那一段绝不单独拿去撞。** 正题整条都是拉丁字母时，第一条查询本来就是它；
    /// 而正题里有汉字时，那一段几乎总是副标题的残片，残片撞出来的全是噪音——真机实测
    /// `最终幻想VI Advance` 的「VI Advance」撞上了 `双截龙Advance`，两个游戏共有的
    /// 只是 `Advance` 这个词。[`Self::latin`] 照旧留着，它是给人看的。
    #[must_use]
    pub fn queries(&self) -> Vec<(&'static str, &str)> {
        let mut out: Vec<(&'static str, &str)> = Vec::new();
        for (label, value) in [
            ("正题", Some(self.title.as_str())),
            ("正题里的中文", self.chinese.as_deref()),
        ] {
            let Some(value) = value else { continue };
            if value.trim().is_empty() || out.iter().any(|(_, seen)| *seen == value) {
                continue;
            }
            out.push((label, value));
        }
        out
    }
}

/// 剥掉的一样东西。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Strip {
    /// 剥掉的那一段，原样。
    pub what: String,
    /// 按哪条规则剥的。
    pub why: Why,
}

/// 一段是按哪条规则剥掉的。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Why {
    /// 扩展名。
    Extension,
    /// **汉化组**署名。
    Team,
    /// 版本。
    Version,
    /// 语言或地区。
    Language,
    /// 容量。
    Size,
    /// 记号噪音（整组比对）。
    Marker,
    /// 正题两头的噪音词。
    Noise,
    /// 整理包留下的编号前缀。
    Numbering,
    /// 平台前缀。
    PlatformPrefix,
    /// 分类前缀或尾巴。
    Genre,
    /// 年份。**它被记下来而不是丢掉**——交叉校验要用。
    Year,
    /// 归不了类的记号组。
    Unknown,
}

impl Why {
    /// 依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Extension => "扩展名",
            Self::Team => "汉化组",
            Self::Version => "版本",
            Self::Language => "语言地区",
            Self::Size => "容量",
            Self::Marker => "记号噪音",
            Self::Noise => "噪音词",
            Self::Numbering => "编号前缀",
            Self::PlatformPrefix => "平台前缀",
            Self::Genre => "分类",
            Self::Year => "年份",
            Self::Unknown => "认不出的记号",
        }
    }
}

/// 四种括号。**结构性的东西不进配置**：库里就这四种写法，而把括号交给配置只会让
/// 一份写坏的配置把整批名字剥成空串。
const BRACKETS: [(char, char); 4] = [('[', ']'), ('(', ')'), ('【', '】'), ('（', '）')];

impl Rules {
    /// 剥掉扩展名。`.cia.zip`、`.tar.zst`、`.gba.zip` 都是真库里的写法，所以剥好几层。
    ///
    /// **判据是「一到五个 ASCII 字母数字，而且里面得有个字母」**：`v1.8B（2023.2.1）`
    /// 结尾那个 `1）` 不是扩展名，`CT特种部队3v1.1` 结尾那个 `1` 也不是——按「最后一个点
    /// 之后的东西」一刀切，这两种名字都会被剪掉半截。分卷的 `.001` 是唯一的例外，
    /// 它确实是扩展名（7-Zip 的通用 Split，ADR-0014）。
    fn strip_extensions(&self, name: &str, out: &mut Parsed) -> String {
        let mut stem = name.trim().to_string();
        for _ in 0..3 {
            let Some((head, ext)) = stem.rsplit_once('.') else {
                break;
            };
            let ok = !head.trim().is_empty()
                && (1..=5).contains(&ext.chars().count())
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && (ext.chars().any(|c| c.is_ascii_alphabetic())
                    || (ext.chars().count() == 3 && ext.chars().all(|c| c.is_ascii_digit())));
            if !ok {
                break;
            }
            out.stripped.push(Strip {
                what: format!(".{ext}"),
                why: Why::Extension,
            });
            stem = head.to_string();
        }
        stem
    }

    /// 把记号组整组切下来并逐组归类，返回剩下的那些字。
    fn take_markers(&self, stem: &str, out: &mut Parsed) -> String {
        let mut body = String::new();
        let mut rest = stem;
        // 最靠前的那个开括号。四种括号混着用，得一起找。
        while let Some((at, open, close)) = BRACKETS
            .iter()
            .filter_map(|(open, close)| rest.find(*open).map(|at| (at, *open, *close)))
            .min_by_key(|(at, _, _)| *at)
        {
            let after = &rest[at + open.len_utf8()..];
            let Some(end) = after.find(close) else {
                // 有开无合：后面没有完整的组了，剩下的整段留给正题。
                break;
            };
            let group = &after[..end];
            // **整个名字就是一组记号**时原样留着：`【真·三国无双】` 那种写法真实存在，
            // 剥成空串比留着记号糟得多（与 `naming::work_title` 同一条道理）。
            let whole =
                at == 0 && after[end + close.len_utf8()..].trim().is_empty() && body.is_empty();
            if whole {
                body.push_str(group);
            } else {
                body.push_str(&rest[..at]);
                self.classify_marker(group, out);
            }
            rest = &after[end + close.len_utf8()..];
        }
        body.push_str(rest);
        body
    }

    /// 一组记号是什么。
    fn classify_marker(&self, group: &str, out: &mut Parsed) {
        let trimmed = group.trim();
        if trimmed.is_empty() {
            return;
        }
        let key = fold(trimmed);
        let record = |out: &mut Parsed, why: Why| {
            out.stripped.push(Strip {
                what: trimmed.to_string(),
                why,
            });
        };
        // 一、**汉化组**。它排第一，因为署名里什么都可能有——`[简体汉化版]` 同时含着
        // 语言词与噪音词，而它说的是「这是一份汉化」，那是这个库里最值钱的一条信息。
        if self.team_names.contains(&key) || self.team_words.iter().any(|word| key.contains(word)) {
            if out.team.is_none() {
                out.team = Some(trimmed.to_string());
            }
            record(out, Why::Team);
            return;
        }
        // 二、四位年份。**只认整组就是四位数字的**（见 [`Parsed::year`]）。
        if let Some(year) = four_digit_year(trimmed) {
            if out.year.is_none() {
                out.year = Some(year);
            }
            record(out, Why::Year);
            return;
        }
        // 三、版本。
        if self.versions.iter().any(|pattern| pattern.matches(trimmed)) {
            if out.version.is_none() {
                out.version = Some(trimmed.to_string());
            }
            record(out, Why::Version);
            return;
        }
        // 四、语言与地区。
        if self.languages.contains(&key) {
            out.languages.push(trimmed.to_string());
            record(out, Why::Language);
            return;
        }
        // 五、容量。
        if self.sizes.iter().any(|pattern| pattern.matches(trimmed)) {
            record(out, Why::Size);
            return;
        }
        // 六、整组噪音。
        if self.marker_noise.contains(&key) {
            record(out, Why::Marker);
            return;
        }
        // 七、整组就是一个平台词：`[NGC][0510]太空堡垒` 那种整理包的写法。
        if self.platform_prefixes.contains(&key) {
            record(out, Why::PlatformPrefix);
            return;
        }
        // 八、整组就是一串数字：同一批名字里的收录编号。**年份上面已经认掉了**，
        // 走到这儿的是 `0510` 这种。
        if key.chars().all(|c| c.is_ascii_digit()) {
            record(out, Why::Numbering);
            return;
        }
        // 九、整组就是一个分类：`[AVG]`、`[SLG]`、`[角色扮演]`。这个库里的整理者
        // 成批地把分类写在方括号里。
        if self.genres.contains(&key) {
            record(out, Why::Genre);
            return;
        }
        // 十、**归不了类**。照样剥掉——记号组里几乎不会有正题——但要说出来，
        // 那是维护者补规则的依据。
        out.unknown.push(trimmed.to_string());
        record(out, Why::Unknown);
    }

    /// 把剩下的那些字清成正题。
    fn clean_title(&self, body: &str, out: &mut Parsed) -> String {
        let mut title = repair_dots(body).trim().to_string();
        // 一、编号前缀：`033.`、`1741 - `、`3.`。**光有数字不算**——`1942` 是真游戏名。
        if let Some(rest) = self.strip_numbering(&title) {
            out.stripped.push(Strip {
                what: title[..title.len() - rest.len()].to_string(),
                why: Why::Numbering,
            });
            title = rest;
        }
        // 二、分类前缀：`動作：`。
        if let Some((genre, rest)) = self.strip_genre_prefix(&title) {
            out.stripped.push(Strip {
                what: genre,
                why: Why::Genre,
            });
            title = rest;
        }
        // 三、平台前缀：`GBC海贼王`。
        if let Some((prefix, rest)) = self.strip_platform_prefix(&title) {
            out.stripped.push(Strip {
                what: prefix,
                why: Why::PlatformPrefix,
            });
            title = rest;
        }
        // 四、尾巴上那串 TitleID：`PROJECT X ZONE 0004008c00073900`。
        if let Some((id, rest)) = self.strip_title_id_tail(&title) {
            out.stripped.push(Strip {
                what: id,
                why: Why::Numbering,
            });
            title = rest;
        }
        // 五、尾巴上由 `-` / `_` 分出来的那几段：`-美版-休闲益智`、`_日文`、`_v97`。
        title = self.strip_tail_segments(&title, out);
        // 六、正题两头的噪音词：`唐老鸭汉化版` → `唐老鸭`。
        title = self.strip_noise_words(&title, out);
        title
            .trim_matches(|c: char| c.is_whitespace() || is_edge_punctuation(c))
            .to_string()
    }

    /// 开头那串编号加一个分隔符。
    fn strip_numbering(&self, title: &str) -> Option<String> {
        let digits: String = title.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || digits.chars().count() > 4 {
            return None;
        }
        let rest = &title[digits.len()..];
        let separator = rest.chars().next()?;
        if !self.numbering.contains(&separator) {
            return None;
        }
        let rest = rest[separator.len_utf8()..].trim_start();
        // 剥完得剩下东西，而且**不能只剩一串数字**：`3.2` 是版本号不是编号加标题。
        if rest.is_empty() || rest.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        Some(rest.to_string())
    }

    /// 开头的 `分类：`。
    fn strip_genre_prefix(&self, title: &str) -> Option<(String, String)> {
        let at = title.find(['：', ':'])?;
        let head = &title[..at];
        if !self.genres.contains(&fold(head.trim())) {
            return None;
        }
        let rest = title[at..].trim_start_matches(['：', ':']).trim_start();
        if rest.is_empty() {
            return None;
        }
        Some((title[..at].to_string(), rest.to_string()))
    }

    /// 开头的平台词。
    ///
    /// **后面必须紧跟着汉字、或者隔着一个分隔符**，这一条是要害：`snes9x_next_libretro`
    /// 与 `PSPDLC120 - GripShift` 里那两个前缀是名字自己的一部分，剥掉就把名字弄坏了。
    /// 剥完还得剩下两个字以上——`GBA` 单独一个名字剥完就空了。
    fn strip_platform_prefix(&self, title: &str) -> Option<(String, String)> {
        for prefix in &self.platform_prefixes {
            // **按字符数取前缀再折**，不拿折过的字节长度回头去切原串：`fold` 会小写、
            // 会规范化，折出来的字节长度与原串不是一回事，照它切迟早切进一个字符中间。
            let count = prefix.chars().count();
            let head: String = title.chars().take(count).collect();
            if head.chars().count() < count || fold(&head) != *prefix {
                continue;
            }
            let tail = &title[head.len()..];
            let next = tail.chars().next()?;
            if !(is_cjk(next) || SEPARATORS.contains(&next)) {
                continue;
            }
            let rest = tail.trim_start_matches([' ', '-', '_', '\u{3000}']);
            if rest.chars().count() < 2 {
                continue;
            }
            return Some((head, rest.to_string()));
        }
        None
    }

    /// 尾巴上那串 TitleID / 内容 ID：`名探偵コナン … 0004008c00118100`、
    /// `PROJECT X ZONE 0004008c00073900`。3DS 与 Switch 的转储成批地这么起名。
    ///
    /// 判据是「最后一段是八位以上的十六进制」——短的那些是标题里的数字（`游戲王1`）。
    fn strip_title_id_tail(&self, title: &str) -> Option<(String, String)> {
        let (head, tail) = title.trim_end().rsplit_once([' ', '_', '\u{3000}'])?;
        let hex = tail.len() >= 8 && tail.chars().all(|c| c.is_ascii_hexdigit());
        if !hex || head.trim().is_empty() {
            return None;
        }
        Some((tail.to_string(), head.trim_end().to_string()))
    }

    /// 尾巴上由 `-` / `_` 分出来的最后几段，整段是语言、分类、版本、容量或噪音词就剥掉。
    fn strip_tail_segments(&self, title: &str, out: &mut Parsed) -> String {
        let mut current = title.to_string();
        for _ in 0..4 {
            let Some(at) = current.rfind(['-', '_', '－']) else {
                break;
            };
            let Some(separator) = current[at..].chars().next() else {
                break;
            };
            let head = &current[..at];
            // **分隔符可能不止一个字节**：`－` 是三个。按 `at + 1` 切会切进字符中间。
            let segment = current[at + separator.len_utf8()..].trim();
            if head.trim().is_empty() || segment.is_empty() {
                break;
            }
            let key = fold(segment);
            let why = if self.languages.contains(&key) {
                Why::Language
            } else if self.genres.contains(&key) {
                Why::Genre
            } else if self.title_noise.contains(&key) {
                Why::Noise
            } else if self.versions.iter().any(|pattern| pattern.matches(segment)) {
                Why::Version
            } else if self.sizes.iter().any(|pattern| pattern.matches(segment)) {
                Why::Size
            } else {
                break;
            };
            if why == Why::Language {
                out.languages.push(segment.to_string());
            }
            out.stripped.push(Strip {
                what: segment.to_string(),
                why,
            });
            current = head.trim_end().to_string();
        }
        current
    }

    /// 正题两头的噪音词，反复剥。**只剥两头，绝不从中间挖**——`中文课堂` 里那两个字
    /// 是标题的一部分，从中间挖会把它变成 `课堂`。
    ///
    /// 比对按**字符**走而不是按字节：折过的串与原串的字节长度不是一回事，
    /// 拿折出来的长度回头去切原串，迟早切进一个字符中间。
    fn strip_noise_words(&self, title: &str, out: &mut Parsed) -> String {
        let mut current = title.trim().to_string();
        for _ in 0..4 {
            let total = current.chars().count();
            let mut hit = None;
            for noise in &self.title_noise {
                let count = noise.chars().count();
                if total <= count {
                    continue;
                }
                let head: String = current.chars().take(count).collect();
                let tail: String = current.chars().skip(total - count).collect();
                if fold(&tail) == *noise {
                    hit = Some((current.len() - tail.len(), current.len(), noise.clone()));
                    break;
                }
                if fold(&head) == *noise {
                    hit = Some((0, head.len(), noise.clone()));
                    break;
                }
            }
            let Some((from, to, noise)) = hit else { break };
            let mut next = current.clone();
            next.replace_range(from..to, "");
            let next = next
                .trim_matches(|c: char| c.is_whitespace() || is_edge_punctuation(c))
                .to_string();
            // 剥完不能只剩一个字——那多半是把标题本身剥掉了。
            if next.chars().count() < 2 {
                break;
            }
            out.stripped.push(Strip {
                what: noise,
                why: Why::Noise,
            });
            current = next;
        }
        current
    }
}

/// 一张模式表折成**指纹认得的样子**：逐字折小写（与 [`Pattern::new`] 编译时那一下
/// 一模一样），再按内容排序去重。
///
/// 折小写这一步不能省：模式里的字面量本来就大小写不敏感，`V#*` 与 `v#*` 是同一条模式，
/// 指纹跟着大小写变就是白重采一整趟——而 `BTreeSet` 也去不掉那一对的重。
fn 折小写(patterns: &[Pattern]) -> BTreeSet<String> {
    patterns
        .iter()
        .map(|pattern| {
            pattern
                .source()
                .chars()
                .map(|ch| ch.to_lowercase().next().unwrap_or(ch))
                .collect()
        })
        .collect()
}

/// 两个汉字之间那个点，去掉。
///
/// `海.贼.王：梦之路飞海.贼团诞生`、`PSV超级机器人大.战V` 是真库里的写法——资源站为了
/// 躲关键词过滤在词中间插点。**只在两边都是汉字时去**：`v1.0` 与 `k73.com` 里的点
/// 是真的点，一并去掉就把别的名字弄坏了。
fn repair_dots(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (index, ch) in chars.iter().enumerate() {
        let drop = (*ch == '.' || *ch == '．')
            && index > 0
            && index + 1 < chars.len()
            && is_han(chars[index - 1])
            && is_han(chars[index + 1]);
        if !drop {
            out.push(*ch);
        }
    }
    out
}

/// 整组正好是四位数字、而且像个年份。
fn four_digit_year(group: &str) -> Option<u16> {
    let text = group.trim();
    if text.chars().count() != 4 || !text.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let year: u16 = text.parse().ok()?;
    (1970..=2100).contains(&year).then_some(year)
}

/// 正题里的中文部分：含汉字或假名的那几段，接起来。
fn han_part(title: &str) -> Option<String> {
    let mut out = String::new();
    for segment in title.split(|c: char| SEPARATORS.contains(&c)) {
        if segment.chars().any(is_cjk) {
            out.push_str(segment.trim());
        }
    }
    (!out.is_empty() && out != title)
        .then_some(out)
        .or_else(|| {
            // 整条正题本来就是中文时不必另给一条——[`Parsed::queries`] 会去重。
            title.chars().any(is_cjk).then(|| title.to_string())
        })
}

/// 正题里的拉丁字母部分：不含汉字与假名的那几段，接起来。
fn latin_part(title: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for segment in title.split(|c: char| SEPARATORS.contains(&c)) {
        let segment = segment.trim();
        if !segment.is_empty() && !segment.chars().any(is_cjk) {
            parts.push(segment);
        }
    }
    let joined = parts.join(" ");
    // 太短的一段不成其为查询：`R`、`v2`、`CN` 撞出来的只会是噪音。
    let letters = joined.chars().filter(char::is_ascii_alphabetic).count();
    (letters >= 3).then_some(joined)
}

/// 拆段用的分隔符。
const SEPARATORS: [char; 10] = [' ', '\u{3000}', '-', '_', '/', '|', '·', '、', ',', '，'];

/// 两头可以修掉的标点。
fn is_edge_punctuation(c: char) -> bool {
    matches!(
        c,
        '-' | '_'
            | '.'
            | ','
            | ':'
            | ';'
            | '~'
            | '·'
            | '、'
            | '，'
            | '：'
            | '；'
            | '～'
            | '+'
            | '＋'
            | '－'
            | '＿'
            | '．'
    )
}

/// 平台名折成可比较的形式：小写、去掉空格、连字符与点。
///
/// [`crate::zh::sync::platform_of`] 也要它——「`Wii U` 与 `wiiu` 是同一个平台」这条
/// 判据两处各写一遍，迟早会漂开。
#[must_use]
pub fn fold_platform(text: &str) -> String {
    fold(text)
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '.' && *c != '_')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 规则() -> Rules {
        Rules::builtin()
    }

    #[test]
    fn 内置规则自洽() {
        // `builtin()` 会 panic 的话，整个工具第一条命令就起不来。
        let rules = 规则();
        assert!(!rules.title_noise.is_empty());
        assert!(!rules.versions.is_empty());
    }

    #[test]
    fn 剥掉汉化组名版本号与噪音词() {
        // 票据的第一条验收，三样一句话里全占了。真库里的名字。
        let parsed = 规则().parse("超级机器人大战R[星组](v1.2+)(简)(JP)(68.92Mb).zip");
        assert_eq!(parsed.title, "超级机器人大战R");
        assert_eq!(parsed.team.as_deref(), Some("星组"));
        assert_eq!(parsed.version.as_deref(), Some("v1.2+"));
        assert_eq!(parsed.languages, vec!["简", "JP"]);
        assert!(parsed.unknown.is_empty(), "{:?}", parsed.unknown);
    }

    #[test]
    fn 汉化组认的是词不是一张写死的名单() {
        // 名单永远追不上——判据是「组里含『汉化』这类词」，所以没见过的组也认得出。
        let parsed = 规则().parse("我的暑假[简体汉化版][某某某汉化组]");
        assert_eq!(parsed.title, "我的暑假");
        assert_eq!(parsed.team.as_deref(), Some("简体汉化版"));
    }

    #[test]
    fn 编号前缀与分类前缀都剥得掉() {
        let parsed = 规则().parse("71.智力：仙界異聞録.zip");
        assert_eq!(parsed.title, "仙界異聞録");
        let parsed = 规则().parse("1741 - 梦想灯笼 [简] [PSPchina].7z");
        assert_eq!(parsed.title, "梦想灯笼");
    }

    #[test]
    fn 编号剥掉的前提是后面跟着分隔符() {
        // `1942` 与 `2048` 是真游戏名，剥掉就什么都不剩了。
        assert_eq!(规则().parse("1942.nes").title, "1942");
        assert_eq!(规则().parse("2048.zip").title, "2048");
    }

    #[test]
    fn 平台前缀剥得掉而单独一个平台名留得住() {
        assert_eq!(规则().parse("GBC海贼王.cia.zip").title, "海贼王");
        assert_eq!(
            规则().parse("FC唐老鸭大冒险2汉化.zip").title,
            "唐老鸭大冒险2"
        );
        // 剥完就空了的一律不剥。
        assert_eq!(规则().parse("GBA.zip").title, "GBA");
    }

    #[test]
    fn 平台前缀后面得跟着汉字或者一个分隔符() {
        // 这一条防的是把名字自己的一部分当成前缀剥掉。两个都是真库里的名字。
        assert_eq!(
            规则().parse("snes9x_next_libretro.cia").title,
            "snes9x_next_libretro"
        );
        assert_eq!(
            规则()
                .parse("PSPDLC120 - GripShift - Level Pack (Europe).rar")
                .title,
            "PSPDLC120 - GripShift - Level Pack"
        );
        assert_eq!(规则().parse("FC CHINESE.7z").title, "CHINESE");
    }

    #[test]
    fn 尾巴上那串_titleid_剥得掉() {
        // 3DS 与 Switch 的转储成批地这么起名。
        assert_eq!(
            规则()
                .parse("PROJECT X ZONE 0004008c00073900.tar.zst")
                .title,
            "PROJECT X ZONE"
        );
        // 短的那些是标题里的数字，不许动。
        assert_eq!(规则().parse("游戲王 1.zip").title, "游戲王 1");
    }

    #[test]
    fn 整组是平台词或者一串编号的记号剥得掉() {
        // `[NGC][0510]太空堡垒 Robotech Battle Cry（美）` 是真库里 NGC 那一批的写法。
        let parsed = 规则().parse("[NGC][0510]太空堡垒 Robotech Battle Cry（美）.7z");
        assert_eq!(parsed.title, "太空堡垒 Robotech Battle Cry");
        assert!(parsed.unknown.is_empty(), "{:?}", parsed.unknown);
    }

    #[test]
    fn 扩展名得有个字母() {
        // `CT特种部队3v1.1` 结尾那个 `1` 不是扩展名。
        assert_eq!(规则().parse("CT特种部队3v1.1.zip").title, "CT特种部队3v1.1");
        // 分卷的 `.001` 是唯一的例外，它确实是扩展名。
        assert_eq!(规则().parse("游戏.7z.001").title, "游戏");
    }

    #[test]
    fn 词中间插的点补得回来() {
        // 资源站为躲关键词过滤插的点，真库里成批出现。
        let parsed = 规则().parse("GBC海.贼.王：梦之路飞海.贼团诞生.zip");
        assert_eq!(parsed.title, "海贼王：梦之路飞海贼团诞生");
        // 版本号里的点、域名里的点**不许动**。
        assert_eq!(规则().parse("k73.com_wbfstoiso").title, "k73.com_wbfstoiso");
    }

    #[test]
    fn 尾巴上的语言与分类段剥得掉() {
        let parsed = 规则().parse("PCSE00769-超级食肉男-美版-休闲益智.VPK.tar.zst");
        assert_eq!(parsed.title, "PCSE00769-超级食肉男");
        assert_eq!(parsed.chinese.as_deref(), Some("超级食肉男"));
        assert_eq!(
            规则().parse("S山脊赛车_日文.vpk.tar.zst").title,
            "S山脊赛车"
        );
    }

    #[test]
    fn 多层扩展名剥得干净而名字里的点留得住() {
        assert_eq!(规则().parse("东方ProjectDS.tar.zst").title, "东方ProjectDS");
        // `v1.8B（2023.2.1）` 结尾那个 `1）` 不是扩展名——按「最后一个点之后」一刀切
        // 会把它剪成 `PSV游戏大全v1.8B（2023.2`。（`PSV` 是平台前缀，另一条规则剥的。）
        assert_eq!(
            规则().parse("PSV游戏大全v1.8B（2023.2.1）").title,
            "游戏大全v1.8B"
        );
    }

    #[test]
    fn 整个名字就是一组记号时原样留着() {
        // `【真·三国无双】` 这种写法真实存在，剥成空串比留着记号糟得多。
        assert_eq!(规则().parse("【真·三国无双】.zip").title, "真·三国无双");
        assert_eq!(规则().parse("(Unknown).zip").title, "Unknown");
    }

    #[test]
    fn 认不出的记号要说出来() {
        // 这一栏是维护者补规则的依据，悄悄剥掉就没人知道该补什么。
        let parsed = 规则().parse("怪物召唤士[Fhz](Beta)(简)(JP)(64Mb)(某个没见过的记号).zip");
        assert_eq!(parsed.title, "怪物召唤士");
        assert_eq!(parsed.unknown, vec!["某个没见过的记号"]);
    }

    #[test]
    fn 只认整组就是四位数字的年份() {
        // `2021汉化修复版` 里那个 2021 是汉化的年份，认了就是编数据。
        assert_eq!(规则().parse("Foo (1990) (JP).zip").year, Some(1990));
        assert_eq!(规则().parse("地球冒险3 2021汉化修复版").year, None);
        assert_eq!(规则().parse("Foo (12345).zip").year, None);
    }

    #[test]
    fn 中文与拉丁两段分得开() {
        let parsed = 规则().parse("Dragon Ball Z Gaiden - Saiya Jin (CN)_海美譯名版.zip");
        assert_eq!(
            parsed.latin.as_deref(),
            Some("Dragon Ball Z Gaiden Saiya Jin")
        );
        assert_eq!(parsed.chinese.as_deref(), Some("海美譯名版"));
    }

    #[test]
    fn 拉丁那一段不单独拿去撞() {
        // 真机实测：`最终幻想VI Advance` 的「VI Advance」撞上了 `双截龙Advance`——
        // 两个游戏共有的只是 `Advance` 这个词。
        let parsed = 规则().parse("最终幻想VI Advance[天幻网](v1.1)(简)(US).zip");
        let labels: Vec<&str> = parsed.queries().iter().map(|(label, _)| *label).collect();
        assert_eq!(labels, vec!["正题", "正题里的中文"]);
        // 整条正题都是拉丁字母时，第一条查询本来就是它。
        let parsed = 规则().parse("ACG_Summer_Holiday.7z");
        assert_eq!(parsed.queries().len(), 1);
        assert_eq!(parsed.queries()[0].1, "ACG_Summer_Holiday");
    }

    #[test]
    fn 用户的规则是补充不是换掉() {
        let dir = crate::testing::temp_dir("name-rules");
        let path = dir.path().join("rules.toml");
        std::fs::write(&path, "\"版本\" = 1\n\"汉化组名\" = [\"某新组\"]\n").expect("能写");
        let rules = Rules::load(&path).expect("读得进来");
        // 自己加的认得出。
        assert_eq!(
            rules.parse("某游戏[某新组].zip").team.as_deref(),
            Some("某新组")
        );
        // 内置的那些**一条都没丢**——这正是「补充」与「换掉」的分界。
        assert_eq!(规则().parse("71.智力：仙界異聞録.zip").title, "仙界異聞録");
        assert_eq!(rules.parse("71.智力：仙界異聞録.zip").title, "仙界異聞録");
    }

    #[test]
    fn 整份换掉也做得到() {
        let dir = crate::testing::temp_dir("name-rules-replace");
        let path = dir.path().join("rules.toml");
        std::fs::write(
            &path,
            "\"版本\" = 1\n\"继承内置\" = false\n\"汉化组名\" = [\"某新组\"]\n",
        )
        .expect("能写");
        let rules = Rules::load(&path).expect("读得进来");
        // 内置那几张表全没了——用户说了「换掉」：编号前缀与分类前缀都不再剥。
        assert_eq!(
            rules.parse("71.智力：仙界異聞録.zip").title,
            "71.智力：仙界異聞録"
        );
    }

    #[test]
    fn 版本对不上就不收() {
        let dir = crate::testing::temp_dir("name-rules-version");
        let path = dir.path().join("rules.toml");
        std::fs::write(&path, "\"版本\" = 99\n").expect("能写");
        assert!(matches!(
            Rules::load(&path),
            Err(RulesError::Version { found: 99, .. })
        ));
    }

    #[test]
    fn 用户改得动内置的那一条平台别名() {
        // 补新条目谁都做得到，**改内置那一条**才是「继承」与「覆盖」的分界。
        let dir = crate::testing::temp_dir("name-rules-alias");
        let path = dir.path().join("rules.toml");
        std::fs::write(
            &path,
            "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"Nintendo Switch\"\n\"是\" = \"NS\"\n",
        )
        .expect("能写");
        let rules = Rules::load(&path).expect("读得进来");
        assert_eq!(rules.platform_alias("Nintendo Switch"), Some("NS"));
        // 没改的那些照旧。
        assert_eq!(rules.platform_alias("PS Vita"), Some("PSV"));
    }

    #[test]
    fn 中文源的平台名折得回本工具的平台名() {
        let rules = 规则();
        assert_eq!(rules.platform_alias("Nintendo Switch"), Some("SWITCH"));
        assert_eq!(rules.platform_alias("PlayStation Vita"), Some("PSV"));
        assert_eq!(rules.platform_alias("PS Vita"), Some("PSV"));
        // 平台清单那张目录别名表折得动的，这里不必重复——认不出返回 `None`，
        // 由调用方去问平台清单。
        assert_eq!(rules.platform_alias("GBA"), None);
    }

    #[test]
    fn 多字节的分隔符与噪音词不会切进字符中间() {
        // 真机上这一条是崩出来的：`－` 是三个字节，而尾段那一步按「分隔符一个字节」
        // 切。切进字符中间在 Rust 里是 panic，一条名字就能停掉整趟识别。
        assert_eq!(规则().parse("塞尔达传说－日文.zip").title, "塞尔达传说");
        assert_eq!(规则().parse("游戏－－繁体").title, "游戏");
    }

    #[test]
    fn 什么样的名字都不许崩() {
        // 名字是盘上那些人起的，这一层对它没有任何控制权。**崩一次就停掉整趟识别**，
        // 而那一趟要跑四万多个变体。
        for name in [
            "",
            ".",
            "..",
            "...",
            "－",
            "－－－",
            "。。。",
            "①②③.zip",
            "🎮游戏－汉化.zip",
            "Ｖ１．０",
            "[][][]",
            "((((",
            "【】",
            "\u{fffd}\u{fffd}.nes",
            "a",
            "中",
            "中文",
            "GBA",
            "简",
            "1",
            "1.",
            "游戏.tar.zst.7z.zip",
            "  　 ",
            "-_-_-",
            "游戏（简）（繁）（日）",
        ] {
            let parsed = 规则().parse(name);
            // 剥出来的东西必须是原名字里真有的字符——不许凭空造字。
            assert!(
                parsed.title.chars().count() <= name.chars().count() + 1,
                "{name}"
            );
        }
    }

    #[test]
    fn 剥掉的每一样都记下来了() {
        let parsed = 规则().parse("我的暑假[简体汉化版][ACG汉化组].7z");
        let whys: Vec<Why> = parsed.stripped.iter().map(|it| it.why).collect();
        assert!(whys.contains(&Why::Extension));
        assert!(whys.contains(&Why::Team));
    }

    /// 一份**继承内置**的规则，从一段 TOML 编出来。
    fn 补一份(text: &str) -> Rules {
        Rules::parse_text(text, "（测试）").expect("编得出来")
    }

    #[test]
    fn 补一条剥离规则指纹就变() {
        // 规则是**配置不是代码**：用户补一条自己遇到的模式，剥出来的正题就变了，
        // 撞出来的中文条目也可能跟着变——所以它进刮削那两层锚点的输入指纹
        // （`scrape::zh` 的两个 `probe`）。不进的话，用户调完规则重跑一趟，
        // 缓存会一口咬定「输入没变」而整条跳过：什么都不重采，而且不报错。
        let 内置 = Rules::builtin();
        assert_eq!(
            内置.fingerprint(),
            Rules::builtin().fingerprint(),
            "同一份规则指纹稳定"
        );
        let 补过 = 补一份("\"版本\" = 1\n\"正题噪音词\" = [\"甲组特供版\"]\n");
        assert_ne!(内置.fingerprint(), 补过.fingerprint());
        // **指纹该变的理由**：这两份规则剥出来的正题真的不是同一个。
        // 不钉这一条，上面那句只是在比两个字符串。
        assert_eq!(
            内置.parse("合金弹头7甲组特供版.7z").title,
            "合金弹头7甲组特供版"
        );
        assert_eq!(补过.parse("合金弹头7甲组特供版.7z").title, "合金弹头7");
    }

    #[test]
    fn 指纹算的是规则的语义不是文件的字节() {
        // **加一行注释不触发全库重采。** 指纹只认「会改变正题的那部分」：
        // 注释、空行、缩进、以及那两张只做「有没有一条对得上」判断的模式表的次序，
        // 都不该让四万多个变体重采一遍。
        let 内置 = Rules::builtin();
        let 白纸 = 补一份("\"版本\" = 1\n");
        assert_eq!(
            内置.fingerprint(),
            白纸.fingerprint(),
            "继承内置、一条都没补，那就是内置那一份"
        );
        let 加注释 = 补一份("# 这是一行注释\n\n\"版本\" = 1\n\n# 又一行\n");
        assert_eq!(内置.fingerprint(), 加注释.fingerprint());
        // 版本模式与容量模式只拿来判断「有没有一条对得上」，谁排在前面不影响正题。
        let 甲 = 补一份("\"版本\" = 1\n\"版本模式\" = [\"v#*\", \"rev #\"]\n");
        let 乙 = 补一份("\"版本\" = 1\n\"版本模式\" = [\"rev #\", \"v#*\"]\n");
        assert_eq!(甲.fingerprint(), 乙.fingerprint(), "调换次序不是改规则");
        // 模式里的字面量本来就**大小写不敏感**（`platform::pattern` 编译时逐字折小写），
        // `V#*` 与 `v#*` 是同一条模式——整理一下配置里的大小写风格不该让全库重采一遍。
        let 小写 = 补一份("\"版本\" = 1\n\"版本模式\" = [\"v#*\"]\n");
        let 大写 = 补一份("\"版本\" = 1\n\"版本模式\" = [\"V#*\"]\n");
        assert_eq!(
            小写.fingerprint(),
            大写.fingerprint(),
            "同一条模式换个大小写写法"
        );
        assert_eq!(
            小写.parse("塞尔达传说(V1.2).zip").title,
            大写.parse("塞尔达传说(V1.2).zip").title,
            "指纹该一样的理由：这两份规则剥出来的正题真的一样"
        );
    }

    #[test]
    fn 同一条词摆在不同的表里不是同一份规则() {
        // 每张表在指纹里报一次名，不然「`分类词` 里有一条甲、`记号噪音` 里没有」
        // 与反过来那一份折出来是同一串——而它们归类不同，剥出来的东西也不同。
        let 当分类 = 补一份("\"版本\" = 1\n\"分类词\" = [\"甲组特供\"]\n");
        let 当噪音 = 补一份("\"版本\" = 1\n\"记号噪音\" = [\"甲组特供\"]\n");
        assert_ne!(当分类.fingerprint(), 当噪音.fingerprint());
    }

    #[test]
    fn 一条词恰好等于下一张表的表名也分得开() {
        // **表名与内容的收尾符不是同一个**，理由就在这一条上：共用一个的话，下面这两份
        // 规则喂给 FNV 的是同一串（`汉化组词 | 汉化组名 | 汉化组名 | 语言地区 | …`），
        // 可甲那条走的是**子串匹配**（`classify_marker` 里那句 `key.contains(word)`）、
        // 乙那条走的是**整组相等**，剥出来的正题不是一回事。
        let 甲 = 补一份("\"版本\" = 1\n\"继承内置\" = false\n\"汉化组词\" = [\"汉化组名\"]\n");
        let 乙 = 补一份("\"版本\" = 1\n\"继承内置\" = false\n\"汉化组名\" = [\"汉化组名\"]\n");
        assert_ne!(甲.fingerprint(), 乙.fingerprint());
        // 指纹该不一样的理由：这两份规则剥出来的东西真的不一样。
        assert_eq!(
            甲.parse("塞尔达传说[某某汉化组名义].zip").team.as_deref(),
            Some("某某汉化组名义")
        );
        assert_eq!(乙.parse("塞尔达传说[某某汉化组名义].zip").team, None);
    }

    #[test]
    fn 中文源平台别名不进这个指纹() {
        // **有意不进。** 那张表只在建索引时用得着（`zh::sync`），它对刮削的全部影响
        // 都由建索引那一刻折出来的那一串盖着（`zh::Index::platform_fold`，已经进了
        // 两层指纹）。再盖一遍的话，「改了别名但还没重建索引」会让指纹变、可这一趟的
        // 输入一个字都没变——那是白重采一整趟。
        let 补别名 = 补一份(
            "\"版本\" = 1\n[[\"中文源平台别名\"]]\n\"叫\" = \"任天堂红白机\"\n\"是\" = \"FC\"\n",
        );
        assert_eq!(
            补别名.platform_alias("任天堂红白机"),
            Some("FC"),
            "别名真补上了"
        );
        assert_eq!(
            Rules::builtin().fingerprint(),
            补别名.fingerprint(),
            "只补别名不改正题，这份指纹就不该动"
        );
    }
}
