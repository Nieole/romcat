//! **ES gamelist 适配器**：`gamelist.xml` 的读、写与字段映射。
//!
//! 第二个**无损往返**档的适配器，而且**一个顶好几个**：调研实测确认 Cocoon、iiSU、
//! Daijishō 三家安卓前端全都复用 ES-DE 这一套（`docs/research/android-frontends.md`
//! §8.1），做透这一个，「多设备并用」当场成立；中文社区分享的现成元数据包也大多是
//! 这个格式，「吸收现成元数据」一起解决。
//!
//! ## ⚠️ 这个格式的文件**技术上不是合法 XML**
//!
//! ES-DE 把 `<alternativeEmulator>` 写在 `<gameList>` **外面**，于是一份 gamelist
//! 有**两个根元素**（`docs/research/metadata-formats.md` §B.1 源码补充第 8 条，
//! ES-DE 源码注释自己承认了这点）。**标准 XML 解析器怼上去直接失败**——
//! Skyscraper 的 `esde.cpp` 为此专门留了一段 `Coding Horror` 注释绕过去。
//!
//! 这里的办法是**不用文档级解析器**：自己写一遍词法，把文件拆成一串
//! [`Node`]。词法根本没有「根元素只许一个」这条规矩，两个根、三个根、根前面还有
//! 注释与 `<?xml?>`，都只是节点。合法性由 ES-DE 定义，不由 XML 规范定义。
//!
//! ## ⚠️ **用户状态就长在同一个文件里**
//!
//! `favorite` / `playcount` / `lastplayed` / `playtime` / `completed` 全在 `<game>`
//! 里边。按 ADR-0006 工具**既不生成也不覆盖**，但在这里**「不碰」不是省略而是搬运**：
//! 导出时若省略这些元素，等于把维护者多年的收藏与游玩记录**清零**。
//!
//! 落地办法：这些元素**一个都不折进** [`Game`]——中立模型里没有它们的位置，于是
//! 工具连「生成一个」的路径都没有；它们只躺在快照里，导出时随着**没变的那些行**
//! 一起原样搬回去。[`Preserved::user_state`] 数的就是搬了几处，那是证据。
//!
//! ## 为什么快照存的是拆开的结构而不是原文
//!
//! 与 [`pegasus`](super::pegasus) 同一条纪律：一个只会 `memcpy` 原文的实现也能
//! 「通过」往返，那证不了任何事。所以 [`Snapshot`] 存的是拆到零件的节点——标签名、
//! 每个属性的前导空白与引号字符、`>` 之前的空白、自闭合与否、标签之外的原文。
//! 渲染时从零件重新拼回去，于是「逐字节相同」证的是**词法真的把每一样都接住了**。
//!
//! ## 三处必须照源码而不是照文档
//!
//! 文档与源码在这个格式上分歧不小（`docs/research/metadata-formats.md` §B.1）：
//!
//! - **`players` 在源码里是字符串、默认 `"unknown"`**，文档写的 `integer` 是错的；
//!   官方示例自己给的就是 `<players>1-2</players>`。所以按自由文本读，区间接得住。
//! - **`rating` 载入时四舍五入到 0.1**（源码 `std::round(stof(v)/0.1f)/10.0f`），
//!   序列化用 `std::stringstream`，也就是最短往返形式（写 `0.7` 而不是 `0.700000`）。
//! - **`<path>` 必须带前导 `./`**（源码 `createRelativePath()` 返回 `"./" + rel`），
//!   USERGUIDE 明确警告从旧版 ES 搬来的文件常常没有这个前缀。
//!
//! ## `<developer>` 只装得下一条字符串
//!
//! 中立库里开发商、发行商与类型都是**集合**（数据源一个键写了几家就是几家），而这个
//! 格式一个 `<game>` 里这三个元素各只有一份。写的那一侧按 `, ` **合成一条**
//! （`joined`），读的那一侧**不拆**——真的 gamelist 里 `Sunsoft, Inc.` 这种带逗号的
//! 单值到处都是，拆开是**改内容**。
//!
//! 两侧因此不对称，代价说清楚：**「库 → 文件 → 库」这一趟两家会压成一条**；而能力档位
//! 说的是**「文件 → 库 → 文件」**（[`Capability::LosslessRoundTrip`] 的定义），那一条
//! 仍旧逐字节成立。留头一条把其余的丢掉才是真的坏——那是**静默换掉一家公司**。
//!
//! ## 布局：`gamelists/<平台目录>/` 与 `downloaded_media/<平台目录>/<类型>/`
//!
//! 与 Pegasus 的 `media/` 完全不同，也与 Batocera 的 `images|videos/` 加文件名后缀
//! 不同（别混）。ES-DE 的 gamelist 里**不再包含媒体路径**（官方原话），媒体靠
//! **文件名约定**找：路径精确镜像 ROM 相对平台目录的路径，文件名是去掉扩展名的 ROM
//! 文件名。于是 [`Adapter::media_placement`] 交出来的槽是 `None`——一个媒体路径都不写。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::path;
use crate::scrape::MediaKind;

use super::converge;
use super::{
    Adapter, AdapterError, Body, Capability, Document, Entry, Game, Lossy, LossyNote,
    MediaPlacement, Parsed, PlayerCount, Preserved, ReleaseDate, StructuralLoss,
};

/// ES gamelist 适配器。
#[derive(Debug, Clone, Copy, Default)]
pub struct Gamelist;

/// 元数据文件叫什么。**大小写敏感**，ES 家族四个变体一致。
pub const FILE_NAME: &str = "gamelist.xml";

/// gamelist 摆在哪个目录下（ES-DE：`<应用数据目录>/gamelists/<平台目录>/gamelist.xml`）。
pub const GAMELISTS_DIR: &str = "gamelists";

/// 媒体摆在哪个目录下（ES-DE：`<应用数据目录>/downloaded_media/<平台目录>/<类型>/`）。
pub const MEDIA_DIR: &str = "downloaded_media";

/// 根元素。**大写 L，大小写敏感**。
const ROOT: &str = "gameList";

/// 这个格式的**结构性损失**（[`Adapter::structural_losses`]）。
///
/// 八条，逐样往返实测出来的（`库 → 文件 → 库`，**不给底本**），每条配一条往返测试钉着。
///
/// ⚠️ **这不是这个格式的全部天花板**，是量过的那几条。眼下明确**不在**这份清单里的有两族：
///
/// - **`rating` 存进去四舍五入到 0.1，与只知道年份的发行日期补出来的那个 1 月 1 日。**
///   按**结构性损失**自己的定义这两样都算，但它们眼下走的是 [`LossyNote`] 那条按行数
///   得出来的路（`fold_game` 里 `rating` 与 `releasedate` 两个分支），与 Pegasus 那一侧
///   是同一族、同一条分界，**这张票不动那条分界**（挂单 `Q334`）。这里还比 Pegasus 多
///   一层：写的那一侧 [`format_rating`] 自己就先四舍五入了，`库 → 文件` 那一趟连有损点
///   都发不出来。
/// - **行尾。** XML 规范要解析器把 `\r\n` 与单个 `\r` 都规范成 `\n`，ES-DE 用的
///   pugixml 照做；而这里的词法**不规范化**（逐字节往返要它），往返回来 `\r` 原样还在。
///   这一条我们自己量不到、也钉不住——照直觉写进清单就是一句没人核过的话（挂单 `Q335`）。
/// - **真的 1970 年 1 月 1 日。** `19700101T000000` 是 ES-DE 的默认值（意思是「没有发行
///   日期」），于是那一天表达不出来——与清单第 3 条（字面上就叫 `unknown` 的开发商）
///   是同一个形状。它也已经走 [`LossyNote`]（读回来推一条 `Dropped`），同上不动分界
///   （挂单 `Q339`）。
///
/// ## 与 Pegasus 那份清单差在哪
///
/// Pegasus 丢的多半是**值的文本形状**（换行、每一行两头的空白、那一行 `.`）；这个格式
/// 丢的多半是**一条装不装得下**：`<developer>` 只有一份、`<game>` 只有一个 `<path>`、
/// 字段表上没有的元素根本写不出去。两家都有的只有「值两端的空白」那一条，而掐的位置也
/// 不一样——Pegasus 掐的是**每一行**的两头、一个值都不放过；这里掐的是整个值的两端，
/// 而且 `desc`、`x-` 扩展键与未知元素这三样根本不掐。
///
/// ## 这几条与「往返一个字节都不差」并不打架
///
/// 带**底本**的那一趟里，原文那几行原样躺在快照里、原样写回去，逐字节相同照旧成立
/// ——[`Capability::LosslessRoundTrip`] 说的正是 `文件 → 库 → 文件`。变形只发生在
/// **新生成**的内容上：库里的值写出去、再读回来，拿到的就不是原来那个值了。
///
/// ## 为什么它们避不开
///
/// **一、值两端的空白被掐掉，三样例外。** 原版 ES 的字段声明里 `desc` 是
/// `MD_MULTILINE_STRING`、别的元素是 `MD_STRING`（调研 §B.1 那张 18 个字段的表）。
/// 而一份手改过的 gamelist 里，元素的文本两端带的常常是**文件自己的缩进**
/// （`<name>\n    甲\n  </name>`），折进中立模型的那一族于是一律 `trim` 掉两头。
/// **不掐的有三样**：`desc`（得装得下带排版的长文），以及 `x-` 扩展键与未知元素
/// ——那两样是**原样留着**的，折那一步根本没经手它们。掐完不剩东西的
/// （`"   "`，或本来就是空串的），那个元素**整条不写**：这个格式的规矩是
/// 「值等于默认值时不写出」，而单行那一族的默认值就是空串。`desc` 在这一格上不例外。
///
/// **二、开发商 / 发行商 / 类型写了几条这件事存不下。** 这三个元素一个 `<game>` 里
/// 各只有一份，而中立库里它们是**集合**。写的那一侧按 `, ` 合成一条（`joined`），
/// 读的那一侧**不拆**——`Sunsoft, Inc.` 这种带逗号的单值在真的 gamelist 里到处都是，
/// 按 `, ` 拆等于**改内容**。于是「两家」与「一家名字里有逗号」写出去长得一模一样，
/// 读回来再也分不开。留头一条把其余的丢掉才是真的坏——那是**静默换掉一家公司**。
///
/// **三、字面上就叫 `unknown` 的那一条。** 原版 ES 给 `developer` / `publisher` /
/// `genre` 的默认值就是 `unknown`（调研 §B.1 那张 18 个字段的表），而这个格式的规矩是
/// 「值等于默认值时不写出」——一家真的叫 `unknown` 的公司写进去，与「这一栏没填」在
/// 文件里是同一件事。**只在它单独一条的时候**：与别的值合成一条之后整条就不等于默认值了。
///
/// **四、「这几个文件是同一部作品」这件事。** 一个 `<game>` 只装得下一个 `<path>`
/// （见 `paths_of`），而中立库里一个条目可以挂着好几个变体。摊成几段、每段带同一份
/// 元数据，是这个格式里说得出口的唯一说法——代价是读回来它们成了几个各自独立的条目。
///
/// **五、中立库里这个格式没有元素的那几样。** `ORDER` 就是它装得下的全部字段。
/// 一段式简介、标签、启动命令、工作目录落在中立文档的**并集**里（ADR-0003），在这张表上
/// 却没有对应的元素——写出去一个字都没有。`desc` 装的是长描述，不拿它顶简介：
/// 那是替用户把两个字段合成一个。
///
/// **六、合集段。** 这个格式里没有合集这个概念（见 `prefix_of`），一份 gamelist 就是
/// 一个平台目录的事。合集段于是整段写不出去。**只有它的平台目录是用掉了的**：
/// `<path>` 相对那个目录解析，写的时候把它剥掉了——那一段不是丢了，是起了作用。
///
/// **七、资源槽。** `MEDIA_ELEMENTS` 那张表就是这个格式认得的全部槽，而规范资源槽
/// 一共二十个（Pegasus 那一侧的 `ASSET_TYPES`）。落在表外的十一个写出去一个元素都没有；
/// 认得的那九个也各只有一份，一个槽里的第二条路径写不出去。这不是疏漏：ES-DE 自己
/// **靠文件名找媒体**，gamelist 里一个媒体路径都不写，这几个元素是原版 ES / Batocera /
/// Recalbox 留下来的。
///
/// **八、`x-` 扩展键与未知元素。** 一个元素就是一份值，没有续行一说，所以一个键只
/// 写得下第一条。更要命的是**键名**：写标签名那一步不转义，而词法扫名字时撞上
/// **空白、`/` 或 `>`** 就收尾（`lex_start`），于是键名里有这三样的写不出一个认得回来
/// 的标签。两种坏法还不一样——空白与 `/` 是**键与值一起不见**，`>` 是**键被截断、值被
/// 前一段键名污染**（`x-我的>备注` 读回来是键 `我的`、值 `备注>值`），后者更坏：用户
/// 拿回来的是一个看着正常的错值。别的字符（`&`、`<`、引号、数字开头）反而都回得来
/// ——判据是「是不是词法的分隔符」，不是「合不合 XML 规范」。Pegasus 的 `x-` 键是自由
/// 文本（`x-我的 备注` 完全合法），跨格式搬过来正撞上这一条。
pub const STRUCTURAL_LOSSES: &[StructuralLoss] = &[
    StructuralLoss {
        what: "值两端的空白，含全角空格 U+3000（`desc`、`x-` 扩展键与未知元素**之外**\
               的每一个元素：标题、排序名、路径、开发商 / 发行商 / 类型、人数、评分、\
               日期与媒体路径）",
        becomes: "被掐掉；掐完不剩东西的，那个元素整条不写、读回来是空的。\
                  `desc`、`x-` 扩展键与未知元素不掐——两端的空白与值里的换行原样回得来，\
                  只有 `desc` 在「整条只剩空白就消失」这一格上仍旧跟着走",
        why: "手改过的 gamelist 里元素文本两端带的常常是文件自己的缩进，折进中立模型的\
              那一族（源码里的 `MD_STRING`）分不出它与值本身的空白，只好一律掐掉；\
              `desc` 是 `MD_MULTILINE_STRING`、得装得下带排版的长文，而 `x-` 扩展键与\
              未知元素是原样留着的，那一步根本没经手它们",
    },
    StructuralLoss {
        what: "开发商 / 发行商 / 类型写了几条这件事",
        becomes: "几条按 `, ` 合成一条，读回来是一条；而本来就带 `, ` 的单值\
                  （`Sunsoft, Inc.`）原样回来——两个输入落到同一个结果上，再也分不开",
        why: "这三个元素一个 `<game>` 里各只有一份，装不下几条；而读的那一侧不敢按 `, `\
              拆——真的 gamelist 里名字带逗号的公司到处都是，拆开是改内容",
    },
    StructuralLoss {
        what: "字面上就叫 `unknown` 的开发商 / 发行商 / 类型（**单独一条**的时候）",
        becomes: "整条消失，读回来这一栏是空的；与别的值合成一条之后反倒留得住",
        why: "`unknown` 是原版 ES 给这三个元素的**默认值**，而这个格式的规矩是\
              「值等于默认值时不写出」——写出来与不写出来是同一个效果",
    },
    StructuralLoss {
        what: "「这几个文件是同一部作品」这件事（**收敛**过的多文件条目）",
        becomes: "摊成几个条目，一个文件一个；读回来是几个条目，不是一个挂着几个文件的。\
                  元数据每一份都在，丢的是「它们是同一部作品」这一层",
        why: "这个格式一个 `<game>` 只装得下一个 `<path>`，而 ES 那边磁盘上每个 ROM \
              文件本来就各自是一个条目——少写的那几条不会消失，只会变成一个光秃秃的文件名",
    },
    StructuralLoss {
        what: "中立库里的一段式简介、标签、启动命令、工作目录",
        becomes: "一个字都不写，读回来这四样是空的",
        why: "这个格式的字段表（原版 ES 的 18 个）里没有对应的元素；\
              `desc` 装的是长描述，把简介挪进去等于替用户把两个字段合成一个",
    },
    StructuralLoss {
        what: "**合集段**（它的简介、短名与 `x-` 扩展键）",
        becomes: "整段不写，读回来一个合集段都没有。它的**平台目录**不是丢了、是用掉了\
                  ——`<path>` 相对它解析，写的时候剥掉了那一段；而剥掉的那一段要\
                  **再导入一趟**、靠盘上的目录重新锚定才回得来，光读文件回不来",
        why: "ES gamelist 里没有合集这个概念：平台是由文件摆在哪个目录下说的，\
              一份 gamelist 就是一个平台目录的事",
    },
    StructuralLoss {
        what: "二十个规范**资源槽**里这个格式没有元素的那十一个，以及一个槽里的第二条路径",
        becomes: "十一个槽（`boxSpine` `poster` `bezel` `panel` `cabinetLeft` \
                  `cabinetRight` `tile` `banner` `steam` `music` `screenshot`）写出去\
                  一个元素都没有；认得的那九个也只写得下**第一条**路径",
        why: "ES-DE 自己靠文件名找媒体、gamelist 里一个媒体路径都不写，这几个元素是\
              原版 ES / Batocera / Recalbox 留下的，各只有一份、也只有这几种",
    },
    StructuralLoss {
        what: "`x-` 扩展键与未知元素：一个键的第二个值，以及**键名里的空白、`/` 或 `>`**",
        becomes: "一个键只写得下第一条值；键名里带空白或 `/` 的，**键与值一起不见**；\
                  带 `>` 的更坏——键被截断、值被前一段键名污染（`x-我的>备注` 读回来是\
                  键 `我的`、值 `备注>值`），拿回来的是个看着正常的错值",
        why: "这个格式里一个元素就是一份值，没有续行一说；而写标签名那一步不转义，\
              词法扫名字时撞上空白、`/` 或 `>` 就收尾——Pegasus 的 `x-` 键是自由文本，\
              跨格式搬过来正撞上这一条",
    },
];

