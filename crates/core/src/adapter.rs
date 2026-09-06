//! **适配器**：把某一种前端格式与**中立库**对接的自包含模块（ADR-0003）。
//!
//! 每个格式一个适配器，负责该格式的读、写与字段映射。第一个是
//! [`pegasus`]，第二个是 [`gamelist`]，两个都做到**无损往返**档。
//!
//! ## 能力档位是**断言**不是声明
//!
//! [`Capability`] 有四档。适配器只声明自己的**上限**（[`Adapter::ceiling`]），
//! 实际档位由 [`assert_capability`] 在**手上这份真文件**上跑一趟往返测出来：读进来、
//! 写回去、逐字节比。过了才算无损往返，没过**自动降档**并说清差在哪一行。
//!
//! 这不是形式主义。ADR-0003 的修订段已经说明「无损往返不可能靠格式本身达成」：
//!
//! - **Pegasus 自己就是有损的**：`rating` 写 `85` 会被静默丢弃，`players` 的 `1-4`
//!   存进去变成 `4`，`release` 写 `1985` 读回来是 `1985-01-01`。
//! - 因此往返只能靠**旁路快照**——导入时把原文件逐字节留下来，导出时以它为基线，
//!   中立模型表达不了的部分（未知键、`x-*` 扩展键、注释、字段顺序）从它还原。
//!
//! 一个只会 `memcpy` 原文的实现也能「通过」往返，那毫无意义。所以快照存的是
//! **逐行拆开的结构**（[`pegasus::Snapshot`]），渲染时从结构里重新拼回去——
//! 逐字节相同这件事，证的是**词法真的把每一样都接住了**，包括缩进、行尾与 BOM。
//!
//! ## 档位还有说不出的一半：**结构性损失**
//!
//! 四个词答的是「读得进吗、写得出吗、写回去一样吗」。它答不了「**把值交给这个格式
//! 存一趟，它自己会把值改成什么样**」——那既不是读写失败，也不妨碍带**底本**的
//! 逐字节往返（原文那几行原样躺在快照里），却真的会让**新生成**的内容变形。
//!
//! [`StructuralLoss`] 补的正是这一半：每个适配器把自己格式**结构上**装不下的东西
//! 逐条声明出来，[`Adapter::structural_losses`] 答。ADR-0003 要的「导出前就知道这个
//! 格式会丢掉什么」于是不必等到用户事后发现。
//!
//! **它与 [`LossyNote`] 分得很开**：那一条是「**手上这份文件**第几行会被吞掉」，
//! 数得出来、指得出行号；这一条是「**格式本身**做不到」，与库里当下有没有这样的值
//! 无关。把它写成「这一趟丢了 N 条」就等于降级成报告的活了。
//!
//! ## 中立文档是所有格式字段的并集
//!
//! [`Document`] 按 ADR-0003 的要求照**并集**设计：`players` 是区间而不是一个数、
//! `release` 带精度、`rating` 是 0–1 的浮点、`files` 与 `developers` 都是列表。
//! 眼下只有一个适配器用得上其中一半，照样立起来——否则加第二个适配器要反复重构模型。
//!
//! ## 用户状态不在这里（ADR-0006）
//!
//! 收藏、游玩次数、通关状态一律不在 [`Game`] 上。Pegasus 那一侧它们根本不在
//! `metadata.pegasus.txt` 里（收藏在 `favorites.txt`、游玩统计在 `stats.db`），
//! 工具**既不读也不写**。ES gamelist 那一侧它们**长在同一个文件里**，「不碰」于是
//! 变成「从快照原样搬运」——[`gamelist`] 靠的正是这一层备好的快照机制：那些元素
//! 一个都不折进 [`Game`]，只躺在快照里，导出时逐条原样搬回去（[`Preserved::user_state`]
//! 数的就是它们）。
//!
//! ## 格式在磁盘上怎么摆，也是适配器的事
//!
//! 两个格式的布局差得很远：Pegasus 是**一个合集一个文件**摊在导出目录根上、媒体按
//! 内容哈希躺在 `media/` 里、路径写进条目；ES-DE 是
//! `gamelists/<平台目录>/gamelist.xml` 加 `downloaded_media/<平台目录>/<类型>/`、
//! 媒体**靠文件名找**、条目里一个媒体路径都不写。
//! 于是 [`Adapter`] 除了读写还答三个布局问题——[`Adapter::metadata_path`]、
//! [`Adapter::rom_bases`]、[`Adapter::media_placement`]。放在适配器里而不是散在
//! `sync` 与 `converge` 里，是因为 ADR-0003 定的就是「一个格式一个**自包含**模块」。