// ════════════════════════════════════════════════════════════════════════
// 快照：拆到零件的节点
// ════════════════════════════════════════════════════════════════════════

/// 一个标签里的属性，拆开来的零件。
///
/// 拆到这个粒度不是洁癖：Batocera 写 `<game id="12345">`、Recalbox 写
/// `<game source="Recalbox" timestamp="…">`，而单引号、等号两侧的空白、属性之间几个
/// 空格全是手改过的文件里真实存在的写法。不逐样接住，写回去就少几个字符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// 属性名之前的空白（至少一个字符）。
    pub lead: String,
    /// 属性名。
    pub name: String,
    /// 名字与 `=` 之间的空白。
    pub pad_name: String,
    /// `=` 与引号之间的空白。
    pub pad_value: String,
    /// 引号是 `"` 还是 `'`。
    pub quote: char,
    /// 引号之间的**原文**，未反转义。
    pub value: String,
}

impl Attribute {
    fn render(&self, out: &mut String) {
        out.push_str(&self.lead);
        out.push_str(&self.name);
        out.push_str(&self.pad_name);
        out.push('=');
        out.push_str(&self.pad_value);
        out.push(self.quote);
        out.push_str(&self.value);
        out.push(self.quote);
    }
}

/// 一个起始标签拆开来的零件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    /// 标签名，**原始大小写**。
    pub name: String,
    /// 属性。
    pub attributes: Vec<Attribute>,
    /// `>` 或 `/>` 之前的空白。
    pub pad: String,
    /// 是不是 `<x/>` 这种自闭合。
    pub self_closing: bool,
}

impl Tag {
    fn render(&self, out: &mut String) {
        out.push('<');
        out.push_str(&self.name);
        for attribute in &self.attributes {
            attribute.render(out);
        }
        out.push_str(&self.pad);
        if self.self_closing {
            out.push_str("/>");
        } else {
            out.push('>');
        }
    }
}

/// 文件里的一个节点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// 标签之外的原文：空白、字符数据、实体引用，**原样**。
    ///
    /// 存的是转义前的原文而不是反转义后的值：`&amp;` 与 `&#38;` 反转义之后是同一个
    /// 字符，再转义回去只剩一种写法——那一趟往返就丢了维护者写的那一种。
    Text(String),
    /// `<?xml … ?>` 这类处理指令。
    Declaration(String),
    /// `<!-- … -->`。
    Comment(String),
    /// `<!DOCTYPE …>` 之类。
    Doctype(String),
    /// `<![CDATA[ … ]]>`。
    Cdata(String),
    /// 起始标签（含自闭合）。
    Start(Tag),
    /// 结束标签。
    End {
        /// 标签名。
        name: String,
        /// `>` 之前的空白。
        pad: String,
    },
    /// 一个 `<` 后面接不上任何认得的东西。
    ///
    /// **原样留着**。它多半是维护者写岔了的一段，替他删掉不是无损。
    Stray(String),
}

impl Node {
    fn render(&self, out: &mut String) {
        match self {
            Self::Text(raw)
            | Self::Declaration(raw)
            | Self::Comment(raw)
            | Self::Doctype(raw)
            | Self::Cdata(raw)
            | Self::Stray(raw) => out.push_str(raw),
            Self::Start(tag) => tag.render(out),
            Self::End { name, pad } => {
                out.push_str("</");
                out.push_str(name);
                out.push_str(pad);
                out.push('>');
            }
        }
    }
}

/// 一段占哪几个节点（左闭右开）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 从第几个节点起（`<game>` 那一个）。
    pub start: usize,
    /// 到第几个为止（不含，也就是 `</game>` 的下一个）。
    pub end: usize,
}

/// 快照里的**一段**：那几个节点，连同它们躺在哪儿。
///
/// 单独立一个类型而不是到处传 `(&[Node], Span)`：这一对在拆子元素、找 `<path>`、
/// 量缩进、找段尾这四件事里成对出现，分开传就总有一天传成两段的组合。
#[derive(Debug, Clone, Copy)]
struct Block<'a> {
    nodes: &'a [Node],
    span: Span,
}

/// **旁路快照**：一份 gamelist 拆开的样子。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// 文件开头有没有 UTF-8 BOM。
    pub bom: bool,
    /// 全部节点。
    pub nodes: Vec<Node>,
    /// 每个 `<game>` 元素占哪几个节点，按文件顺序。
    ///
    /// **只有 `<game>`**。`<folder>` 不是前端条目（它是磁盘上的一个目录），
    /// 也不折进中立文档；它连同别的一切段外内容一起原样搬运。
    pub blocks: Vec<Span>,
    /// `</gameList>` 是第几个节点。新段插在它**前面**；没有根元素时是 `None`。
    pub close_at: Option<usize>,
}

impl Snapshot {
    /// 第 `nth` 段。
    fn block(&self, span: Span) -> Block<'_> {
        Block {
            nodes: &self.nodes,
            span,
        }
    }

    /// 把整份快照原样拼回去。
    #[must_use]
    pub fn render(&self) -> Vec<u8> {
        let mut out = String::new();
        if self.bom {
            out.push('\u{feff}');
        }
        for node in &self.nodes {
            node.render(&mut out);
        }
        out.into_bytes()
    }
}

// ════════════════════════════════════════════════════════════════════════
// 适配器
// ════════════════════════════════════════════════════════════════════════

impl Adapter for Gamelist {
    fn name(&self) -> &'static str {
        "ES-Gamelist"
    }

    fn ceiling(&self) -> Capability {
        Capability::LosslessRoundTrip
    }

    fn file_name(&self) -> &'static str {
        FILE_NAME
    }

    fn structural_losses(&self) -> &'static [StructuralLoss] {
        STRUCTURAL_LOSSES
    }

    fn read(&self, bytes: &[u8]) -> Result<Parsed, AdapterError> {
        let snapshot = snapshot_of(bytes)?;
        let (doc, lossy, counts) = fold(&snapshot);
        let text = std::str::from_utf8(bytes).unwrap_or_default();
        let preserved = Preserved {
            // 行数按真正的换行数来——报告里那一列印给人看，人数的是行。
            lines: text.lines().count() as u64,
            comments: snapshot
                .nodes
                .iter()
                .filter(|node| matches!(node, Node::Comment(_)))
                .count() as u64,
            unknown_keys: doc
                .entries
                .iter()
                .filter_map(|entry| entry.game())
                .map(|game| game.unknown.len() as u64)
                .sum(),
            extension_keys: doc
                .entries
                .iter()
                .filter_map(|entry| entry.game())
                .map(|game| game.extra.len() as u64)
                .sum(),
            user_state: counts.user_state,
        };
        Ok(Parsed {
            doc,
            source: bytes.to_vec(),
            preserved,
            lossy,
        })
    }

    fn write(&self, doc: &Document, baseline: Option<&Parsed>) -> Result<Vec<u8>, AdapterError> {
        // 基线的原文在这里**重新拆一遍**（同 Pegasus）：同一串字节拆两次是同一个结果，
        // 换来的是跨格式的 [`Parsed`] 不必装着某一个适配器的私有形状。
        let baseline = baseline
            .map(|parsed| snapshot_of(&parsed.source).map(|snapshot| (&parsed.doc, snapshot)))
            .transpose()?;
        Ok(render(
            doc,
            baseline.as_ref().map(|(was, snapshot)| (*was, snapshot)),
        ))
    }

    fn kept_verbatim(&self, doc: &Document, baseline: &Parsed) -> u64 {
        // **按段数，不按条目数。** 一个 `<game>` 只装得下一个文件，多文件条目摊成
        // 好几段——照 [`Entry::origin`] 数的话，摊开的那几段会被全算成「没认领」。
        // 真库上一趟导出就是 19,442 段的谎。
        let Ok(snapshot) = snapshot_of(&baseline.source) else {
            return 0;
        };
        let claimed = claims(doc, &snapshot, prefix_of(doc));
        (snapshot.blocks.len() - claimed.len()) as u64
    }

    fn metadata_path(&self, collection: &str) -> String {
        format!(
            "{GAMELISTS_DIR}/{}/{FILE_NAME}",
            converge::safe_segment(collection)
        )
    }

    fn rom_bases(&self, file: &Path, root: Option<&Path>) -> Vec<PathBuf> {
        let dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        let Some(root) = root else {
            // 没给主库根：只剩「gamelist 与 ROM 同处一个目录」这条路——原版 ES、
            // Batocera 与 Recalbox 都把它写在那个平台目录里，与 ROM 并排。
            return vec![dir];
        };
        let mut out = Vec::new();
        // `gamelists/<平台目录>/gamelist.xml` → ROM 在 `<主库根>/<平台目录>/` 下。
        if let Some(directory) = directory_of(file) {
            out.push(root.join(directory));
        }
        // 主库根本身。两条理由：`<path>` 写的是完整的键（数不出唯一平台目录时导出
        // 就是这样写的），以及别人分享的包里路径写法五花八门。
        out.push(root.to_path_buf());
        // 最后才是文件自己所在的目录。
        if !out.contains(&dir) {
            out.push(dir);
        }
        out
    }

    fn media_placement(
        &self,
        rom_key: &str,
        kind: MediaKind,
        _hash: &str,
        ext: &str,
    ) -> Option<MediaPlacement> {
        // **路径精确镜像 ROM 相对平台目录的路径，文件名是去掉扩展名的 ROM 文件名。**
        // 官方示例：ROM `~/ROMs/c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u`
        // → 媒体 `downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg`。
        // 键的第一段是**根名**，平台目录在它后面（`path::library_key`）：
        // 剥的是「根名 + 平台目录」那一整截，留下来的才是相对平台目录的路径。
        let prefix = path::platform_dir_of_key(rom_key)?;
        let directory = path::platform_of_key(rom_key)?;
        let rest = rom_key.strip_prefix(prefix)?.trim_start_matches('/');
        let stem = rest.rsplit_once('.').map_or(rest, |(stem, _)| stem);
        Some(MediaPlacement {
            path: format!(
                "{MEDIA_DIR}/{directory}/{}/{stem}.{ext}",
                media_dir_of(kind)?
            ),
            // **条目里一个媒体路径都不写。** ES-DE 官方原话：gamelist.xml 里不再包含
            // 媒体信息，应用会去找与 ROM 文件名匹配的任何媒体。写进去既没用，
            // 又会在 ES-DE 重写这份文件时被清掉，凭空造出一次「外部改动」。
            slot: None,
        })
    }
}