pub mod converge;
pub mod gamelist;
pub mod pegasus;
pub mod report;
pub mod transfer;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::scrape::MediaKind;

/// 一个适配器对自己能做到什么程度的显式声明（`CONTEXT.md` 的**能力档位**）。
///
/// **对用户可见**：`romcat adapters` 列得出来，导出前说得出「这个格式会丢掉什么」，
/// 而不是让用户事后发现（ADR-0003）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Capability {
    /// 只读得进来，写不回去。
    ReadOnly,
    /// 只写得出去，读不进来。**成本极低**，多前端并用当场就成立（ADR-0003）。
    WriteOnly,
    /// 双向：读得进、写得出，但**往返会丢东西**。
    Bidirectional,
    /// **无损往返**：读进中立库再写回去，与原文件逐字节相同——包括中立模型建模不了的
    /// 未知字段、扩展键与注释。
    LosslessRoundTrip,
}

impl Capability {
    /// 打给用户、也存进报告的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "只读",
            Self::WriteOnly => "只写",
            Self::Bidirectional => "双向",
            Self::LosslessRoundTrip => "无损往返",
        }
    }

    /// 报告里固定的排列顺序，由弱到强。
    #[must_use]
    pub fn all() -> [Self; 4] {
        [
            Self::ReadOnly,
            Self::WriteOnly,
            Self::Bidirectional,
            Self::LosslessRoundTrip,
        ]
    }

    /// 往返没过时降到哪一档。
    ///
    /// **只降这一级**：读得进、写得出这两件事往返测试并没有推翻，被推翻的只有
    /// 「写回去与原文一样」。把它一路降到只写，等于把已经验证过的能力也一起否掉。
    #[must_use]
    pub fn downgraded(self) -> Self {
        match self {
            Self::LosslessRoundTrip => Self::Bidirectional,
            other => other,
        }
    }
}

/// 一条**结构性损失**：这个格式**结构上**装不下的一样东西。
///
/// 它是**能力档位说不出的那一半**（见模块文档）。四个档位答的是读得进、写得出、
/// 写回去一样不一样；这一条答的是**把一个值交给这个格式存一趟，它自己会把值改成
/// 什么样**——Pegasus 的简介里那个单个换行读回来是一个空格，就是这一种。
///
/// ## 它是**声明**，不是**计数**
///
/// 与 [`LossyNote`] 分得很开：那一条挂在某份文件的某一行上，这一条挂在**格式**上。
/// 因此它是 `&'static`、进不了任何计数器，也**不看库里当下有没有带换行的简介**——
/// 一份简介一行换行都没有的库，这两条照样成立、照样该在导出前说出口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct StructuralLoss {
    /// 哪一样东西会变形。
    pub what: &'static str,
    /// 交给这个格式存一趟，它会变成什么样。
    pub becomes: &'static str,
    /// 这个格式为什么避不开——**说得出为什么，才不会被当成待修的 bug**。
    pub why: &'static str,
}

impl StructuralLoss {
    /// 打给用户的那一行。
    ///
    /// **一处定死**：`romcat adapters`、导入报告、导出报告三处印的是同一句。
    /// 各写各的，用户在导出前读到的与导入后读到的就会是两种说法（票 02 的验收点名）。
    #[must_use]
    pub fn line(&self) -> String {
        format!("{}：{}\n    ——{}", self.what, self.becomes, self.why)
    }
}

/// 一个前端格式的适配器。
///
/// 三个方法就是全部：**它是什么**、**读**、**写**。写的时候多一个 `baseline`
/// 参数——那是无损往返的全部依据（见模块文档）。
pub trait Adapter {
    /// 这个格式叫什么。报告与命令行都用它。
    fn name(&self) -> &'static str;

    /// 声称能做到哪一档。**这只是上限**，实测档位见 [`assert_capability`]。
    fn ceiling(&self) -> Capability;

    /// 这个格式**结构上**装不下什么——[`Capability`] 那四个词说不出的那一半。
    ///
    /// **与手上这份文件无关**：它逐条描述格式本身的边界，不数这一趟撞上了几处
    /// （那是 [`Parsed::lossy`] 的活）。导出前就该说得出「这个格式会丢掉什么」是
    /// ADR-0003 点名的要求。
    ///
    /// 默认是空的，意思是**还没查过**，不是「这个格式什么都不丢」。与
    /// [`Accepts::Anything`](crate::capability::Accepts::Anything) 那条「没查过就不作
    /// 声称」是同一条纪律，理由也是同一个（ADR-0017：**错误比不转换更糟**，用户会以为
    /// 工具已经处理妥当）。报告因此对空清单只字不提，而不是印一句「无结构性损失」的
    /// 假保证。
    fn structural_losses(&self) -> &'static [StructuralLoss] {
        &[]
    }

    /// 这个格式的元数据文件默认叫什么。
    fn file_name(&self) -> &'static str;

    /// 读一份该格式的文件。
    ///
    /// # Errors
    /// 文件不是这个格式、或者编码不对时返回错误。
    fn read(&self, bytes: &[u8]) -> Result<Parsed, AdapterError>;

    /// 写出去。
    ///
    /// `baseline` 是导入时存下来的那份**旁路快照**。给了它，中立模型表达不了的部分
    /// （未知键、`x-*`、注释、字段顺序）从它逐字还原；不给就整份从头生成。
    ///
    /// # Errors
    /// 中立文档里有这个格式装不下的东西时返回错误。
    fn write(&self, doc: &Document, baseline: Option<&Parsed>) -> Result<Vec<u8>, AdapterError>;

    /// 基线里有几段**这次的中立文档一段都没认领**，因此原样留了下来。
    ///
    /// 导出报告直接印它，那是「一次往返没有蒸发别人的心血」的证据。**归适配器答**，
    /// 因为「一个条目占几段」是格式自己的事：Pegasus 一个条目一段，ES gamelist 一个
    /// `<game>` 只装得下一个文件，多文件条目于是摊成好几段。在共用那一层按
    /// [`Entry::origin`] 数，会把摊开的那几段全算成「没认领」——真库上一趟导出就是
    /// 19,442 段的谎。
    ///
    /// 默认按 [`Entry::origin`] 数：一个条目认领一段。
    fn kept_verbatim(&self, doc: &Document, baseline: &Parsed) -> u64 {
        let claimed: std::collections::BTreeSet<usize> = doc
            .entries
            .iter()
            .filter_map(|entry| entry.origin)
            .collect();
        baseline
            .doc
            .entries
            .iter()
            .enumerate()
            .filter(|(nth, entry)| entry.game().is_some() && !claimed.contains(nth))
            .count() as u64
    }

    /// 一个**合集**的元数据落在导出目录里的哪个**相对路径**上。
    ///
    /// 默认是「一个合集一个文件、全摊在根上」（Pegasus）。ES-DE 那一套要
    /// `gamelists/<平台目录>/gamelist.xml`，于是它自己覆盖这一条。
    fn metadata_path(&self, collection: &str) -> String {
        converge::file_name_for(collection, self.file_name())
    }

    /// 条目里那条**相对路径**该以哪几个目录为基准解析回中立库的键。
    ///
    /// **按顺序试，第一个在库里找得到变体的算数。** 给的是一串而不是一个：ES-DE 的
    /// gamelist 躺在 `gamelists/<平台目录>/` 下、ROM 却在 `<主库根>/<平台目录>/` 下，
    /// 而**别人分享的包里那一段写的未必是我们这份库的平台目录**——他机器上的目录叫
    /// 什么是他的事。多试一个主库根，那批就不必被报成「对不上库里的变体」。
    ///
    /// 默认是元数据文件自己所在的目录——Pegasus 的 `file:` 就是这样解析的。
    fn rom_bases(&self, file: &Path, root: Option<&Path>) -> Vec<PathBuf> {
        let _ = root;
        vec![file.parent().unwrap_or(Path::new(".")).to_path_buf()]
    }

    /// 一份媒体铺到**子库**的哪个落点上，以及这条路径要不要写进条目。
    ///
    /// `rom_key` 是那个变体在中立库里的键——子库里的布局照搬它（挂账 D79），于是
    /// 「媒体路径镜像 ROM 路径」这类约定在这里算得出来。`None` 表示这一份不铺
    /// （认不出是什么的图就是这一档，见 [`crate::sync::media`] 的模块文档）。
    ///
    /// **没有默认实现**：铺法是格式的一部分，猜错一次就是把说明书扫描件当封面铺到
    /// 掌机上，或者铺了一堆前端根本找不到的文件。
    fn media_placement(
        &self,
        rom_key: &str,
        kind: MediaKind,
        hash: &str,
        ext: &str,
    ) -> Option<MediaPlacement>;
}