/// 一份媒体落在 `downloaded_media` 的哪个类型目录下。
///
/// 取自官方那 12 个目录名的完整清单。**认不出是什么的图没有目录**——不猜。
#[must_use]
pub fn media_dir_of(kind: MediaKind) -> Option<&'static str> {
    Some(match kind {
        MediaKind::Cover => "covers",
        MediaKind::Screenshot => "screenshots",
        MediaKind::Video => "videos",
        MediaKind::Other => return None,
    })
}

/// 一份 gamelist 说的是哪个**平台目录**：`…/gamelists/<平台目录>/gamelist.xml` 里那一段。
fn directory_of(file: &Path) -> Option<String> {
    let dir = file.parent()?;
    // 上一级得真叫 `gamelists`，否则这就是「与 ROM 同处一目录」的那一种摆法，
    // 目录名是平台目录名这件事无从谈起。
    if dir.parent()?.file_name()?.to_str()? != GAMELISTS_DIR {
        return None;
    }
    Some(dir.file_name()?.to_str()?.to_string())
}

/// 把一份原文拆成快照。**读与写共用它**，各拆一遍必然有一天拆得不一样。
fn snapshot_of(bytes: &[u8]) -> Result<Snapshot, AdapterError> {
    // **不猜编码。** ES 家族四个变体都用 pugixml，默认按 UTF-8 读；猜出来的 GBK
    // 就算蒙对了，那份文件在前端里本来也是乱码——替用户「修好」它，等于悄悄改写了
    // 他的原文，而这张票的全部意义是一个字节都不改。
    let text = std::str::from_utf8(bytes).map_err(|error| AdapterError::NotUtf8 {
        path: FILE_NAME.to_string(),
        offset: error.valid_up_to(),
    })?;
    Ok(lex(text))
}

// ════════════════════════════════════════════════════════════════════════
// 词法
// ════════════════════════════════════════════════════════════════════════

/// 把原文拆成节点。**只做词法，不认字段，也不管有几个根元素**。
fn lex(text: &str) -> Snapshot {
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut nodes = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        let Some(at) = rest.find('<') else {
            nodes.push(Node::Text(rest.to_string()));
            break;
        };
        if at > 0 {
            nodes.push(Node::Text(rest[..at].to_string()));
        }
        let (node, taken) = lex_tag(&rest[at..]);
        nodes.push(node);
        rest = &rest[at + taken..];
    }
    let (blocks, close_at) = structure(&nodes);
    Snapshot {
        bom,
        nodes,
        blocks,
        close_at,
    }
}

/// 从 `<` 起拆一个标签，返回节点与吃掉了几个字节。
fn lex_tag(rest: &str) -> (Node, usize) {
    // 顺序照 XML 的词法：先认三种 `<!` 开头的，再认 `<?`，再认 `</`，最后才是起始标签。
    for (open, close, make) in [
        ("<!--", "-->", Node::Comment as fn(String) -> Node),
        ("<![CDATA[", "]]>", Node::Cdata as fn(String) -> Node),
    ] {
        if let Some(after) = rest.strip_prefix(open) {
            let end = after
                .find(close)
                .map_or(rest.len(), |at| at + open.len() + close.len());
            return (make(rest[..end].to_string()), end);
        }
    }
    for (open, close, make) in [
        ("<?", "?>", Node::Declaration as fn(String) -> Node),
        ("<!", ">", Node::Doctype as fn(String) -> Node),
    ] {
        if let Some(after) = rest.strip_prefix(open) {
            let end = after
                .find(close)
                .map_or(rest.len(), |at| at + open.len() + close.len());
            return (make(rest[..end].to_string()), end);
        }
    }
    if let Some(after) = rest.strip_prefix("</") {
        let Some(at) = after.find('>') else {
            return (Node::Stray(rest.to_string()), rest.len());
        };
        let inner = &after[..at];
        let name = inner.trim_end();
        return (
            Node::End {
                name: name.to_string(),
                pad: inner[name.len()..].to_string(),
            },
            at + 3,
        );
    }
    match lex_start(rest) {
        Some((tag, taken)) => (Node::Start(tag), taken),
        // 认不出：吃到下一个 `<` 之前为止，原样留着。**不吃掉那个 `<`**，
        // 否则一段写岔了的文字会把它后面一个好端端的标签也一起吞掉。
        None => {
            let end = rest[1..].find('<').map_or(rest.len(), |at| at + 1);
            (Node::Stray(rest[..end].to_string()), end)
        }
    }
}

/// 拆一个起始标签。拆不出来（没有 `>`、名字为空、属性写法不认）时是 `None`。
fn lex_start(rest: &str) -> Option<(Tag, usize)> {
    let after = rest.strip_prefix('<')?;
    let name_len = after
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(after.len());
    let name = &after[..name_len];
    if name.is_empty() {
        return None;
    }
    let mut cursor = &after[name_len..];
    let mut taken = 1 + name_len;
    let mut attributes = Vec::new();
    loop {
        let lead_len = cursor.len() - cursor.trim_start().len();
        let (lead, body) = cursor.split_at(lead_len);
        if body.starts_with("/>") {
            return Some((
                Tag {
                    name: name.to_string(),
                    attributes,
                    pad: lead.to_string(),
                    self_closing: true,
                },
                taken + lead_len + 2,
            ));
        }
        if body.starts_with('>') {
            return Some((
                Tag {
                    name: name.to_string(),
                    attributes,
                    pad: lead.to_string(),
                    self_closing: false,
                },
                taken + lead_len + 1,
            ));
        }
        // 一个属性：`名 = "值"`。等号两侧的空白与引号字符逐样接住。
        if lead.is_empty() || body.is_empty() {
            return None;
        }
        let attr_name_len = body
            .find(|c: char| c.is_whitespace() || c == '=' || c == '>' || c == '/')
            .unwrap_or(body.len());
        let attr_name = &body[..attr_name_len];
        if attr_name.is_empty() {
            return None;
        }
        let after_name = &body[attr_name_len..];
        let pad_name_len = after_name.len() - after_name.trim_start().len();
        let after_pad = &after_name[pad_name_len..];
        let after_equals = after_pad.strip_prefix('=')?;
        let pad_value_len = after_equals.len() - after_equals.trim_start().len();
        let after_pad_value = &after_equals[pad_value_len..];
        let quote = after_pad_value.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let value_end = after_pad_value[1..].find(quote)?;
        let value = &after_pad_value[1..1 + value_end];
        attributes.push(Attribute {
            lead: lead.to_string(),
            name: attr_name.to_string(),
            pad_name: after_name[..pad_name_len].to_string(),
            pad_value: after_equals[..pad_value_len].to_string(),
            quote,
            value: value.to_string(),
        });
        let consumed = lead_len + attr_name_len + pad_name_len + 1 + pad_value_len + value_end + 2;
        cursor = &cursor[consumed..];
        taken += consumed;
    }
}

/// 走一遍节点，找出 `<game>` 各占哪几个，以及 `</gameList>` 在第几个。
fn structure(nodes: &[Node]) -> (Vec<Span>, Option<usize>) {
    let mut blocks = Vec::new();
    let mut close_at = None;
    let mut in_root = false;
    let mut open: Option<(usize, String)> = None;
    let mut depth = 0usize;
    for (index, node) in nodes.iter().enumerate() {
        match node {
            Node::Start(tag) if tag.self_closing => {
                // `<game/>` 这种空段也算一段。
                if open.is_none() && in_root && tag.name == "game" {
                    blocks.push(Span {
                        start: index,
                        end: index + 1,
                    });
                }
            }
            Node::Start(tag) => {
                if let Some((_, name)) = &open {
                    if &tag.name == name {
                        depth += 1;
                    }
                } else if in_root && tag.name == "game" {
                    open = Some((index, tag.name.clone()));
                    depth = 1;
                } else if !in_root && tag.name == ROOT {
                    in_root = true;
                }
            }
            Node::End { name, .. } => {
                if let Some((start, open_name)) = &open {
                    if name == open_name {
                        depth -= 1;
                        if depth == 0 {
                            blocks.push(Span {
                                start: *start,
                                end: index + 1,
                            });
                            open = None;
                        }
                    }
                } else if in_root && name == ROOT {
                    in_root = false;
                    close_at = Some(index);
                }
            }
            _ => {}
        }
    }
    (blocks, close_at)
}

// ════════════════════════════════════════════════════════════════════════
// 折成中立文档
// ════════════════════════════════════════════════════════════════════════

/// **用户状态**：ADR-0006 说的那四样，加上上次游玩，以及 ES 家族各变体的拼法。
///
/// 它们**一个都不折进中立模型**——工具连「生成一个」的路径都没有。在快照里躺着，
/// 导出时随没变的那些一起原样搬回去。
///
/// `hidden` / `kidgame` / `broken` 这些没列进来：它们是用户设的**标记**不是「玩出来的
/// 历史」（`CONTEXT.md` 的**用户状态**词条），照未知键的路走，一样原样留着。
const USER_STATE: &[&str] = &[
    // 收藏
    "favorite",
    // 游玩次数
    "playcount",
    // 上次游玩
    "lastplayed",
    // 游玩时长：ES-DE 是 `playtime`，Batocera 是 `gametime`，Recalbox 是 `timeplayed`
    "playtime",
    "gametime",
    "timeplayed",
    // 通关状态
    "completed",
];

/// 折一遍数出来的东西。
#[derive(Debug, Clone, Copy, Default)]
struct Counts {
    user_state: u64,
}

/// 一个子元素：名字加它的文本内容（已反转义），以及它在第几个节点上。
struct Child {
    name: String,
    text: String,
    at: usize,
}

/// 把一段里的子元素收出来。**只收直接子元素**，孙子（Recalbox 的 `<maps><map/></maps>`）
/// 连同它的父元素一起当成一个整体留在快照里。
impl Block<'_> {
    /// 把这一段里的子元素收出来。**只收直接子元素**，孙子（Recalbox 的
    /// `<maps><map/></maps>`）连同它的父元素一起当成一个整体留在快照里。
    fn children(self) -> Vec<Child> {
        children(self.nodes, self.span)
    }

    /// 这一段的 `<path>` 写的是什么（原样，含前导 `./`）。
    fn path(self) -> Option<String> {
        self.children()
            .into_iter()
            .find(|kid| kid.name == "path")
            .map(|kid| kid.text.trim().to_string())
    }

    /// 一个子元素从 `at` 起到哪个节点为止（不含）。
    fn element_end(self, at: usize) -> usize {
        element_end(self.nodes, self.span, at)
    }

    /// 这一段里子元素的缩进（含前面那个换行）。
    fn child_indent(self) -> String {
        for index in self.span.start + 1..self.span.end {
            if let Node::Text(raw) = &self.nodes[index]
                && let Some((_, tail)) = raw.rsplit_once('\n')
                && tail.chars().all(char::is_whitespace)
            {
                return format!("\n{tail}");
            }
        }
        format!("\n{INDENT}{INDENT}")
    }

    /// 这一段结尾那几个字：`</game>` 与它前面那一截空白。
    fn close_suffix(self) -> String {
        let mut out = String::new();
        if self.span.end >= 2
            && let Node::Text(raw) = &self.nodes[self.span.end - 2]
        {
            out.push_str(raw);
        }
        if self.span.end >= 1 {
            self.nodes[self.span.end - 1].render(&mut out);
        }
        out
    }

    /// 把这一段原样搬出去。
    fn render(self, out: &mut String) {
        for index in self.span.start..self.span.end {
            self.nodes[index].render(out);
        }
    }
}

fn children(nodes: &[Node], span: Span) -> Vec<Child> {
    let mut out = Vec::new();
    let mut index = span.start + 1;
    while index + 1 < span.end {
        let Node::Start(tag) = &nodes[index] else {
            index += 1;
            continue;
        };
        if tag.self_closing {
            out.push(Child {
                name: tag.name.clone(),
                text: String::new(),
                at: index,
            });
            index += 1;
            continue;
        }
        // 找它自己的结束标签，同名嵌套照数。
        let mut depth = 1usize;
        let mut cursor = index + 1;
        let mut text = String::new();
        while cursor < span.end && depth > 0 {
            match &nodes[cursor] {
                Node::Start(inner) if inner.name == tag.name && !inner.self_closing => depth += 1,
                Node::End { name, .. } if name == &tag.name => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Node::Text(raw) if depth == 1 => text.push_str(&unescape(raw)),
                Node::Cdata(raw) if depth == 1 => {
                    let inner = raw
                        .strip_prefix("<![CDATA[")
                        .and_then(|rest| rest.strip_suffix("]]>"))
                        .unwrap_or(raw);
                    text.push_str(inner);
                }
                _ => {}
            }
            cursor += 1;
        }
        out.push(Child {
            name: tag.name.clone(),
            text,
            at: index,
        });
        index = cursor + 1;
    }
    out
}

/// 把快照折成中立文档，顺带点名格式自己会吞掉的写法、数一遍用户状态。
fn fold(snapshot: &Snapshot) -> (Document, Vec<LossyNote>, Counts) {
    let mut entries = Vec::new();
    let mut lossy = Vec::new();
    let mut counts = Counts::default();
    let line_of = line_index(&snapshot.nodes);
    for (nth, span) in snapshot.blocks.iter().enumerate() {
        let kids = snapshot.block(*span).children();
        let game = fold_game(&kids, &line_of, &mut lossy, &mut counts);
        entries.push(Entry {
            origin: Some(nth),
            body: Body::Game(game),
        });
    }
    (Document { entries }, lossy, counts)
}

/// 每个节点从第几行开始（从 1 数）。有损点要报行号，而节点不是行。
fn line_index(nodes: &[Node]) -> Vec<usize> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut line = 1usize;
    let mut buffer = String::new();
    for node in nodes {
        out.push(line);
        buffer.clear();
        node.render(&mut buffer);
        line += buffer.matches('\n').count();
    }
    out
}