/// 一份媒体在子库里的落点。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPlacement {
    /// 相对子库根的路径。
    pub path: String,
    /// 写进条目的哪个**资源槽**。
    ///
    /// `None` 表示这个格式**靠文件名找媒体**，条目里一个媒体路径都不该写——ES-DE
    /// 正是这一种（官方原话：gamelist.xml 里不再包含媒体路径，应用按 ROM 文件名去找）。
    pub slot: Option<&'static str>,
}

/// 读一份文件的产物：中立模型看得懂的那一半，加上表达不了的那一半。
#[derive(Debug, Clone, PartialEq)]
pub struct Parsed {
    /// 中立模型这一侧。
    pub doc: Document,
    /// **旁路快照**：原文逐字节。导出时以它为基线还原表达不了的部分。
    ///
    /// 这里存的是**字节**而不是某个适配器拆好的结构。理由是这一层要对所有格式成立：
    /// 拆开来的形状是各家自己的事（Pegasus 是逐行的属性表，ES gamelist 是两个根元素的
    /// XML），摆进这个跨格式的类型里就等于让第二个适配器一进来先改一遍它。
    /// 各家在 [`Adapter::write`] 里按自己的办法把这串字节重新拆开——那一步是确定的，
    /// 同一串字节拆两次是同一个结果。
    pub source: Vec<u8>,
    /// **留下来了什么**：这是「一次往返没有蒸发心血」的证据，报告直接印它。
    pub preserved: Preserved,
    /// 这份文件里**格式自己会吞掉**的写法。
    ///
    /// 它不是我们的损失，是 Pegasus 的：`rating: 85` 它读不进去，`players: 1-4`
    /// 的下界它存不下。**说出来**比悄悄放过强——用户手写的那一行本来就没生效。
    pub lossy: Vec<LossyNote>,
}

/// 一份文件里**中立模型表达不了、但原样留下来了**的那些东西各有多少。
///
/// 由适配器数出来，因为「一条注释」在各家格式里长得不一样。报告只管印。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Preserved {
    /// 一共几行（或者这个格式里对应的那个计数单位）。
    pub lines: u64,
    /// 几条注释。
    pub comments: u64,
    /// 几个中立模型不认的键。
    pub unknown_keys: u64,
    /// 几个扩展键（Pegasus 的 `x-*`）。
    pub extension_keys: u64,
    /// 几处**用户状态**（ADR-0006）。
    ///
    /// 收藏、游玩次数、游玩时长、通关状态、上次游玩。它们**一个都不折进中立模型**，
    /// 只躺在快照里，导出时逐条原样搬回去。Pegasus 那一侧这个数永远是 0——那些东西
    /// 根本不在 `metadata.pegasus.txt` 里；ES gamelist 那一侧它们长在同一个文件里，
    /// **导出时省略这些元素就等于把维护者的收藏与游玩记录清零**，所以这个数就是
    /// 「搬运真的发生了」的证据。
    pub user_state: u64,
}

/// 格式吞东西的三种吃法。**分开数**：把它们混成一个总数会把最要命的那种淹掉。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lossy {
    /// **整条丢弃**：这一行在前端里根本不存在。`rating: 85` 就是这一种。
    Dropped,
    /// **削平**：值进去了，但少了一半。`players: 1-4` 存成 `4`，下界没了。
    Flattened,
    /// **补足**：值进去了，格式却自作主张补了我们并不知道的一段。
    /// `release: 1985` 读回来是 `1985-01-01`——年份没丢，「只知道年份」这件事丢了。
    Padded,
}

impl Lossy {
    /// 报告里用的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Dropped => "整条丢弃",
            Self::Flattened => "削平",
            Self::Padded => "补足",
        }
    }

    /// 报告里固定的排列顺序，**由重到轻**。
    #[must_use]
    pub fn all() -> [Self; 3] {
        [Self::Dropped, Self::Flattened, Self::Padded]
    }
}

/// 一处**格式自己**会吞掉的写法。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LossyNote {
    /// 第几行（从 1 数）。
    pub line: usize,
    /// 哪个键。
    pub key: String,
    /// 原样写的是什么。
    pub value: String,
    /// 哪一种吃法。
    pub kind: Lossy,
    /// 会怎么被吞掉，给人看的一句。
    pub detail: String,
}

/// 一份中立文档：一个格式文件里的全部段，**按原文顺序**。
///
/// 顺序不是装饰。Pegasus 的语义是「一个 `game` 会被加入到该文件中**此前定义过的
/// 所有** collection」——写文件的顺序本身就有语义，重排一次就换了归属。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    /// 全部段。
    pub entries: Vec<Entry>,
}

/// 文档里的一段。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 它对应**基线快照**里的第几段；从头生成的段是 `None`。
    ///
    /// 导出时靠它逐字还原：有基线的段照着原文改，没有的段整段新写。
    pub origin: Option<usize>,
    /// 段的内容。
    pub body: Body,
}

impl Entry {
    /// 一段新生成的内容。
    #[must_use]
    pub fn new(body: Body) -> Self {
        Self { origin: None, body }
    }

    /// 这一段是不是一个游戏。
    #[must_use]
    pub fn game(&self) -> Option<&Game> {
        match &self.body {
            Body::Game(game) => Some(game),
            Body::Collection(_) => None,
        }
    }

    /// 这一段是不是一个合集。
    #[must_use]
    pub fn collection(&self) -> Option<&Collection> {
        match &self.body {
            Body::Collection(collection) => Some(collection),
            Body::Game(_) => None,
        }
    }
}

/// 段是合集还是游戏。
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    /// 合集段。
    Collection(Collection),
    /// 游戏段。
    Game(Game),
}