fn fold_game(
    kids: &[Child],
    line_of: &[usize],
    lossy: &mut Vec<LossyNote>,
    counts: &mut Counts,
) -> Game {
    let mut out = Game::default();
    for kid in kids {
        let line = line_of.get(kid.at).copied().unwrap_or(0);
        let value = kid.text.trim().to_string();
        match kid.name.as_str() {
            // `<path>` 是条目的身份。前导 `./` 是 ES-DE 要求的写法（源码
            // `createRelativePath()`），中立模型里不留它——留着的话，同一个文件在
            // 两个格式里就成了两条不同的路径，收敛时对不上。
            "path" => out.files.push(strip_dot_slash(&value).to_string()),
            "name" => out.title = value,
            "sortname" => out.sort_title = non_empty(&value),
            "desc" => out.description = Some(kid.text.clone()).filter(|t| !t.trim().is_empty()),
            // **一整条，不按逗号拆。** 写的那一侧把多家拼成一条（[`joined`]），
            // 但读回来不拆——`Sunsoft, Inc.` 这种带逗号的单值在真的 gamelist 里
            // 到处都是，拆开是改内容。两侧的不对称与它的代价见 [`joined`]。
            "developer" => push_non_empty(&mut out.developers, &value),
            "publisher" => push_non_empty(&mut out.publishers, &value),
            "genre" => push_non_empty(&mut out.genres, &value),
            "players" => {
                // 源码里是 `MD_STRING`、默认 `"unknown"`，文档写的 `integer` 是错的。
                // 认不出区间的写法不是有损点——那一栏本来就是自由文本，ES 原样显示。
                out.players = parse_players(&value);
            }
            "rating" => {
                out.rating = parse_rating(&value);
                if out.rating.is_none() && !value.is_empty() {
                    lossy.push(LossyNote {
                        line,
                        key: kid.name.clone(),
                        value: value.clone(),
                        kind: Lossy::Dropped,
                        detail: "`rating` 只认 0–1 的浮点，别的写法 `stof` 解不出来，\
                                 这一行在前端里没生效"
                            .to_string(),
                    });
                } else if let Some(rating) = out.rating
                    && (rating - round_to_tenth(rating)).abs() > f64::EPSILON
                {
                    lossy.push(LossyNote {
                        line,
                        key: kid.name.clone(),
                        value: value.clone(),
                        kind: Lossy::Flattened,
                        detail: format!(
                            "ES-DE 载入时四舍五入到 0.1 的整数倍（源码 \
                             `std::round(stof(v)/0.1f)/10.0f`）——存进去是 {}",
                            round_to_tenth(rating)
                        ),
                    });
                }
            }
            "releasedate" => {
                out.release = parse_datetime(&value);
                match out.release {
                    None if !value.is_empty() => lossy.push(LossyNote {
                        line,
                        key: kid.name.clone(),
                        value: value.clone(),
                        kind: Lossy::Dropped,
                        detail: "`releasedate` 只认 `%Y%m%dT%H%M%S`（如 `19950311T000000`），\
                                 别的写法解不出来"
                            .to_string(),
                    }),
                    Some(_) => lossy.push(LossyNote {
                        line,
                        key: kid.name.clone(),
                        value: value.clone(),
                        kind: Lossy::Padded,
                        detail: "这个格式的日期**没有精度**这一说：只知道年份的发行日期\
                                 写进去也带着一个月一个日，而 ES-DE 只显示日期部分、\
                                 时间被忽略"
                            .to_string(),
                    }),
                    None => {}
                }
            }
            // **用户状态：一个都不折进中立模型**（ADR-0006）。只数一笔，留在快照里。
            name if USER_STATE.contains(&name) => counts.user_state += 1,
            // 别的 ES 变体写在这里的媒体路径（原版 ES / Batocera / Recalbox 的
            // `image` / `video` / `marquee` / `thumbnail`）。ES-DE 自己不写也不读，
            // 但**别人分享的元数据包里到处都是**，收进资源槽，往返照旧。
            //
            // 槽在这里**一次查出来绑住**：查两遍再 `unwrap_or_default` 的话，
            // 兜到空串时写回去 `slot_element("")` 认不出，那张图会无声消失。
            _ if 已归槽(&mut out.assets, &kid.name, &value) => {}
            // 我们自己写出去的那几个扩展元素（`x-romcat-*`）。
            name if name.starts_with(EXTRA_PREFIX) => {
                out.extra.insert(
                    name[EXTRA_PREFIX.len()..].to_string(),
                    vec![kid.text.clone()],
                );
            }
            // 这个格式认得、中立模型没有对应概念的元素：`kidgame`、`altemulator`、
            // `controller`、Batocera 的 `crc32`……**原样留着**，不报成有损点——
            // 它们在**前端里是生效的**，报成「这一行本来就没生效」是假警报。
            name => {
                out.unknown.insert(name.to_string(), vec![kid.text.clone()]);
            }
        }
    }
    out
}

/// 这个元素是不是一条媒体路径；是就收进它的资源槽，并回答「收下了」。
///
/// 写成一个「查一次、就地收下」的判据，而不是「先问一次再查一次」：后者中间那一步
/// 兜底成空串时，写回去认不出那个槽，那张图会无声消失。
fn 已归槽(assets: &mut BTreeMap<String, Vec<String>>, name: &str, value: &str) -> bool {
    let Some(slot) = media_slot(name) else {
        return false;
    };
    if !value.is_empty() {
        assets
            .entry(slot.to_string())
            .or_default()
            .push(value.to_string());
    }
    true
}

fn non_empty(value: &str) -> Option<String> {
    (!value.is_empty()).then(|| value.to_string())
}

fn push_non_empty(out: &mut Vec<String>, value: &str) {
    // `unknown` 是原版 ES 的默认值，写出来与不写出来是同一个效果（源码「等于默认值
    // 的字段不写出」）。把它当成一个真的开发商名字落进中立库，是给库里灌噪音。
    if !value.is_empty() && value != "unknown" {
        out.push(value.to_string());
    }
}

/// 我们自己的扩展元素的前缀。
///
/// ES 家族没有官方的扩展机制（`x-*` 是 Pegasus 那一侧的），四个变体里只有 Batocera
/// **保证**原样往返未知元素。所以这几个元素在别家前端里随时可能被清掉——但它们的
/// 用处是「下一次导入时精确对回中立库里的那个变体」，而**真正的身份是 `<path>`**
/// （调研 D.4 第 2 条：主键用路径），扩展元素只是锦上添花。
pub const EXTRA_PREFIX: &str = "x-";

/// 别的 ES 变体写在 gamelist 里的媒体元素 ↔ 规范资源槽。
///
/// **一张表两个方向**，不是两个互为反表的 `match`：那种写法改一边忘一边，媒体路径
/// 就静默消失。每行第一列是**写回去用的那个元素名**（读的时候认全部别名，写的时候
/// 只用这一个），第二列是规范槽，其余是只读得进来的别名。
///
/// ES-DE 自己不写也不读这些元素（它靠文件名找媒体），但**别人分享的元数据包里到处
/// 都是**——原版 ES、Batocera 与 Recalbox 都写它们。
const MEDIA_ELEMENTS: &[(&str, &str, &[&str])] = &[
    ("image", "boxFront", &["boxart"]),
    ("thumbnail", "boxFull", &[]),
    ("marquee", "marquee", &[]),
    ("video", "video", &[]),
    ("fanart", "background", &[]),
    ("titleshot", "titlescreen", &[]),
    ("boxback", "boxBack", &[]),
    ("wheel", "logo", &[]),
    ("cartridge", "cartridge", &[]),
];

/// 一个元素名落在哪个规范资源槽上。认不出是 `None`。
fn media_slot(name: &str) -> Option<&'static str> {
    MEDIA_ELEMENTS
        .iter()
        .find(|(element, _, aliases)| *element == name || aliases.contains(&name))
        .map(|(_, slot, _)| *slot)
}

/// 一个规范资源槽写回去用哪个元素名。认不出是 `None`。
fn slot_element(slot: &str) -> Option<&'static str> {
    MEDIA_ELEMENTS
        .iter()
        .find(|(_, canonical, _)| *canonical == slot)
        .map(|(element, _, _)| *element)
}

fn strip_dot_slash(value: &str) -> &str {
    value.strip_prefix("./").unwrap_or(value)
}

fn round_to_tenth(value: f64) -> f64 {
    (value / 0.1).round() / 10.0
}

/// 读 `rating`。源码 `stof` 之后 clamp 到 0–1。
#[must_use]
pub fn parse_rating(raw: &str) -> Option<f64> {
    let value: f64 = raw.trim().parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    Some(value.clamp(0.0, 1.0))
}

/// 写 `rating`。**最短往返形式**——序列化用 `std::stringstream`，写 `0.7` 而不是
/// `0.700000`（调研 §B.1 源码补充第 4 条）。
#[must_use]
pub fn format_rating(value: f64) -> String {
    let clamped = value.clamp(0.0, 1.0);
    // 载入时反正要四舍五入到 0.1，写一个它存不下的精度只是自欺。
    // `{}` 出的正是最短往返形式：`0.7` 而不是 `0.700000`，满分是 `1` 而不是 `1.0`
    // ——`std::stringstream` 那一侧写出来也是这两个样子。
    format!("{}", round_to_tenth(clamped))
}

/// 读 `players`。自由文本，`4` 与 `1-2` 都接得住；别的写法交给 `None`。
#[must_use]
pub fn parse_players(raw: &str) -> Option<PlayerCount> {
    let raw = raw.trim();
    let (min, max) = match raw.split_once('-') {
        Some((low, high)) => (low.trim(), high.trim()),
        None => (raw, raw),
    };
    if min.is_empty() || !min.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if max.is_empty() || !max.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(PlayerCount {
        min: min.parse().ok()?,
        max: max.parse().ok()?,
    })
}

/// 写 `players`。**区间原样写出去**——这个格式的 `players` 是字符串，下界不会丢
/// （与 Pegasus 的 `std::max(a,b)` 正好相反，那边只好把下界塞进扩展键）。
#[must_use]
pub fn format_players(players: PlayerCount) -> String {
    if players.is_range() {
        format!("{}-{}", players.min, players.max)
    } else {
        players.max.to_string()
    }
}

/// 读 `releasedate` / `lastplayed` 的 `%Y%m%dT%H%M%S`，如 `19950311T000000`。
#[must_use]
pub fn parse_datetime(raw: &str) -> Option<ReleaseDate> {
    let raw = raw.trim();
    let date = raw.split('T').next()?;
    if date.len() != 8 || !date.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i32 = date[..4].parse().ok()?;
    let month: u8 = date[4..6].parse().ok()?;
    let day: u8 = date[6..8].parse().ok()?;
    // `19700101T000000` 是 ES-DE 的默认值，被特判成 epoch 0（源码补充第 3 条）。
    // 它的意思是「没有发行日期」，不是「1970 年 1 月 1 日」。
    if year == 1970 && month == 1 && day == 1 {
        return None;
    }
    Some(ReleaseDate {
        year,
        month: Some(month),
        day: Some(day),
    })
}

/// 写 `releasedate`。**这个格式没有精度这一说**：只知道年份的日期也得写出月与日，
/// 于是补一个 1 月 1 日——那是格式的损失，[`Parsed::lossy`] 那一侧逐条说了出来。
#[must_use]
pub fn format_datetime(date: ReleaseDate) -> String {
    format!(
        "{:04}{:02}{:02}T000000",
        date.year,
        date.month.unwrap_or(1),
        date.day.unwrap_or(1)
    )
}

// ════════════════════════════════════════════════════════════════════════
// 转义
// ════════════════════════════════════════════════════════════════════════

/// 把 XML 的转义还原成字符。认不出的实体引用**原样留着**——猜错了就是改写用户的字。
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let Some(end) = after.find(';') else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let entity = &after[1..end];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            other => other
                .strip_prefix('#')
                .and_then(|digits| match digits.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),
                    None => digits.parse::<u32>().ok(),
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&after[..=end]),
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 把一个值转义成能写进元素里的样子。
///
/// 只转 `&` `<` `>` 三个——`"` 与 `'` 在元素内容里是合法字符，转了只是把文件变丑，
/// 而 pugixml 写出来的也是这三个。
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

// ════════════════════════════════════════════════════════════════════════
// 写
// ════════════════════════════════════════════════════════════════════════

/// 生成内容用的缩进。
const INDENT: &str = "    ";

/// 一个中立字段在这个格式里叫什么，以及写出去的顺序。
///
/// 顺序照原版 ES 的序列化顺序（`MetaData.cpp` 的声明顺序），`path` 在最前——那是
/// 这个格式的惯例，只有 Recalbox 因为倒序循环把它写在了最后。
const ORDER: &[(&str, Field)] = &[
    ("path", Field::Path),
    ("name", Field::Title),
    ("sortname", Field::SortTitle),
    ("desc", Field::Description),
    ("rating", Field::Rating),
    ("releasedate", Field::Release),
    ("developer", Field::Developer),
    ("publisher", Field::Publisher),
    ("genre", Field::Genre),
    ("players", Field::Players),
];

/// 一个中立字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Path,
    Title,
    SortTitle,
    Description,
    Rating,
    Release,
    Developer,
    Publisher,
    Genre,
    Players,
}

/// 一个字段在这份文档里的值；没有值就不写这个元素（「等于默认值的字段不写出」）。
fn value_of(game: &Game, field: Field, file: &str) -> Option<String> {
    match field {
        // **`<path>` 一个条目只装得下一个。** 多文件条目在写的时候摊成多个 `<game>`，
        // 每个带自己的那一条——见 [`render`] 的注释。
        Field::Path => Some(format!("./{file}")),
        Field::Title => non_empty(&game.title),
        Field::SortTitle => game.sort_title.clone(),
        Field::Description => game.description.clone(),
        Field::Rating => game.rating.map(format_rating),
        Field::Release => game.release.map(format_datetime),
        Field::Developer => joined(&game.developers),
        Field::Publisher => joined(&game.publishers),
        Field::Genre => joined(&game.genres),
        Field::Players => game.players.map(format_players),
    }
}