/// 一个**合集**段。
///
/// 它对应 `CONTEXT.md` 的**合集**——与平台正交的一组游戏。当前库里「目录 = 平台 =
/// 前端的 collection」三者恰好重合，但模型上是两回事（ADR-0011）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Collection {
    /// 合集名。
    pub name: String,
    /// 短名（`snes`、`nes`）。
    pub shortname: Option<String>,
    /// 这个合集的内容住在哪个**平台目录**下（[`path::platform_of_key`](crate::path::platform_of_key)
    /// 取的就是它）。
    ///
    /// **它与 [`name`](Self::name) 常常不是同一个词**：真库上 22 个平台里有 12 个
    /// 目录名与平台名对不上（`WII` 的目录叫 `Wii`、`PS1` 的叫 `ps`、`WS` 的叫 `wsc`）。
    /// 平台名是给人看的，平台目录是磁盘上的事实（ADR-0011：目录是强先验）。
    ///
    /// ES 家族要它：`es_systems.xml` 里那个 `<name>` **就是这个目录名**
    /// （`<path>%ROMPATH%/<name>`），而条目的 `<path>` 是相对它解析的。拿平台名顶上去，
    /// Android 与 Linux 上（大小写敏感）那 12 个平台一个都指不着。Pegasus 那一侧没有
    /// 对应的键，写出去的文件一个字都不因此改变。
    ///
    /// 一个平台的内容散在多个平台目录里时是 `None`——那时说不出唯一的那一个，
    /// 路径就整条原样写出去。
    pub directory: Option<String>,
    /// 集合级默认启动命令。
    pub launch: Option<String>,
    /// 一段式简介。
    pub summary: Option<String>,
    /// 长描述。
    pub description: Option<String>,
    /// 扫描规则里的搜索目录。
    pub directories: Vec<String>,
    /// 扫描规则里的扩展名。
    pub extensions: Vec<String>,
    /// 显式列出的文件。**合集段也收 `file` / `files`**（调研 A.3 的 `m_coll_attribs`）。
    pub files: Vec<String>,
    /// 启动时的工作目录。
    pub workdir: Option<String>,
    /// 合集的排序名。
    pub sort_title: Option<String>,
    /// 集合级默认资源。
    pub assets: BTreeMap<String, Vec<String>>,
    /// `x-*` 扩展键（去掉 `x-` 前缀）。**格式官方指定的保真通道。**
    pub extra: BTreeMap<String, Vec<String>>,
    /// 这个格式认得、但中立模型没有对应概念的键（如 Pegasus 的 `regex`）。
    ///
    /// **原样留着**。它们是「只此一家」的能力（调研摘要第 2 条），建模等于替用户
    /// 决定要不要，而快照能一字不差地还给他。
    pub unknown: BTreeMap<String, Vec<String>>,
}

/// 一个**游戏**段，也就是前端里的一个条目。
///
/// 字段按 ADR-0003 要的**并集**设计，不是按 Pegasus 现在用得上的那些。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Game {
    /// **显示标题**（票 15 挑出来的那一个）。
    pub title: String,
    /// **排序标题**，与显示标题分开（`CONTEXT.md`）。
    pub sort_title: Option<String>,
    /// 这个条目对应磁盘上的哪几份东西。**列表**：多碟、多变体都在这里。
    ///
    /// **第一条是首选变体**——作品级收敛之后默认启动的那一个（ADR-0012）。
    pub files: Vec<String>,
    /// 开发商。
    pub developers: Vec<String>,
    /// 发行商。
    pub publishers: Vec<String>,
    /// 类型。
    pub genres: Vec<String>,
    /// 标签。
    pub tags: Vec<String>,
    /// 人数。**是区间不是一个数**——`1-4` 的下界在 Pegasus 里存不下，
    /// 中立模型不跟着它一起丢。
    pub players: Option<PlayerCount>,
    /// 一段式简介。
    pub summary: Option<String>,
    /// 长描述。
    pub description: Option<String>,
    /// 发行日期，**带精度**。
    pub release: Option<ReleaseDate>,
    /// 评分，0.0–1.0。
    pub rating: Option<f64>,
    /// 这个条目自己的启动命令。
    pub launch: Option<String>,
    /// 启动时的工作目录。
    pub workdir: Option<String>,
    /// 资源：规范类型名 → 一到多个路径或 URL。
    pub assets: BTreeMap<String, Vec<String>>,
    /// `x-*` 扩展键（去掉 `x-` 前缀）。
    pub extra: BTreeMap<String, Vec<String>>,
    /// 这个格式认得、但中立模型没有对应概念的键。原样留着。
    pub unknown: BTreeMap<String, Vec<String>>,
}

/// 一个发行日期，**精度显式**。
///
/// 精度必须建模（调研 D.2 第 2 条）：Pegasus 接受 `1985` 但解析后补成 `1985-01-01`，
/// 模型里没有「只精确到年」的标记。中立模型跟着丢的话，导出时就会凭空写出一个
/// 谁也不知道的 1 月 1 日。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReleaseDate {
    /// 年。
    pub year: i32,
    /// 月；只知道年份时是 `None`。
    pub month: Option<u8>,
    /// 日；不知道时是 `None`。
    pub day: Option<u8>,
}

impl ReleaseDate {
    /// 只知道年份的那一档。
    #[must_use]
    pub fn year_only(year: i32) -> Self {
        Self {
            year,
            month: None,
            day: None,
        }
    }
}

/// 人数：一个区间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlayerCount {
    /// 最少几个人。
    pub min: u32,
    /// 最多几个人。
    pub max: u32,
}

impl PlayerCount {
    /// 只有一个数时，上下界相同。
    #[must_use]
    pub fn exactly(count: u32) -> Self {
        Self {
            min: count,
            max: count,
        }
    }

    /// 这是不是一个真区间——也就是**下界会在 Pegasus 里丢掉**的那一档。
    #[must_use]
    pub fn is_range(self) -> bool {
        self.min != self.max
    }
}

/// 适配器读写不下去的原因。
#[derive(Debug, thiserror::Error)]
pub enum AdapterError {
    /// 文件不是 UTF-8。
    ///
    /// **不猜编码**：Pegasus 自己按 UTF-8 读（官方文档写明首选 UTF-8），猜出来的
    /// GBK 就算蒙对了，那份文件在 Pegasus 里本来也是乱码——替用户「修好」它，
    /// 等于悄悄改写了他的原文，而这张票的全部意义是一个字节都不改。
    #[error(
        "{path} 不是 UTF-8（第 {offset} 个字节起）。Pegasus 按 UTF-8 读元数据文件，\
         这份文件在它那里本来也读不对。先转成 UTF-8 再导入。"
    )]
    NotUtf8 {
        /// 哪份文件。
        path: String,
        /// 第几个字节开始不对。
        offset: usize,
    },
    /// 中立文档里有这个格式装不下的东西。
    #[error("{format} 装不下：{detail}")]
    Unrepresentable {
        /// 哪个格式。
        format: &'static str,
        /// 哪里装不下。
        detail: String,
    },
}

/// 一次**往返实测**的结论。档位由它断言，不由适配器声明。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assertion {
    /// 适配器声称的上限。
    pub ceiling: Capability,
    /// 实测下来是哪一档。
    pub asserted: Capability,
    /// 写回去与原文逐字节相同吗。
    pub identical: bool,
    /// 不相同的话，第一处差在哪。
    pub difference: Option<Difference>,
}

/// 往返没过时，第一处差在哪。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    /// 第几行（从 1 数）。
    pub line: usize,
    /// 原文那一行。
    pub expected: String,
    /// 写回去那一行。
    pub found: String,
}