/// 多家合成一条：这三个元素在这个格式里是**一个字符串**，一个 `<game>` 里只有一份。
///
/// 中立库里开发商是**集合**（数据源一个键写了几家就是几家）。装不下的出路只有两条，
/// 这里选的是**合成一条**：
///
/// - 留头一条、把其余的丢掉，等于**换掉一家公司**——留下哪一家由码位序决定
///   （`Priorities::pick_all` 交出来的先后，挂单 Q27），既不是第一家也不是主要那家。
/// - 合成一条，前端上显示得出全部几家，用户按开发商筛的时候至少还含得住
///   （`contains` 那个运算符）。
///
/// **读的那一侧不拆**（见 [`fold_game`] 里那三个元素）：真的 gamelist 里
/// `Sunsoft, Inc.` 这种带逗号的单值到处都是，按 `, ` 拆等于**改内容**，比合成一条坏
/// 得多。两侧因此不对称，代价说清楚：**「库 → 文件 → 库」这一趟两家会压成一条**，
/// 而档位（无损往返）说的是**「文件 → 库 → 文件」**，那一条仍旧逐字节成立
/// ——原文里的一整条读进来是一整条、写回去还是那一整条。
fn joined(values: &[String]) -> Option<String> {
    (!values.is_empty()).then(|| values.join(", "))
}

/// 一个条目要写成哪几条 `<path>`。
///
/// **这个格式一个 `<game>` 只装得下一个文件。** 中立库里一个条目可以挂着好几个变体
/// （**收敛**：一个条目、多个文件），而 ES 那边磁盘上每个 ROM 文件本来就各自是一个
/// 条目——gamelist.xml 只是给它们**补元数据**，不是定义条目清单。所以少写的那几条
/// 不会消失，只会变成一个光秃秃的文件名。摊成多条，每条带同一份元数据，才是这个格式
/// 里「这几个文件是同一部作品」唯一说得出口的说法。
fn paths_of(game: &Game, prefix: Option<&str>) -> Vec<String> {
    game.files
        .iter()
        .map(|file| relativize(file, prefix).to_string())
        .collect()
}

/// 把中立库的键化成相对那个**平台目录**的路径。
///
/// `<path>` 是相对平台目录解析的（源码 `createRelativePath()`，官方示例
/// `~/ROMs/c64/Multidisk/…` 对 `./Multidisk/…`），而中立库的键是相对主库根的
/// （ADR-0020）。两者差的正好是那一段。
///
/// **对不上就整条原样写出去**：数不出唯一的平台目录时前缀是合集名，而它未必是
/// 磁盘上那个目录。这时写完整的键，用户把 ES 那边的 ROM 目录指到主库根上照样对得
/// 起来，而 [`Adapter::rom_bases`] 那一侧也把主库根列进了候选。
fn relativize<'a>(file: &'a str, prefix: Option<&str>) -> &'a str {
    let Some(prefix) = prefix else { return file };
    file.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('/'))
        .unwrap_or(file)
}

/// 要从 `<path>` 里剥掉的那一段**平台目录**。
///
/// 来自文档里的**合集**段，取的是它的 `directory` 而不是名字：真库上 22 个平台里有
/// 12 个两者对不上，而 `<path>` 是相对**磁盘上那个目录**解析的。数不出唯一目录时
/// 退回合集名，剥不掉就整条原样写出去（见 [`relativize`]）。
///
/// ES gamelist 自己没有合集这个概念（平台是由文件摆在哪个目录下说的），所以读进来的
/// 文档里没有合集段，剥不掉也不必剥：那些路径本来就是相对平台目录的。
fn prefix_of(doc: &Document) -> Option<&str> {
    let collection = doc.entries.iter().find_map(|entry| entry.collection())?;
    Some(
        collection
            .directory
            .as_deref()
            .unwrap_or(collection.name.as_str()),
    )
}

/// 基线里哪一段归谁：段下标 → （新文档里第几个条目，它的第几个文件）。
///
/// 两趟，**由硬到软**：
///
/// 1. 跨格式那一层（[`transfer::rebase`](super::transfer)）已经定好的对应关系
///    （[`Entry::origin`]）——它比过变体的键、比过路径、比过标题，是最有依据的一条；
/// 2. 剩下的文件按 `<path>` 各自认领。**路径是这个格式的主键**（调研 D.4 第 2 条），
///    而摊开的第二个、第三个文件在共用那一层根本没有对应的条目可指。
///
/// 两趟都没认领到的段**原样留着**（[`Adapter::kept_verbatim`] 数的正是它们）。
fn claims(
    doc: &Document,
    snapshot: &Snapshot,
    prefix: Option<&str>,
) -> BTreeMap<usize, (usize, usize)> {
    let mut by_path: BTreeMap<String, usize> = BTreeMap::new();
    for (nth, span) in snapshot.blocks.iter().enumerate() {
        if let Some(path) = snapshot.block(*span).path() {
            by_path.entry(path).or_insert(nth);
        }
    }
    let mut claimed: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut taken: BTreeSet<usize> = BTreeSet::new();
    for (nth, entry) in doc.entries.iter().enumerate() {
        if entry.game().is_none() {
            continue;
        }
        if let Some(origin) = entry.origin
            && origin < snapshot.blocks.len()
            && taken.insert(origin)
        {
            claimed.insert(origin, (nth, 0));
        }
    }
    for (nth, entry) in doc.entries.iter().enumerate() {
        let Some(game) = entry.game() else { continue };
        let already = claimed
            .iter()
            .any(|(_, (owner, index))| *owner == nth && *index == 0);
        for (index, file) in paths_of(game, prefix).iter().enumerate() {
            if index == 0 && already {
                continue;
            }
            let Some(block) = by_path.get(&format!("./{file}")).copied() else {
                continue;
            };
            if taken.insert(block) {
                claimed.insert(block, (nth, index));
            }
        }
    }
    claimed
}

/// 把一份中立文档写成 gamelist。
fn render(doc: &Document, baseline: Option<(&Document, &Snapshot)>) -> Vec<u8> {
    let prefix = prefix_of(doc);
    let Some((was, snapshot)) = baseline else {
        return render_fresh(doc, prefix);
    };

    let claimed = claims(doc, snapshot, prefix);

    let mut out = String::new();
    if snapshot.bom {
        out.push('\u{feff}');
    }
    let mut cursor = 0usize;
    for (nth, span) in snapshot.blocks.iter().enumerate() {
        while cursor < span.start {
            // 新段插在 `</gameList>` 之前——而段与段之间的一切（`<folder>`、注释、
            // 缩进）原样搬。
            snapshot.nodes[cursor].render(&mut out);
            cursor += 1;
        }
        match claimed.get(&nth) {
            Some((owner, index)) => merge_block(
                &mut out,
                snapshot.block(*span),
                was.entries.get(nth),
                &doc.entries[*owner],
                *index,
                prefix,
            ),
            // 新文档里没有它：**照搬**。别人分享的包里那些我们没认出来的条目，
            // 不因为工具不认得就消失——那正是「合并而不是覆盖」。
            None => snapshot.block(*span).render(&mut out),
        }
        cursor = span.end;
    }
    // 基线上没有的段接在 `</gameList>` 之前；没有根元素时接在末尾。
    let close_at = snapshot.close_at.unwrap_or(snapshot.nodes.len());
    while cursor < close_at {
        snapshot.nodes[cursor].render(&mut out);
        cursor += 1;
    }
    let indent = indent_of(snapshot);
    for (nth, entry) in doc.entries.iter().enumerate() {
        let Some(game) = entry.game() else { continue };
        for (index, _) in paths_of(game, prefix).iter().enumerate() {
            if claimed
                .values()
                .any(|(owner, at)| *owner == nth && *at == index)
            {
                continue;
            }
            write_block(&mut out, game, index, prefix, &indent);
        }
    }
    while cursor < snapshot.nodes.len() {
        snapshot.nodes[cursor].render(&mut out);
        cursor += 1;
    }
    out.into_bytes()
}

/// 基线里 `<game>` 前面那一截缩进长什么样。新段照它写，排版才不打架。
fn indent_of(snapshot: &Snapshot) -> String {
    let Some(first) = snapshot.blocks.first() else {
        return INDENT.to_string();
    };
    if first.start == 0 {
        return INDENT.to_string();
    }
    let Node::Text(raw) = &snapshot.nodes[first.start - 1] else {
        return INDENT.to_string();
    };
    match raw.rsplit_once('\n') {
        Some((_, tail)) if tail.chars().all(char::is_whitespace) && !tail.is_empty() => {
            tail.to_string()
        }
        _ => INDENT.to_string(),
    }
}

/// 从头生成整份文件。
fn render_fresh(doc: &Document, prefix: Option<&str>) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\"?>\n");
    out.push_str("<gameList>\n");
    for entry in &doc.entries {
        let Some(game) = entry.game() else { continue };
        for (index, _) in paths_of(game, prefix).iter().enumerate() {
            write_block(&mut out, game, index, prefix, INDENT);
        }
    }
    out.push_str("</gameList>\n");
    out.into_bytes()
}

/// 从头写一段。
///
/// **一个用户状态元素都不写。** 那不是省事：工具既不生成也不覆盖（ADR-0006），
/// 而这里根本没有可写的东西——它们压根不在 [`Game`] 上。
fn write_block(
    out: &mut String,
    game: &Game,
    file_index: usize,
    prefix: Option<&str>,
    indent: &str,
) {
    let files = paths_of(game, prefix);
    let Some(file) = files.get(file_index) else {
        return;
    };
    let _ = writeln!(out, "{indent}<game>");
    for (element, field) in ORDER {
        if let Some(value) = value_of(game, *field, file) {
            let _ = writeln!(
                out,
                "{indent}{INDENT}<{element}>{}</{element}>",
                escape(&value)
            );
        }
    }
    for (slot, values) in &game.assets {
        let Some(element) = slot_element(slot) else {
            continue;
        };
        if let Some(value) = values.first() {
            let _ = writeln!(
                out,
                "{indent}{INDENT}<{element}>{}</{element}>",
                escape(value)
            );
        }
    }
    for (name, values) in &game.extra {
        if let Some(value) = values.first() {
            let _ = writeln!(
                out,
                "{indent}{INDENT}<{EXTRA_PREFIX}{name}>{}</{EXTRA_PREFIX}{name}>",
                escape(value)
            );
        }
    }
    for (name, values) in &game.unknown {
        if let Some(value) = values.first() {
            let _ = writeln!(out, "{indent}{INDENT}<{name}>{}</{name}>", escape(value));
        }
    }
    let _ = writeln!(out, "{indent}</game>");
}

/// 一段：照着基线那几个节点写，只重写真的变了的元素。
///
/// **「库里没有」不等于「用户想删掉它」**（同 Pegasus）：新文档在某个字段上是空的，
/// 就一律照搬基线那几行。用户状态、`kidgame`、`altemulator`、别人包里那些我们不认得
/// 的元素，都不会因为「中立库里恰好没有对应的值」而在一次导出里蒸发掉。
fn merge_block(
    out: &mut String,
    block: Block<'_>,
    was: Option<&Entry>,
    now: &Entry,
    file_index: usize,
    prefix: Option<&str>,
) {
    let Some(game) = now.game() else {
        block.render(out);
        return;
    };
    let files = paths_of(game, prefix);
    let Some(file) = files.get(file_index).cloned() else {
        block.render(out);
        return;
    };
    // 哪些元素变了。**没变的一律照搬原文**——这正是「逐字节相同」成立的地方。
    let mut changed: BTreeMap<&str, String> = BTreeMap::new();
    for (element, field) in ORDER {
        // **`<path>` 比的是基线自己那一条**，不是拿新路径去比新路径（那永远相等）。
        // 对不上就重写：别人分享的包里那一条指着他机器上的文件名，而这一段是靠标题
        // 对回来的——把它改成我们真有的那个文件，条目才在前端里点得开。
        let before = match field {
            Field::Path => was
                .and_then(super::Entry::game)
                .and_then(|game| game.files.first())
                .map(|file| format!("./{file}")),
            other => was
                .and_then(super::Entry::game)
                .and_then(|game| value_of(game, *other, &file)),
        };
        let after = value_of(game, *field, &file);
        if let Some(after) = after
            && Some(&after) != before.as_ref()
        {
            changed.insert(element, after);
        }
    }
    for (slot, values) in &game.assets {
        let Some(element) = slot_element(slot) else {
            continue;
        };
        let before = was
            .and_then(super::Entry::game)
            .and_then(|game| game.assets.get(slot))
            .and_then(|values| values.first());
        if let Some(after) = values.first()
            && Some(after) != before
        {
            changed.insert(element, after.clone());
        }
    }
    let extras: Vec<(String, String)> = game
        .extra
        .iter()
        .filter_map(|(name, values)| {
            let before = was
                .and_then(super::Entry::game)
                .and_then(|game| game.extra.get(name))
                .and_then(|values| values.first());
            let after = values.first()?;
            (Some(after) != before).then(|| (format!("{EXTRA_PREFIX}{name}"), after.clone()))
        })
        .collect();
    for (name, value) in &extras {
        changed.insert(name.as_str(), value.clone());
    }

    let kids = block.children();
    let mut written: BTreeSet<&str> = BTreeSet::new();
    // 段里第几个节点是某个要重写的子元素的开头 → 它到哪个节点为止。
    let mut rewrite: BTreeMap<usize, (usize, &str)> = BTreeMap::new();
    for kid in &kids {
        let Some((element, _)) = changed.get_key_value(kid.name.as_str()) else {
            continue;
        };
        rewrite.insert(kid.at, (block.element_end(kid.at), element));
    }

    let indent = block.child_indent();
    let mut index = block.span.start;
    // 段末尾的空白留到最后——新加的元素要插在 `</game>` 前面。
    while index < block.span.end {
        if let Some((end, element)) = rewrite.get(&index) {
            if written.insert(element) {
                let value = changed.get(element).cloned().unwrap_or_default();
                let _ = write!(out, "<{element}>{}</{element}>", escape(&value));
            }
            index = *end;
            continue;
        }
        block.nodes[index].render(out);
        index += 1;
    }
    // 基线里没有、这次新出现的元素，补在 `</game>` **之前**。
    let mut tail = String::new();
    for (element, value) in &changed {
        if written.contains(element) {
            continue;
        }
        let _ = write!(tail, "{indent}<{element}>{}</{element}>", escape(value));
    }
    if !tail.is_empty() {
        insert_before_close(out, &tail, block);
    }
}

/// 把补写的元素塞在这一段的 `</game>` 之前。
fn insert_before_close(out: &mut String, tail: &str, block: Block<'_>) {
    // 段的最后一个节点就是 `</game>`；它前面那一截空白（换行加缩进）也要留在
    // 补写内容的**后面**，不然 `</game>` 会贴到新元素屁股上。
    let suffix = block.close_suffix();
    // **段尾不在手上就接在末尾。** 一段写岔了的原文（比如某个子元素没有结束标签）
    // 会让重写那一步把 `</game>` 一并吞掉，于是 `out` 末尾根本不是段尾那几个字。
    // 那时宁可排版难看：往返当场不成立、档位自动降到双向（`assert_capability` 指得出
    // 第一处分岔），而那是**说得出口的失败**——按长度硬切一刀是 panic。
    if !out.ends_with(&suffix) {
        out.push_str(tail);
        return;
    }
    let rest = out.split_off(out.len() - suffix.len());
    out.push_str(tail);
    out.push_str(&rest);
}

/// 一个子元素从 `at` 起到哪个节点为止（不含）。
fn element_end(nodes: &[Node], span: Span, at: usize) -> usize {
    let Node::Start(tag) = &nodes[at] else {
        return at + 1;
    };
    if tag.self_closing {
        return at + 1;
    }
    let mut depth = 1usize;
    let mut index = at + 1;
    while index < span.end {
        match &nodes[index] {
            Node::Start(inner) if inner.name == tag.name && !inner.self_closing => depth += 1,
            Node::End { name, .. } if name == &tag.name => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    span.end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Collection, assert_capability};

    /// 一份**照真文件的样子**写的 gamelist：两个根元素、注释、别的变体写的媒体路径、
    /// 用户状态、我们不认得的元素、CRLF 混着 LF、属性用单引号、末尾没有换行。
    const 手上这份: &str = "<?xml version=\"1.0\"?>\r\n\
        <alternativeEmulator>\r\n\
        \x20   <label>Snes9x - Current</label>\r\n\
        </alternativeEmulator>\r\n\
        <gameList>\n\
        \x20   <!-- 2019 年开始攒的，别乱动 -->\n\
        \x20   <game id='12345'>\n\
        \x20       <path>./魂斗罗.zip</path>\n\
        \x20       <name>魂斗罗 &amp; 沙罗曼蛇</name>\n\
        \x20       <desc>横版射击</desc>\n\
        \x20       <rating>0.75</rating>\n\
        \x20       <releasedate>19880220T000000</releasedate>\n\
        \x20       <developer>Konami</developer>\n\
        \x20       <players>1-2</players>\n\
        \x20       <image>./media/魂斗罗.png</image>\n\
        \x20       <favorite>true</favorite>\n\
        \x20       <playcount>42</playcount>\n\
        \x20       <lastplayed>20240115T203000</lastplayed>\n\
        \x20       <playtime>7200</playtime>\n\
        \x20       <kidgame>false</kidgame>\n\
        \x20   </game>\n\
        \x20   <folder>\n\
        \x20       <path>./多碟</path>\n\
        \x20       <name>多碟游戏</name>\n\
        \x20   </folder>\n\
        \x20   <game>\n\
        \x20       <path>./沙罗曼蛇.zip</path>\n\
        \x20       <name>沙罗曼蛇</name>\n\
        \x20   </game>\n\
        </gameList>";

    #[test]
    fn 两个根元素的文件读得动而且往返一个字节都不差() {
        // ⚠️ `<alternativeEmulator>` 写在 `<gameList>` 外面，**技术上不是合法 XML**。
        // 标准解析器怼上去直接失败；词法这一层根本没有「根元素只许一个」这条规矩。
        let assertion = assert_capability(&Gamelist, 手上这份.as_bytes()).expect("读得动");
        assert!(
            assertion.identical,
            "第 {} 行分岔：原文 {:?}，写回去 {:?}",
            assertion.difference.as_ref().map_or(0, |d| d.line),
            assertion.difference.as_ref().map(|d| &d.expected),
            assertion.difference.as_ref().map(|d| &d.found),
        );
        assert_eq!(assertion.asserted, Capability::LosslessRoundTrip);
        // 两个根元素都还在。
        let written = String::from_utf8(
            Gamelist
                .write(
                    &Gamelist.read(手上这份.as_bytes()).expect("读得动").doc,
                    Some(&Gamelist.read(手上这份.as_bytes()).expect("读得动")),
                )
                .expect("写得出"),
        )
        .expect("是 UTF-8");
        assert!(written.contains("<alternativeEmulator>"), "{written}");
        assert!(written.contains("<gameList>"), "{written}");
    }

    #[test]
    fn 用户状态一个都不折进中立模型_只在快照里躺着() {
        let parsed = Gamelist.read(手上这份.as_bytes()).expect("读得动");
        let game = parsed.doc.entries[0].game().expect("第一段是游戏");
        // ADR-0006：工具**既不读也不写**。中立模型里连放它们的地方都没有——
        // 于是「生成一个 favorite」这条路径压根不存在。
        for name in ["favorite", "playcount", "lastplayed", "playtime"] {
            assert!(
                !game.unknown.contains_key(name),
                "{name} 不该落进中立模型：{:?}",
                game.unknown
            );
        }
        assert_eq!(parsed.preserved.user_state, 4, "四处用户状态数出来了");
        // 不是用户状态的标记照未知键的路走，一样原样留着。
        assert!(game.unknown.contains_key("kidgame"));
    }

    #[test]
    fn 导出时用户状态逐条原样搬过去_省略等于清零() {
        let baseline = Gamelist.read(手上这份.as_bytes()).expect("读得动");
        // 库里给了一个新标题——这是导出会改的那一行。
        let mut doc = baseline.doc.clone();
        let Body::Game(game) = &mut doc.entries[0].body else {
            panic!("第一段是游戏");
        };
        game.title = "魂斗罗（汉化）".to_string();
        let written = Gamelist.write(&doc, Some(&baseline)).expect("写得出");
        let text = String::from_utf8(written).expect("是 UTF-8");
        assert!(text.contains("<name>魂斗罗（汉化）</name>"), "{text}");
        // 收藏与游玩记录一条不少，**而且一个字都没改**。
        assert!(text.contains("<favorite>true</favorite>"), "{text}");
        assert!(text.contains("<playcount>42</playcount>"), "{text}");
        assert!(
            text.contains("<lastplayed>20240115T203000</lastplayed>"),
            "{text}"
        );
        assert!(text.contains("<playtime>7200</playtime>"), "{text}");
        // 我们不认得的元素、别的变体写的媒体路径、注释、`<folder>` 也都还在。
        assert!(text.contains("<kidgame>false</kidgame>"), "{text}");
        assert!(text.contains("<image>./media/魂斗罗.png</image>"), "{text}");
        assert!(text.contains("2019 年开始攒的"), "{text}");
        assert!(text.contains("<folder>"), "{text}");
        assert!(text.contains("id='12345'"), "属性的单引号也留着：{text}");
    }

    #[test]
    fn 从头生成时一个用户状态元素都不写() {
        // 「既不生成」的那一半。库里根本没有这些东西，于是写不出来。
        let doc = Document {
            entries: vec![Entry::new(Body::Game(Game {
                title: "魂斗罗".to_string(),
                files: vec!["FC/魂斗罗.zip".to_string()],
                ..Game::default()
            }))],
        };
        let text = String::from_utf8(Gamelist.write(&doc, None).expect("写得出")).expect("UTF-8");
        for name in USER_STATE {
            assert!(!text.contains(&format!("<{name}>")), "{name}：{text}");
        }
    }

    #[test]
    fn 别人分享的包里没认出来的条目照搬不删() {
        let baseline = Gamelist.read(手上这份.as_bytes()).expect("读得动");
        // 新文档只认领了第一段，第二段（沙罗曼蛇）整个不见了。
        let doc = Document {
            entries: baseline.doc.entries[..1].to_vec(),
        };
        let text = String::from_utf8(Gamelist.write(&doc, Some(&baseline)).expect("写得出"))
            .expect("UTF-8");
        assert!(
            text.contains("<name>沙罗曼蛇</name>"),
            "合并而不是覆盖：{text}"
        );
    }

    #[test]
    fn 多文件条目摊成多个段_每个带自己的路径() {
        // 这个格式一个 `<game>` 只装得下一个文件，而磁盘上每个 ROM 本来就各自是一个
        // 条目——少写的那几条不会消失，只会变成光秃秃的文件名。
        let doc = Document {
            entries: vec![
                Entry::new(Body::Collection(Collection {
                    name: "FC".to_string(),
                    directory: Some("FC".to_string()),
                    ..Collection::default()
                })),
                Entry::new(Body::Game(Game {
                    title: "魂斗罗".to_string(),
                    files: vec![
                        "FC/魂斗罗汉化.zip".to_string(),
                        "FC/魂斗罗日版.zip".to_string(),
                    ],
                    ..Game::default()
                })),
            ],
        };
        let text = String::from_utf8(Gamelist.write(&doc, None).expect("写得出")).expect("UTF-8");
        assert_eq!(text.matches("<game>").count(), 2, "{text}");
        // 平台目录那一段被剥掉了：`<path>` 是相对**它**解析的。
        assert!(text.contains("<path>./魂斗罗汉化.zip</path>"), "{text}");
        assert!(text.contains("<path>./魂斗罗日版.zip</path>"), "{text}");
        assert!(!text.contains("FC/"), "平台那一段不该出现：{text}");
        // 合集段自己不写出去——平台是由文件摆在哪个目录下说的。
        assert!(!text.contains("collection"), "{text}");
    }

    #[test]
    fn 剥的是磁盘上那个目录_不是平台名() {
        // 真库上 22 个平台里有 12 个两者对不上（`WII` 的目录叫 `Wii`）。
        // `<path>` 是相对**磁盘上那个目录**解析的，剥错一段就一条都指不着。
        let doc = Document {
            entries: vec![
                Entry::new(Body::Collection(Collection {
                    name: "WII".to_string(),
                    directory: Some("Wii".to_string()),
                    ..Collection::default()
                })),
                Entry::new(Body::Game(Game {
                    title: "塞尔达".to_string(),
                    files: vec!["Wii/塞尔达.rar".to_string()],
                    ..Game::default()
                })),
            ],
        };
        let text = String::from_utf8(Gamelist.write(&doc, None).expect("写得出")).expect("UTF-8");
        assert!(text.contains("<path>./塞尔达.rar</path>"), "{text}");
    }

    #[test]
    fn 数不出唯一目录时整条键原样写出去() {
        // 一个平台的内容散在多个平台目录里：说不出唯一的那一个，就不猜。
        let doc = Document {
            entries: vec![
                Entry::new(Body::Collection(Collection {
                    name: "FC".to_string(),
                    directory: None,
                    ..Collection::default()
                })),
                Entry::new(Body::Game(Game {
                    title: "魂斗罗".to_string(),
                    files: vec!["nes/魂斗罗.zip".to_string()],
                    ..Game::default()
                })),
            ],
        };
        let text = String::from_utf8(Gamelist.write(&doc, None).expect("写得出")).expect("UTF-8");
        assert!(text.contains("<path>./nes/魂斗罗.zip</path>"), "{text}");
    }

    #[test]
    fn 生成的文件自己也往返() {
        let doc = Document {
            entries: vec![Entry::new(Body::Game(Game {
                title: "甲 & 乙 <丙>".to_string(),
                files: vec!["FC/甲.zip".to_string()],
                rating: Some(0.8),
                players: Some(PlayerCount { min: 1, max: 4 }),
                release: Some(ReleaseDate::year_only(1988)),
                ..Game::default()
            }))],
        };
        let bytes = Gamelist.write(&doc, None).expect("写得出");
        let assertion = assert_capability(&Gamelist, &bytes).expect("读得动");
        assert!(assertion.identical, "{:?}", assertion.difference);
        // 转义写进去也读得回来。
        let back = Gamelist.read(&bytes).expect("读得动");
        let game = back.doc.entries[0].game().expect("是游戏");
        assert_eq!(game.title, "甲 & 乙 <丙>");
        // 人数是区间，这个格式**存得下下界**（与 Pegasus 的 `max(a,b)` 正相反）。
        assert_eq!(game.players, Some(PlayerCount { min: 1, max: 4 }));
    }

    #[test]
    fn 元数据落点是_gamelists_下每个平台目录一份() {
        assert_eq!(Gamelist.metadata_path("FC"), "gamelists/FC/gamelist.xml");
        assert_eq!(
            Gamelist.metadata_path("../etc"),
            "gamelists/.._etc/gamelist.xml",
            "合集名里的路径分隔符造不出别处的文件"
        );
    }

    #[test]
    fn 媒体按文件名约定铺_条目里一个路径都不写() {
        // 官方示例：ROM `c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u`
        // → `downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg`
        let placed = Gamelist
            .media_placement(
                "库/c64/Multidisk/Last Ninja 2/Last Ninja 2.m3u",
                MediaKind::Screenshot,
                "abc123",
                "jpg",
            )
            .expect("铺得出");
        assert_eq!(
            placed.path,
            "downloaded_media/c64/screenshots/Multidisk/Last Ninja 2/Last Ninja 2.jpg"
        );
        assert_eq!(
            placed.slot, None,
            "ES-DE 的 gamelist 里不再包含媒体路径，靠文件名找"
        );
        assert_eq!(
            Gamelist.media_placement("FC/甲.zip", MediaKind::Other, "abc", "png"),
            None,
            "认不出是什么的图一张都不铺"
        );
    }

    #[test]
    fn 路径以平台目录为基准解析回中立库的键() {
        let file = Path::new("/卡/gamelists/FC/gamelist.xml");
        let bases = Gamelist.rom_bases(file, Some(Path::new("/库")));
        assert_eq!(bases[0], PathBuf::from("/库/FC"), "先试平台目录");
        assert_eq!(bases[1], PathBuf::from("/库"), "再试主库根");
        // 与 ROM 同处一目录的那种摆法（原版 ES、Batocera、Recalbox）。
        let beside = Path::new("/库/FC/gamelist.xml");
        assert_eq!(
            Gamelist.rom_bases(beside, None),
            vec![PathBuf::from("/库/FC")]
        );
    }

    #[test]
    fn 评分与日期照源码的口味读写() {
        // 载入时四舍五入到 0.1（源码 `std::round(stof(v)/0.1f)/10.0f`）。
        assert_eq!(parse_rating("0.75"), Some(0.75));
        assert_eq!(format_rating(0.75), "0.8", "写它存得下的那个精度");
        assert_eq!(format_rating(0.7), "0.7", "最短往返形式，不是 0.700000");
        assert_eq!(parse_rating("不是数"), None);
        assert_eq!(parse_rating("2"), Some(1.0), "clamp 到 0–1");
        // `19700101T000000` 是默认值，意思是「没有发行日期」。
        assert_eq!(parse_datetime("19700101T000000"), None);
        assert_eq!(
            parse_datetime("19950311T000000"),
            Some(ReleaseDate {
                year: 1995,
                month: Some(3),
                day: Some(11)
            })
        );
        assert_eq!(
            format_datetime(ReleaseDate::year_only(1988)),
            "19880101T000000",
            "这个格式没有精度这一说，只知道年份也得补出月与日"
        );
    }

    #[test]
    fn 日期没有精度这件事逐条点名() {
        let parsed = Gamelist.read(手上这份.as_bytes()).expect("读得动");
        let 找 = |key: &str| parsed.lossy.iter().find(|note| note.key == key);
        assert_eq!(
            找("releasedate").map(|note| note.kind),
            Some(Lossy::Padded),
            "年份没丢，「只精确到年」这件事丢了"
        );
    }

    #[test]
    fn 缺少前导点斜杠的路径也认得() {
        // USERGUIDE 明确警告：从旧版 ES 搬来的文件常常没有这个前缀。
        let text = "<gameList><game><path>甲.zip</path><name>甲</name></game></gameList>";
        let parsed = Gamelist.read(text.as_bytes()).expect("读得动");
        assert_eq!(
            parsed.doc.entries[0].game().expect("是游戏").files,
            vec!["甲.zip".to_string()]
        );
        // 而且原样写得回去——我们不替用户把它「修好」成 `./甲.zip`。
        assert!(
            assert_capability(&Gamelist, text.as_bytes())
                .expect("读得动")
                .identical
        );
    }

    #[test]
    fn 带bom与自闭合标签的文件也往返() {
        let text = "\u{feff}<?xml version='1.0'?>\n<gameList>\n  <game/>\n  \
                    <game><path>./甲.zip</path><name>甲</name></game>\n</gameList>\n";
        let assertion = assert_capability(&Gamelist, text.as_bytes()).expect("读得动");
        assert!(assertion.identical, "{:?}", assertion.difference);
    }

    #[test]
    fn 不是utf8就不猜编码() {
        let error = Gamelist.read(&[0xC4, 0xE3]).expect_err("该拒绝");
        assert!(matches!(error, AdapterError::NotUtf8 { .. }));
    }

    #[test]
    fn 未知实体引用原样留着() {
        // 认不出的实体猜错了就是改写用户的字。
        assert_eq!(unescape("a&amp;b&nbsp;c&#65;"), "a&b&nbsp;cA");
        assert_eq!(escape("a&b<c>d"), "a&amp;b&lt;c&gt;d");
    }

    #[test]
    fn 子元素没有结束标签时不炸_只是往返不成立() {
        // 写岔了的原文一定会有。**它可以让往返不成立，但不许让工具崩**——
        // 与「不可读是第三种状态」（ADR-0021）是同一条纪律：说得出口的失败，不是崩。
        let 写岔了 = "<gameList><game><path>./甲.zip</path><name>甲</game></gameList>";
        let baseline = Gamelist.read(写岔了.as_bytes()).expect("读得动");
        let mut doc = baseline.doc.clone();
        let Body::Game(game) = &mut doc.entries[0].body else {
            panic!("是游戏");
        };
        game.title = "乙".to_string();
        game.genres = vec!["射击".to_string()];
        let bytes = Gamelist.write(&doc, Some(&baseline)).expect("写得出");
        let text = String::from_utf8(bytes).expect("UTF-8");
        assert!(text.contains("<name>乙</name>"), "{text}");
        assert!(text.contains("<genre>射击</genre>"), "{text}");
    }

    #[test]
    fn 改一个字段只重写那一个元素() {
        let baseline = Gamelist.read(手上这份.as_bytes()).expect("读得动");
        let mut doc = baseline.doc.clone();
        let Body::Game(game) = &mut doc.entries[0].body else {
            panic!("第一段是游戏");
        };
        game.developers = vec!["科乐美".to_string()];
        let text = String::from_utf8(Gamelist.write(&doc, Some(&baseline)).expect("写得出"))
            .expect("UTF-8");
        assert!(text.contains("<developer>科乐美</developer>"), "{text}");
        assert!(
            text.contains("<rating>0.75</rating>"),
            "没动的元素一个字都不改：{text}"
        );
        assert!(text.contains("<desc>横版射击</desc>"), "{text}");
    }

    #[test]
    fn 基线里没有的元素补在段末尾而不是段外() {
        let baseline = Gamelist.read(手上这份.as_bytes()).expect("读得动");
        let mut doc = baseline.doc.clone();
        let Body::Game(game) = &mut doc.entries[1].body else {
            panic!("第二段是游戏");
        };
        game.genres = vec!["射击".to_string()];
        let text = String::from_utf8(Gamelist.write(&doc, Some(&baseline)).expect("写得出"))
            .expect("UTF-8");
        let genre = text.find("<genre>").expect("补上了");
        let close = text.rfind("</game>").expect("有闭合");
        assert!(genre < close, "补在段里而不是段外：{text}");
        // 补完还得是能读回来的样子。
        assert!(
            assert_capability(&Gamelist, text.as_bytes())
                .expect("读得动")
                .identical,
            "{text}"
        );
    }

    // ════════════════════════════════════════════════════════════════════
    // 结构性损失：先量，再声明
    //
    // 量的一律是「**库 → 文件 → 库**」这一趟，而且**不给底本**。给了底本那一趟证的
    // 是「原文写回去一个字节都不差」（档位那件事），这里要量的是另一件：**把库里的值
    // 交给这个格式存一趟，它变成什么样**。
    // ════════════════════════════════════════════════════════════════════

    /// 一个游戏段从头写出去、再读回来，得到的**几段**。
    ///
    /// 交出来的是 `Vec`：这个格式一个 `<game>` 只装得下一个 `<path>`，多文件的条目
    /// 写出去就是好几段——那本身是要量的东西之一。
    fn 往返(game: Game) -> Vec<Game> {
        let doc = Document {
            entries: vec![Entry::new(Body::Game(game))],
        };
        let written = Gamelist.write(&doc, None).expect("写得出");
        let back = Gamelist.read(&written).expect("读得回来");
        back.doc
            .entries
            .iter()
            .map(|entry| entry.game().expect("是游戏段").clone())
            .collect()
    }

    /// 只有一个文件的条目往返一趟，取那唯一的一段。
    fn 一段往返(game: Game) -> Game {
        let mut back = 往返(game);
        assert_eq!(back.len(), 1, "一个文件就该只写出一段");
        back.remove(0)
    }

    /// 量结构性损失用的那个游戏段：一个标题、一个文件，别的都空着。
    fn 一个条目() -> Game {
        Game {
            title: "甲".to_string(),
            files: vec!["FC/甲.zip".to_string()],
            ..Game::default()
        }
    }

    #[test]
    fn 值两端的空白被掐掉_而_desc_与扩展键不掐() {
        // 单行那一族（源码里是 `MD_STRING`）读回来两端都被 `trim` 掉了。
        let 回来 = 一段往返(Game {
            title: "  甲  ".to_string(),
            sort_title: Some("\u{3000}JIA\t".to_string()),
            genres: vec!["  动作  ".to_string()],
            ..一个条目()
        });
        assert_eq!(回来.title, "甲", "标题两端的空白没了");
        assert_eq!(
            回来.sort_title.as_deref(),
            Some("JIA"),
            "全角空格与制表符一样掐"
        );
        assert_eq!(回来.genres, vec!["动作".to_string()]);

        // **不掐的有三样。** `desc` 在源码里是 `MD_MULTILINE_STRING`（别的是 `MD_STRING`），
        // 得装得下带排版的长文；`x-` 扩展键与未知元素则是**原样留着、一个字都不解释**，
        // 折进中立模型的那一步根本没经手它们。声明里这三样不能省——省了就成了
        // 「简介的排版也会没」，而那是假的。
        let 回来 = 一段往返(Game {
            description: Some("\u{3000}\u{3000}第一段  \n  第二段\n".to_string()),
            extra: BTreeMap::from([("通关".to_string(), vec!["  是  ".to_string()])]),
            unknown: BTreeMap::from([("kidgame".to_string(), vec!["\u{3000}false ".to_string()])]),
            ..一个条目()
        });
        assert_eq!(
            回来.description.as_deref(),
            Some("\u{3000}\u{3000}第一段  \n  第二段\n"),
            "简介一个字符都不掐"
        );
        assert_eq!(
            回来.extra.get("通关"),
            Some(&vec!["  是  ".to_string()]),
            "`x-` 扩展键两端的空白也原样回得来"
        );
        assert_eq!(
            回来.unknown.get("kidgame"),
            Some(&vec!["\u{3000}false ".to_string()]),
            "未知元素一样"
        );

        // 掐完不剩东西的，**那个元素整条不写**，读回来是空的——`desc` 在这一格上不例外
        // （它两端的空白留得住，但整条只剩空白的一样消失），而 `x-` 扩展键与未知元素
        // 连这一格都不例外地留着。
        let 回来 = 一段往返(Game {
            title: "   ".to_string(),
            sort_title: Some("  ".to_string()),
            description: Some("  \n  ".to_string()),
            genres: vec!["  ".to_string()],
            developers: vec!["\u{3000}".to_string()],
            extra: BTreeMap::from([("通关".to_string(), vec!["  ".to_string()])]),
            ..一个条目()
        });
        assert_eq!(回来.title, "", "标题整条没了");
        assert_eq!(回来.sort_title, None);
        assert_eq!(回来.description, None, "只有空白的简介一样整条没了");
        assert!(回来.genres.is_empty());
        assert!(回来.developers.is_empty());
        assert_eq!(
            回来.extra.get("通关"),
            Some(&vec!["  ".to_string()]),
            "只有空白的扩展键照样在——它根本没被掐"
        );

        let 空白 = STRUCTURAL_LOSSES
            .iter()
            .find(|loss| loss.what.contains("两端的空白"))
            .expect("值两端的空白那一条在");
        assert!(空白.what.contains("U+3000"), "全角空格得点名：{空白:?}");
        assert!(空白.becomes.contains("掐掉"), "{空白:?}");
        assert!(
            空白.becomes.contains("整条"),
            "掐空了元素就没了，这句不能省：{空白:?}"
        );
        assert!(
            空白.becomes.contains("desc") && 空白.becomes.contains("扩展键"),
            "不掐的那三样都得说出口——省了就成了「简介与扩展键的排版也会没」：{空白:?}"
        );
    }

    #[test]
    fn 多值字段合成一条_读回来分不出原先是几条() {
        // `<developer>` / `<publisher>` / `<genre>` 一个 `<game>` 里各只有一份，
        // 而中立库里它们是**集合**。写的那一侧按 `, ` 合成一条，读的那一侧**不拆**。
        let 回来 = 一段往返(Game {
            developers: vec!["科乐美".to_string(), "KCE东京".to_string()],
            publishers: vec!["甲".to_string(), "乙".to_string()],
            genres: vec!["动作".to_string(), "冒险".to_string()],
            ..一个条目()
        });
        assert_eq!(
            回来.developers,
            vec!["科乐美, KCE东京".to_string()],
            "两家压成一条"
        );
        assert_eq!(回来.publishers, vec!["甲, 乙".to_string()]);
        assert_eq!(回来.genres, vec!["动作, 冒险".to_string()]);

        // **两个不同的输入落到同一个结果上。** 本来就带 `, ` 的单值原样回来，
        // 于是「两家」与「一家名字里有逗号」读回来再也分不开——`Sunsoft, Inc.` 这种
        // 在真的 gamelist 里到处都是，按 `, ` 拆开是改内容，比合成一条坏得多。
        let 回来 = 一段往返(Game {
            developers: vec!["Sunsoft, Inc.".to_string()],
            ..一个条目()
        });
        assert_eq!(
            回来.developers,
            vec!["Sunsoft, Inc.".to_string()],
            "一整条，不拆"
        );

        let 多值 = STRUCTURAL_LOSSES
            .iter()
            // **按 `写了几条这件事` 找**：光是「几条」这个词太泛，别的 `what` 迟早撞上。
            .find(|loss| loss.what.contains("写了几条这件事"))
            .expect("多值字段那一条在");
        assert!(多值.becomes.contains("一条"), "压成一条得说出口：{多值:?}");
        assert!(
            多值.becomes.contains("分不开"),
            "两个输入撞成一个，这句不能省：{多值:?}"
        );
    }

    #[test]
    fn 字面上就叫_unknown_的开发商整条消失() {
        // 原版 ES 的 `developer` / `publisher` / `genre` **默认值就是 `unknown`**
        // （调研 §B.1 那张 18 个字段的表），而这个格式的规矩是「值等于默认值时不写出」。
        // 于是一家真的叫 `unknown` 的公司写进去，与「这一栏没填」在文件里是同一件事。
        let 回来 = 一段往返(Game {
            developers: vec!["unknown".to_string()],
            publishers: vec!["unknown".to_string()],
            genres: vec!["unknown".to_string()],
            ..一个条目()
        });
        assert!(
            回来.developers.is_empty(),
            "整条没了：{:?}",
            回来.developers
        );
        assert!(回来.publishers.is_empty());
        assert!(回来.genres.is_empty());

        // **只有它自己一条的时候才没。** 与别的值合成一条之后整条就不等于默认值了，
        // `unknown` 那一半反而留得住——声明里得说准到「单独一条」。
        let 回来 = 一段往返(Game {
            genres: vec!["unknown".to_string(), "动作".to_string()],
            ..一个条目()
        });
        assert_eq!(
            回来.genres,
            vec!["unknown, 动作".to_string()],
            "合成一条就留住了"
        );

        let 那条 = STRUCTURAL_LOSSES
            .iter()
            .find(|loss| loss.what.contains("unknown"))
            .expect("`unknown` 那一条在");
        assert!(那条.becomes.contains("整条"), "{那条:?}");
        assert!(
            那条.becomes.contains("单独") || 那条.what.contains("单独"),
            "说准到「单独一条」，不然合成一条那一格就成了假话：{那条:?}"
        );
    }

    #[test]
    fn 一个条目挂着几个文件_写出去就是几个条目() {
        // 这个格式一个 `<game>` 只装得下一个 `<path>`。**收敛**（一个条目、多个文件）
        // 于是在这里表达不出来：摊成几段、每段带同一份元数据，读回来就是**几个条目**。
        let 回来 = 往返(Game {
            files: vec!["FC/甲.zip".to_string(), "FC/甲2.zip".to_string()],
            genres: vec!["动作".to_string()],
            ..一个条目()
        });
        assert_eq!(回来.len(), 2, "摊成了两段");
        assert_eq!(回来[0].files, vec!["FC/甲.zip".to_string()]);
        assert_eq!(回来[1].files, vec!["FC/甲2.zip".to_string()]);
        // 元数据每一份都在——摊开不是把别的文件丢掉。
        for game in &回来 {
            assert_eq!(game.title, "甲");
            assert_eq!(game.genres, vec!["动作".to_string()]);
        }

        let 那条 = STRUCTURAL_LOSSES
            .iter()
            .find(|loss| loss.what.contains("同一部作品"))
            .expect("多文件条目那一条在");
        assert!(那条.becomes.contains("几个条目"), "{那条:?}");
        assert!(
            那条.becomes.contains("元数据"),
            "元数据每一份都还在，这句不能省——省了读着像丢了东西：{那条:?}"
        );
    }

    #[test]
    fn 中立库有而这个格式没有元素的那几样整条不见() {
        // [`ORDER`] 就是这个格式装得下的全部字段。中立文档按**并集**设计
        // （ADR-0003），落在并集里、却在这张表上没有对应元素的那几样，写出去一个字都没有。
        let 回来 = 一段往返(Game {
            summary: Some("一句话简介".to_string()),
            tags: vec!["中文".to_string(), "已通关".to_string()],
            launch: Some("retroarch -L fceumm {file.path}".to_string()),
            workdir: Some("/opt/retroarch".to_string()),
            ..一个条目()
        });
        assert_eq!(回来.summary, None, "一段式简介没有对应元素");
        assert!(回来.tags.is_empty(), "标签没有对应元素");
        assert_eq!(回来.launch, None, "启动命令没有对应元素");
        assert_eq!(回来.workdir, None, "工作目录没有对应元素");
        // **`desc` 装的是长描述，不是简介**：这一条不因为「反正 `desc` 空着」就把简介
        // 挪进去——那是替用户把两个字段合成一个。
        assert_eq!(回来.description, None);

        let 那条 = STRUCTURAL_LOSSES
            .iter()
            .find(|loss| loss.what.contains("一段式简介"))
            .expect("没有对应元素那一条在");
        assert!(那条.becomes.contains("一个字都不写"), "{那条:?}");
        for 词 in ["标签", "启动命令", "工作目录"] {
            assert!(那条.what.contains(词), "{词}也得点名：{那条:?}");
        }
    }

    #[test]
    fn 合集段整段不见() {
        // ES gamelist 里**没有合集这个概念**：平台是由文件摆在哪个目录下说的
        // （见 [`prefix_of`]）。于是一份带合集段的中立文档写出去，那一段连同它的
        // 简介与 `x-` 扩展键一个字都没有。
        let doc = Document {
            entries: vec![
                Entry::new(Body::Collection(Collection {
                    name: "FC".to_string(),
                    shortname: Some("nes".to_string()),
                    directory: Some("FC".to_string()),
                    summary: Some("红白机".to_string()),
                    extra: BTreeMap::from([("我的备注".to_string(), vec!["值".to_string()])]),
                    ..Collection::default()
                })),
                Entry::new(Body::Game(一个条目())),
            ],
        };
        let written = Gamelist.write(&doc, None).expect("写得出");
        let text = String::from_utf8(written.clone()).expect("UTF-8");
        assert!(!text.contains("红白机"), "合集的简介一个字都没写：{text}");
        assert!(!text.contains("我的备注"), "合集的扩展键也没写：{text}");
        let back = Gamelist.read(&written).expect("读得回来");
        assert!(
            back.doc
                .entries
                .iter()
                .all(|entry| entry.collection().is_none()),
            "读回来一个合集段都没有"
        );
        // **合集段的 `directory` 不是白丢的**：它在写的那一步用掉了——`<path>` 是相对
        // 那个平台目录解析的，那一段被剥掉了。
        assert!(
            text.contains("<path>./甲.zip</path>"),
            "平台目录剥掉了：{text}"
        );
        // **而这一趟读回来的键真的短了一截。** 「不是丢了、是起了作用」只在**真的导入**
        // 里成立——那时靠 `Adapter::rom_bases` 从盘上的平台目录重新锚定。这里既没有根
        // 也没有底本，剥掉的那一段就回不来了。声明里这个前提不能省。
        let 回来 = back.doc.entries[0].game().expect("是游戏段");
        assert_eq!(
            回来.files,
            vec!["甲.zip".to_string()],
            "库里的键 `FC/甲.zip` 读回来短了一截"
        );

        let 那条 = STRUCTURAL_LOSSES
            .iter()
            .find(|loss| loss.what.contains("合集段"))
            .expect("合集段那一条在");
        assert!(那条.becomes.contains("整段"), "{那条:?}");
        assert!(
            那条.becomes.contains("平台目录") && 那条.becomes.contains("重新锚定"),
            "`directory` 是用掉了不是丢了，而「靠什么才回得来」这个前提一样不能省：{那条:?}"
        );
    }

    #[test]
    fn 资源槽这个格式只认九个_一个槽也只写得下一条() {
        // [`MEDIA_ELEMENTS`] 那张表就是这个格式认得的全部槽。规范资源槽一共二十个
        // （Pegasus 那一侧的 `ASSET_TYPES`），落在表外的十一个写出去一个元素都没有。
        let 全部槽 = [
            "boxFront",
            "boxBack",
            "boxSpine",
            "boxFull",
            "cartridge",
            "logo",
            "poster",
            "marquee",
            "bezel",
            "panel",
            "cabinetLeft",
            "cabinetRight",
            "tile",
            "banner",
            "steam",
            "background",
            "music",
            "screenshot",
            "titlescreen",
            "video",
        ];
        let 回来 = 一段往返(Game {
            assets: 全部槽
                .iter()
                .map(|slot| ((*slot).to_string(), vec![format!("{slot}.png")]))
                .collect(),
            ..一个条目()
        });
        let 丢了: Vec<&str> = 全部槽
            .iter()
            .filter(|slot| !回来.assets.contains_key(**slot))
            .copied()
            .collect();
        assert_eq!(
            丢了,
            [
                "boxSpine",
                "poster",
                "bezel",
                "panel",
                "cabinetLeft",
                "cabinetRight",
                "tile",
                "banner",
                "steam",
                "music",
                "screenshot"
            ],
            "这十一个槽这个格式没有对应元素"
        );

        // 一个槽里几条路径，**只写第一条**——这个格式一个 `<game>` 里每个媒体元素也只有一份。
        let 回来 = 一段往返(Game {
            assets: BTreeMap::from([(
                "boxFront".to_string(),
                vec!["头一张.png".to_string(), "第二张.png".to_string()],
            )]),
            ..一个条目()
        });
        assert_eq!(
            回来.assets.get("boxFront"),
            Some(&vec!["头一张.png".to_string()]),
            "第二张没了"
        );

        let 那条 = STRUCTURAL_LOSSES
            .iter()
            .find(|loss| loss.what.contains("资源槽"))
            .expect("资源槽那一条在");
        assert!(
            那条.becomes.contains("十一"),
            "丢了几个槽得数得出来：{那条:?}"
        );
        assert!(
            那条.becomes.contains("第一条"),
            "一个槽只写得下一条，这句不能省：{那条:?}"
        );
    }

    #[test]
    fn 扩展键与未知元素一个键只写第一条_键名带空白的整条不见() {
        // 一个键几个值：**只写第一条**。这个格式里一个元素就是一份值，没有续行一说。
        let 回来 = 一段往返(Game {
            extra: BTreeMap::from([(
                "通关".to_string(),
                vec!["是".to_string(), "两周目".to_string()],
            )]),
            unknown: BTreeMap::from([(
                "kidgame".to_string(),
                vec!["false".to_string(), "true".to_string()],
            )]),
            ..一个条目()
        });
        assert_eq!(回来.extra.get("通关"), Some(&vec!["是".to_string()]));
        assert_eq!(
            回来.unknown.get("kidgame"),
            Some(&vec!["false".to_string()])
        );

        // **键名里有空白或 `/` 的，键与值一起不见。** 写标签名那一步不转义，而词法扫
        // 名字时撞上这两样就收尾，`<x-我的 备注>` 于是不是一个认得回来的标签，读那一侧
        // 当整行文本跳过。Pegasus 的 `x-` 键是自由文本（`x-我的 备注` 完全合法），
        // 跨格式搬过来正撞上。
        for 键 in ["我的 备注", "我的/备注"] {
            let 回来 = 一段往返(Game {
                extra: BTreeMap::from([((*键).to_string(), vec!["值".to_string()])]),
                ..一个条目()
            });
            assert!(回来.extra.is_empty(), "{键}：键与值一起没了");
            assert!(回来.unknown.is_empty(), "{键}：也没落到未知元素里");
        }
        // **`>` 那一格的坏法不一样，也更坏**：键被截断、值被前一段键名污染，拿回来的是
        // 一个**看着正常的错值**，不是一个空缺。声明里这一格不能与上面那两样混着说。
        let 回来 = 一段往返(Game {
            extra: BTreeMap::from([("我的>备注".to_string(), vec!["值".to_string()])]),
            ..一个条目()
        });
        assert_eq!(
            回来.extra.get("我的"),
            Some(&vec!["备注>值".to_string()]),
            "键截成了 `我的`，值成了 `备注>值`"
        );
        // 别的字符回得来——判据是**是不是词法的分隔符**，不是「合不合 XML 规范」：
        // `<x-我的&备注>` 与 `<x-3周目>` 一样不是合法 XML 标签，却原样回得来。
        for 键 in ["我的&备注", "我的<备注", "我的\"备注", "3周目"] {
            let 回来 = 一段往返(Game {
                extra: BTreeMap::from([((*键).to_string(), vec!["值".to_string()])]),
                ..一个条目()
            });
            assert_eq!(
                回来.extra.get(键),
                Some(&vec!["值".to_string()]),
                "{键} 回得来"
            );
        }

        let 那条 = STRUCTURAL_LOSSES
            .iter()
            // **按 `第二个值` 找。** 「扩展键」「未知元素」这两个词在第 1 条与第 6 条的
            // `what` 里也有，按它们找就成了「谁排在前面找到谁」——清单一重排这条测试
            // 就指错条目（这一格已经踩过两回）。
            .find(|loss| loss.what.contains("第二个值"))
            .expect("扩展键与未知元素那一条在");
        assert!(那条.becomes.contains("第一条"), "{那条:?}");
        assert!(
            那条.what.contains("空白"),
            "键名里的空白是丢东西的那一半，不能省：{那条:?}"
        );
        assert!(
            那条.becomes.contains("污染"),
            "`>` 那一格拿回来的是个看着正常的错值，这句不能省：{那条:?}"
        );
    }

    #[test]
    fn 结构性损失不看库里当下有什么() {
        // 一份**一样都没撞上**的文档，清单照样说得出这八条——它说的是这个格式结构上
        // 做不到什么，不是「这一趟丢了几条」。写成后者就成了报告的活。
        let 回来 = 一段往返(Game {
            title: "一个字都不会变形的标题".to_string(),
            description: Some("横版射击".to_string()),
            developers: vec!["科乐美".to_string()],
            ..一个条目()
        });
        assert_eq!(回来.title, "一个字都不会变形的标题", "这一趟一个字都没丢");
        assert_eq!(回来.description.as_deref(), Some("横版射击"));
        assert_eq!(回来.developers, vec!["科乐美".to_string()]);
        assert_eq!(回来.files, vec!["FC/甲.zip".to_string()]);

        assert!(
            !Gamelist.structural_losses().is_empty(),
            "这一趟没撞上，声明照样在；空清单读作**还没查过**，量过了就不许再是空的"
        );
        // **钉住条数。** 上面那句只挡得住「清空」，挡不住「悄悄少一条」，而模块文档、
        // 票据与迁回去的挂单都写着八条——删掉一条那几处当场变成假话，却没有任何东西会红。
        assert_eq!(
            Gamelist.structural_losses().len(),
            8,
            "量出来的是八条；改了条数，文档与票据里那个数也得跟着改"
        );
    }
}