/// 在**手上这份真文件**上跑一趟往返，断言这个适配器实际到得了哪一档。
///
/// 读进来、以自己为基线写回去、逐字节比。过了就是无损往返档，没过**自动降一档**
/// 并指出第一处差异——这就是「档位是断言不是声明」的落地。
///
/// # Errors
/// 读或写失败时返回错误。**读不动不等于降档**：那说明这份文件根本不是这个格式，
/// 与「能不能往返」是两件事。
pub fn assert_capability(adapter: &dyn Adapter, bytes: &[u8]) -> Result<Assertion, AdapterError> {
    let parsed = adapter.read(bytes)?;
    let written = adapter.write(&parsed.doc, Some(&parsed))?;
    let identical = written == bytes;
    let ceiling = adapter.ceiling();
    Ok(Assertion {
        ceiling,
        asserted: if identical {
            ceiling
        } else {
            ceiling.downgraded()
        },
        identical,
        difference: if identical {
            None
        } else {
            Some(first_difference(bytes, &written))
        },
    })
}

/// 两份字节头一次分岔在哪一行。
fn first_difference(expected: &[u8], found: &[u8]) -> Difference {
    let left = String::from_utf8_lossy(expected);
    let right = String::from_utf8_lossy(found);
    let mut lefts = left.split('\n');
    let mut rights = right.split('\n');
    let mut line = 0usize;
    loop {
        line += 1;
        match (lefts.next(), rights.next()) {
            (None, None) => {
                return Difference {
                    line,
                    expected: "（到头了）".to_string(),
                    found: "（到头了）".to_string(),
                };
            }
            (a, b) if a != b => {
                return Difference {
                    line,
                    expected: a.unwrap_or("（原文到这里就没了）").to_string(),
                    found: b.unwrap_or("（写出来的到这里就没了）").to_string(),
                };
            }
            _ => {}
        }
    }
}

/// 本程序带的全部适配器。
///
/// 是一份**清单**而不是散落在各处的 `match`：`romcat adapters` 要列得出来，
/// 而「支持哪些格式」是增量工作不是版本级决策（ADR-0003）。
#[must_use]
pub fn all() -> Vec<Box<dyn Adapter>> {
    vec![Box::new(pegasus::Pegasus), Box::new(gamelist::Gamelist)]
}

/// 本程序带的全部适配器叫什么。
///
/// 单拎出来是因为**它有一个跨模块的消费者**：`romcat import` 把手工维护的元数据落成
/// `scrape_value`，源名就是适配器的名字，于是
/// [`scrape::all_source_names`](crate::scrape::all_source_names) 要认得它——
/// 漏了它，`priorities.toml` 里那一行会被报成「打错字的源」。
#[must_use]
pub fn names() -> Vec<&'static str> {
    all().iter().map(|adapter| adapter.name()).collect()
}

/// 按名字找一个适配器（大小写不敏感）。
#[must_use]
pub fn find(name: &str) -> Option<Box<dyn Adapter>> {
    all()
        .into_iter()
        .find(|adapter| adapter.name().eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 往返没过只降一档而不是一路降到只写() {
        // 读得进、写得出这两件事往返测试并没有推翻。
        assert_eq!(
            Capability::LosslessRoundTrip.downgraded(),
            Capability::Bidirectional
        );
        assert_eq!(
            Capability::Bidirectional.downgraded(),
            Capability::Bidirectional
        );
        assert_eq!(Capability::WriteOnly.downgraded(), Capability::WriteOnly);
    }

    #[test]
    fn 按名字找得到适配器() {
        assert!(find("pegasus").is_some());
        assert!(find("PEGASUS").is_some());
        assert!(find("es-gamelist").is_some());
        assert!(find("ES-Gamelist").is_some());
        assert!(find("没有这个格式").is_none());
    }

    #[test]
    fn 每个适配器的名字与默认文件名各不相同() {
        // 名字是 `priorities.toml` 里的源名，也是 `--format` 认的那个词；
        // 撞名等于两个格式抢同一列刮削值。
        let names = names();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), names.len(), "适配器的名字撞了：{names:?}");
    }

    #[test]
    fn 头一处差异指得出行号() {
        let diff = first_difference(b"a\nb\nc\n", b"a\nB\nc\n");
        assert_eq!(diff.line, 2);
        assert_eq!(diff.expected, "b");
        assert_eq!(diff.found, "B");
    }
}
