//! **Switch 的免密钥识别层**：容器的文件名表是明文，读它一个密钥都不要。
//!
//! Switch 的内容主体确实加密——NCA 整份是密文，读它的头要 `header_key`。但**容器那一层
//! 不加密**：PFS0（`.nsp` / `.nsz`）与 HFS0（`.xci` / `.xcz`）的头、条目表与**字符串表**
//! 全是明文，三个一手实现（hactool 的 `hfs0_process()`、nsz 的 `Pfs0.open()`、
//! switch-library-manager 的 `readPfs0()`）里一个解密调用都没有
//! （`docs/research/switch-identification.md` §2.2.2）。
//!
//! 而这个容器里的文件名本身就是内容的标识：
//!
//! | 文件名 | 是什么 | 说得出 |
//! |---|---|---|
//! | `<32 位 hex>.nca` / `.cnmt.nca` / `.ncz` / `.cnmt.ncz` | **ContentId** = 整个 NCA 的 SHA-256 前 16 字节 | 查 titledb 得到 (TitleID, 版本) |
//! | `<32 位 hex>.tik` | **RightsId** = TitleID ‖ 7 字节零 ‖ KeyGeneration | **前 16 位 hex 就是 TitleID** |
//! | `<32 位 hex>.cert` | 票据的证书 | 这是一份 CDN 包 |
//! | `*.cnmt.xml` / `*.nacp.xml` / `cardspec.xml` / `authoringtoolinfo.xml` | 转储工具写的明文 XML | 这是哪一种转储 |
//!
//! **这个工具绝不内置、绝不分发、绝不下载任何密钥。** 那不是保守，是有判决先例的红线
//! （Nintendo 对 Ryujinx 的 DMCA 通知逐字引 17 U.S.C. §1201，一次下架 575 个仓库，
//! 调研 §3.3）。这一层**只读容器元数据**，走到的正好是「TitleID + 精确版本 +
//! 本体/补丁/附属内容 + 结构指纹」——对本项目已经够用。
//!
//! **NCZ 头里那两个字段不碰。** `NCZSECTN` 结构里明文放着 `cryptoKey` 与
//! `cryptoCounter`，读它们会把这个工具从「读容器元数据」推向「规避技术措施」（调研
//! §2.3.2）。这一层压根不打开 `.ncz`——它只看文件名。
//!
//! ## `.nsz` / `.xcz` 不必先还原
//!
//! nsz 的 `docs/formats.md` 原话：NSZ 与 NSP、XCZ 与 XCI **功能上完全相同**，差别只在
//! 里面的 `.nca` 换成了 `.ncz`。压缩器改名字那一句是 `newFileName = path[0:-1]+"z"`
//! ——**只改最后一个字符，32 位 hex 的 ContentId 词干原样保留**。所以这一层认 `.ncz`
//! 与认 `.nca` 是同一行代码，零解压。
//!
//! （反过来说：NSZ / XCZ 的整文件哈希与对应的 NSP / XCI **毫无关系**——NCZ 是先解密
//! 再用 zstd 压的。所以任何按整文件哈希的数据库都对不上它们，而这一层根本不走那条路。）
//!
//! ## XCI 有两套偏移，别硬编码
//!
//! switchbrew 描述的是**整张卡的地址空间**，而实际转储文件从 CardHeader 开始——前面
//! `0x1000` 字节的 CardKeyArea 写完就再也读不出来，转储工具拿不到它。于是同一个
//! `"HEAD"` 魔数在 scene 风格的转储里位于文件偏移 `0x100`，在 FullXCI 里位于 `0x1100`。
//! 四个独立来源交叉验证过这个位移（调研 §2.1.1）。所以这里**两处都探**，探到哪儿算哪儿，
//! 此后所有偏移相对它算。
//!
//! ## 卡带头里没有 TitleID
//!
//! CardHeader 的明文部分只有 PackageId（挑战-应答用，**不是** TitleID），加密的那
//! `0x70` 字节里是系统更新的 UppId。**任何位置都没有游戏的 TitleID**。所以 XCI 的入口
//! 是 `secure` 分区那张 HFS0 文件名表，而不是卡带头本身。

use std::collections::BTreeMap;
use std::io::SeekFrom;

use crate::catalog::identify::{Candidate, Confidence};
use crate::dat::Convention;
use crate::dat::chinese::ChineseMark;
use crate::fs::ReadSeek;
use crate::path::file_name_of_key;
use crate::titledb::store::{Store, StoreError};
use crate::titledb::{Content, Title};

use super::ident::{IdKind, Ident};

/// 一份内容最多为这一层读多少字节。
///
/// 头、条目表、字符串表加起来通常只有几 KB；XCI 要多跳两次（卡带头 → 根 HFS0 →
/// `secure` 的 HFS0），跳过去的那几百 MB **不读**，只是 seek。这个上限挡的是畸形头
/// 声称自己有几百万个条目那种情形。
pub const BUDGET: u64 = 4 << 20;

/// 探 XCI 那两处 `"HEAD"` 要读到 `0x1104`，再加上 `0x1000` 那一套的
/// `PartitionFsHeaderAddress`（`0x1130`）。一次读到 `0x1200` 把两套都覆盖掉。
pub const HEAD_LEN: usize = 0x1200;

/// 一张分区表最多认多少条。真机上 PFS0 是个位数、XCI 的 `secure` 也是个位数；
/// 这个数挡的是畸形头（`0x40 × count` 直接把内存吃光）。
const MAX_ENTRIES: u32 = 4096;

/// 字符串表最大多大。同上，挡畸形头。
const MAX_STRING_TABLE: u32 = 1 << 20;

/// ContentId 与 RightsId 都是 16 字节，写成 32 个 hex 字符。
const ID_HEX: usize = 32;

/// RightsId 的前 16 位 hex 就是 TitleID（大端 8 字节）。
const TITLE_HEX: usize = 16;

/// 这个扩展名是 Switch 的四种之一吗。
///
/// **四种共用同一条代码路径**：`.nsp` 与 `.nsz` 都是裸 PFS0，`.xci` 与 `.xcz` 都是
/// 卡带头加 HFS0，压缩与否只改内部条目的扩展名（`.nca` → `.ncz`）。
#[must_use]
pub fn by_name(name: &str) -> bool {
    let lower = file_name_of_key(name).to_ascii_lowercase();
    [".nsp", ".nsz", ".xci", ".xcz"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

/// 这份内容是哪一种容器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Wrapper {
    /// 裸 PFS0：`.nsp` 与 `.nsz`。
    Pfs0,
    /// 卡带头加一棵 HFS0：`.xci` 与 `.xcz`。
    Xci,
}

impl Wrapper {
    /// 存进**中立库**用的短码。与 [`label`](Self::label) 分开写，理由同
    /// `disc::Shell::code`：改一次报告用词不该让已经存进库里的记录读不回来。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Pfs0 => "pfs0",
            Self::Xci => "xci",
        }
    }

    /// 报告与**依据**里写的那个名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Pfs0 => "PFS0",
            Self::Xci => "XCI",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Pfs0, Self::Xci]
            .into_iter()
            .find(|it| it.code() == code)
    }
}

/// **结构指纹**：这份转储是怎么来的。判据全是容器里的文件名，一个密钥都不要
/// （NX Game Info 的 Structure 分类，调研 §3.1）。
///
/// 它不产出 TitleID，但它说得出「这个包完不完整、是不是被人转换过」——而那正是
/// **裁决**要看的东西（ADR-0002）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Structure {
    /// scene 风格的 XCI：`update` / `normal` / `secure` 三个分区都在。
    SceneXci,
    /// 转换来的 XCI：**只有** `secure`，是从 NSP 转过来的。
    ConvertedXci,
    /// scene 风格的 NSP：带 `legalinfo` / `nacp` / `programinfo` / `cardspec` 四份 XML。
    SceneNsp,
    /// CDN 风格的 NSP：带 `.cert` 与 `.tik`，直接从 eShop 下下来的样子。
    CdnNsp,
    /// 转换来的 NSP：**没有** `.cert` 也没有 `.tik`，是从 XCI 转过来或去掉了 titlekey。
    ConvertedNsp,
    /// 自制应用：带 `authoringtoolinfo.xml`。
    Homebrew,
    /// 不完整：连一份 `.cnmt.nca` 都没有，说不出这个包该有哪些内容。
    Incomplete,
}

impl Structure {
    /// 存进中立库用的短码。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::SceneXci => "scene-xci",
            Self::ConvertedXci => "converted-xci",
            Self::SceneNsp => "scene-nsp",
            Self::CdnNsp => "cdn-nsp",
            Self::ConvertedNsp => "converted-nsp",
            Self::Homebrew => "homebrew",
            Self::Incomplete => "incomplete",
        }
    }

    /// 报告与依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SceneXci => "scene XCI",
            Self::ConvertedXci => "转换来的 XCI",
            Self::SceneNsp => "scene NSP",
            Self::CdnNsp => "CDN NSP",
            Self::ConvertedNsp => "转换来的 NSP",
            Self::Homebrew => "自制应用",
            Self::Incomplete => "不完整",
        }
    }

    /// 全部七种，顺序固定。
    #[must_use]
    pub fn all() -> [Self; 7] {
        [
            Self::SceneXci,
            Self::ConvertedXci,
            Self::SceneNsp,
            Self::CdnNsp,
            Self::ConvertedNsp,
            Self::Homebrew,
            Self::Incomplete,
        ]
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::all().into_iter().find(|it| it.code() == code)
    }
}

/// 这一份是本体、补丁，还是附属内容。
///
/// 判据是 TitleID 的最后三位 hex：本体恒 `000`、更新恒 `800`、其余是 AddOnContent
/// （调研 §1 与 §7 A 层的 `titleType`）。
///
/// **必须分得出来，不然库体检会把一堆更新包报成游戏**：真机上 `.nsp` 平均只有 512 MiB，
/// 82 个里相当一部分是更新与 DLC 而不是本体（调研的实现陷阱第 7 条）。用词表的词
/// （`CONTEXT.md`）：Switch 的更新包正是**补丁**（自己不可运行、把一个变体变换成另一个），
/// DLC 正是**附属内容**（有自己的名字与元数据、不能独立运行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// 本体。
    Base,
    /// **补丁**：更新包，TitleID 尾 `800`。
    Patch,
    /// **附属内容**：DLC，TitleID 尾既不是 `000` 也不是 `800`。
    AddOn,
}

impl Kind {
    /// 报告与依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Base => "本体",
            Self::Patch => "补丁",
            Self::AddOn => "附属内容",
        }
    }

    /// 存进中立库用的短码。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Base => "base",
            Self::Patch => "patch",
            Self::AddOn => "addon",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        [Self::Base, Self::Patch, Self::AddOn]
            .into_iter()
            .find(|it| it.code() == code)
    }

    /// 一个 TitleID 说自己是哪一种。
    #[must_use]
    pub fn of(title_id: &str) -> Option<Self> {
        let value = u64::from_str_radix(title_id, 16).ok()?;
        Some(match value & 0xFFF {
            0x000 => Self::Base,
            0x800 => Self::Patch,
            _ => Self::AddOn,
        })
    }

    /// 这一种的**本体** TitleID。
    ///
    /// 补丁与附属内容都挂在同一部**作品**下，而它们的 TitleID 由本体那一串算出来
    /// （调研实测 71,182 条 `rightsId` 与所在 title **100% 前 13 位 hex 相同**）：
    ///
    /// | 这一种 | 怎么折回去 | 例 |
    /// |---|---|---|
    /// | 本体 | 就是它自己 | `0100EAE019904000` |
    /// | 补丁 | 清掉 `0x800` 那一位 | `0100EAE019904800` → `0100EAE019904000` |
    /// | 附属内容 | 清掉低 12 位再减 `0x1000` | `0100EAE01990512C` → `0100EAE019904000` |
    ///
    /// 最后一行是要害：DLC 的号段在本体之上一档，**直接把尾三位抹成 `000` 会折到一个
    /// 根本不存在的 TitleID 上**（`…512C` 会折成 `…5000` 而不是 `…4000`）。
    #[must_use]
    pub fn base_of(title_id: &str) -> Option<String> {
        let value = u64::from_str_radix(title_id, 16).ok()?;
        let base = match Self::of(title_id)? {
            Self::Base => value,
            Self::Patch => value & !0x800,
            Self::AddOn => (value & !0xFFF).checked_sub(0x1000)?,
        };
        Some(format!("{base:016X}"))
    }
}

/// 一份内容的容器头探出来的全部事实。
///
/// 它落进中立库的 `content_switch`（与 `content_disc`、`content_cart` 同一条路）：
/// **算过的不再算**，第二趟识别在这一层上是零字节。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    /// 认出来的容器形态，存的是 [`Wrapper::code`] 那个短码；认不出是 `None`。
    pub wrapper: Option<String>,
    /// **结构指纹**，存的是 [`Structure::code`]。
    pub structure: Option<String>,
    /// XCI 的根分区名（`update` / `logo` / `normal` / `secure`）。PFS0 是空的。
    pub partitions: Vec<String>,
    /// 明文文件名表**原样**。依据里要写它——事后复核的人看的是这一串。
    pub entries: Vec<String>,
    /// 抽出来的 ContentId（32 位 hex，小写），去重后按字典序。
    pub content_ids: Vec<String>,
    /// 读出来的标识。眼下只有 `.tik` 文件名给出的 TitleID。
    pub ids: Vec<Ident>,
    /// 里面有 `.ncz` / `.cnmt.ncz` 吗——**这一份是压缩过的**。
    pub compressed: bool,
    /// 本体 / 补丁 / 附属内容，存的是 [`Kind::code`]；说不出来是 `None`。
    pub kind: Option<String>,
    /// 读不出想要的那一段时，说人话的一句。
    pub note: Option<String>,
}

impl Facts {
    /// 这一份说得出 TitleID 吗。
    #[must_use]
    pub fn title_id(&self) -> Option<&str> {
        self.ids.first().map(|id| id.key.as_str())
    }

    /// 这一层到底有没有读出东西来。
    ///
    /// **判据不是「有没有 TitleID」**：一份去掉了 titlekey 的包没有 `.tik`，但它的
    /// ContentId 照样是判据（查一次 titledb 就说得出是哪个游戏的哪个版本）。
    #[must_use]
    pub fn usable(&self) -> bool {
        !self.ids.is_empty() || !self.content_ids.is_empty()
    }
}

/// 取一段字节的办法。
///
/// 两个实现，因为这一层的两种处境**取字节的代价完全不同**：
///
/// - [`Prefix`]：手上只有前面那一段（**透明容器**里的一条只解压得到前缀）。够不着的
///   位置如实答 `None`，而不是装作读到了空。
/// - [`Seeked`]：裸文件，能 seek。XCI 必须走它——`secure` 分区的 HFS0 头真机实测落在
///   文件的 368 MB 处，前缀读一辈子也够不着，而 seek 过去只要几 KB。
pub trait Source {
    /// 读 `at` 起的至多 `len` 字节。够不着、读不动、或者一个字节都没有时是 `None`；
    /// 读到但不足 `len` 时如实交出短的那一段，长度由调用方检查。
    fn at(&mut self, at: u64, len: usize) -> Option<Vec<u8>>;
}

/// 手上只有前面那一段字节。
pub struct Prefix<'a>(
    /// 已经读到手的那一段。它之后的位置一律答 `None`。
    pub &'a [u8],
);

impl Source for Prefix<'_> {
    fn at(&mut self, at: u64, len: usize) -> Option<Vec<u8>> {
        let start = usize::try_from(at).ok()?;
        let slice = self.0.get(start..)?;
        let end = len.min(slice.len());
        if end == 0 {
            return None;
        }
        Some(slice[..end].to_vec())
    }
}

/// 一个能 seek 的只读句柄，带**读字节数的上限**。
///
/// 上限不是保险丝而是这一层的成本闸：整个 Switch 目录 223.84 GiB，而这一层的全部意义
/// 是**每份只读几 KB**。畸形头声称自己有几百万个条目时，闸门在读之前就把它挡下来。
pub struct Seeked<'a> {
    inner: &'a mut dyn ReadSeek,
    budget: u64,
    read: u64,
}

impl<'a> Seeked<'a> {
    /// 包一个只读句柄，最多让它读 `budget` 字节。
    pub fn new(inner: &'a mut dyn ReadSeek, budget: u64) -> Self {
        Self {
            inner,
            budget,
            read: 0,
        }
    }

    /// 这一趟真读了多少字节。**它进总账**——「第二趟读了多少」是增量兑现与否的凭据。
    #[must_use]
    pub fn read(&self) -> u64 {
        self.read
    }
}

impl Source for Seeked<'_> {
    fn at(&mut self, at: u64, len: usize) -> Option<Vec<u8>> {
        let left = self.budget.saturating_sub(self.read);
        let want = u64::try_from(len).ok()?.min(left);
        let want = usize::try_from(want).ok()?;
        if want == 0 {
            return None;
        }
        self.inner.seek(SeekFrom::Start(at)).ok()?;
        let mut buffer = vec![0; want];
        let mut filled = 0;
        while filled < want {
            match self.inner.read(&mut buffer[filled..]) {
                Ok(0) => break,
                Ok(got) => filled += got,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => return None,
            }
        }
        buffer.truncate(filled);
        self.read += u64::try_from(filled).unwrap_or(0);
        (filled > 0).then_some(buffer)
    }
}

/// 分区表里的一条。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    /// 明文文件名。
    name: String,
    /// 这一条在文件里的**绝对**偏移。
    at: u64,
    /// 这一条多大。
    size: u64,
}

/// 探一份 Switch 容器。
///
/// `name` 是文件名（只用来在认不出时把话说清楚），`source` 是取字节的办法。
///
/// **判据是字节不是扩展名**：`PFS0` 与 `HEAD` 两个魔数说了算。一份改错了扩展名的
/// `.nsp` 照样认得出，而一份躺在 `switch/` 目录下的别的东西照样认不出——目录只是
/// 强先验（ADR-0011）。
#[must_use]
pub fn probe(name: &str, source: &mut dyn Source) -> Facts {
    let Some(head) = source.at(0, HEAD_LEN) else {
        return note(format!("{}：一个字节都没读到", file_name_of_key(name)));
    };
    if head.starts_with(b"PFS0") {
        return match rows(source, 0, 0x18, b"PFS0") {
            Ok(found) => from_rows(Wrapper::Pfs0, Vec::new(), &found),
            Err(why) => note(why),
        };
    }
    match card_header_base(&head) {
        Some(base) => xci(source, &head, base),
        None => note(format!(
            "{}：头 {} 字节里既没有 PFS0，也没有 0x100 / 0x1100 处的 HEAD——\
             这不是 Switch 的容器",
            file_name_of_key(name),
            head.len()
        )),
    }
}

/// 只带一句话的事实。
fn note(why: String) -> Facts {
    Facts {
        note: Some(why),
        ..Facts::default()
    }
}

/// CardHeader 在文件的哪个偏移上。
///
/// **两处都探，不硬编码**：scene 风格的转储从 CardHeader 开始（`"HEAD"` 在 `0x100`），
/// FullXCI 前面补了 `0x1000` 字节的 CardKeyArea（`"HEAD"` 在 `0x1100`）。
/// 四个独立来源交叉验证过这个位移（调研 §2.1.1）。
#[must_use]
pub fn card_header_base(head: &[u8]) -> Option<u64> {
    [0_u64, 0x1000]
        .into_iter()
        .find(|base| match usize::try_from(*base) {
            Ok(at) => head.get(at + 0x100..at + 0x104) == Some(b"HEAD"),
            Err(_) => false,
        })
}

/// 走完一张卡带转储：卡带头 → 根 HFS0 → `secure` 分区的 HFS0。
fn xci(source: &mut dyn Source, head: &[u8], base: u64) -> Facts {
    let at = match usize::try_from(base) {
        Ok(at) => at,
        Err(_) => return note("XCI 的 CardHeader 偏移越界".to_string()),
    };
    let Some(field) = head.get(at + 0x130..at + 0x138) else {
        return note("XCI 的头读得不够长，取不到 PartitionFsHeaderAddress".to_string());
    };
    let address = u64::from_le_bytes(field.try_into().unwrap_or([0; 8]));
    let root = match rows(source, base.saturating_add(address), 0x40, b"HFS0") {
        Ok(root) => root,
        Err(why) => return note(format!("XCI 的根 HFS0 读不出来：{why}")),
    };
    let partitions: Vec<String> = root.iter().map(|row| row.name.clone()).collect();
    // **只走 `secure`**。`update` 分区里躺着整套系统更新的 NCA，它们的 ContentId 查出来
    // 是系统 title（`0100000000000816`）而不是游戏——把它们收进来，每一张卡都会多出
    // 一条指向系统更新的候选。`normal` / `logo` 只有图标。
    let Some(secure) = root.iter().find(|row| row.name == "secure") else {
        return card_only(
            partitions,
            "XCI 的根 HFS0 里没有 secure 分区，认不出内容".to_string(),
        );
    };
    match rows(source, secure.at, 0x40, b"HFS0") {
        Ok(found) => from_rows(Wrapper::Xci, partitions, &found),
        Err(why) => card_only(partitions, format!("XCI 的 secure 分区读不出来：{why}")),
    }
}

/// 卡带头读到了、`secure` 分区没读到时的那一份事实。
///
/// **分区名照样交出去**：那是结构指纹的判据（scene 还是转换来的），而它已经读到手了
/// ——一句「读不出来」把它一起扔掉，等于把已经花掉的那几 KB 白花。
fn card_only(partitions: Vec<String>, why: String) -> Facts {
    Facts {
        wrapper: Some(Wrapper::Xci.code().to_string()),
        structure: Some(
            structure_of(Wrapper::Xci, &partitions, &[])
                .code()
                .to_string(),
        ),
        partitions,
        note: Some(why),
        ..Facts::default()
    }
}

/// 读一张 PFS0 或 HFS0 的头、条目表与字符串表。
///
/// **PFS0 与 HFS0 只差两处**：魔数，和一条条目多大（`0x18` 对 `0x40`，HFS0 多了被哈希
/// 区域的大小与一段 SHA-256）。前 `0x18` 字节的布局一模一样，所以这一份代码两边共用
/// （switch-library-manager 的 `readPfs0()` 也是这么写的）。
///
/// ⚠ **计数字段是 u32 不是 u16**。switch-library-manager 那一处用 `Uint16` 读
/// `FileCount` 又拿 uint16 去算表长，HFS0 一条 `0x40` 字节时超过 1023 条就溢出。
/// 调研的实现陷阱第 3 条专门点了这一处，别照抄。
fn rows(
    source: &mut dyn Source,
    base: u64,
    entry_size: usize,
    magic: &[u8; 4],
) -> Result<Vec<Row>, String> {
    let head = source
        .at(base, 0x10)
        .ok_or_else(|| format!("偏移 {base} 处读不到分区头"))?;
    if head.len() < 0x10 {
        return Err(format!("偏移 {base} 处的分区头只有 {} 字节", head.len()));
    }
    if &head[..4] != magic {
        return Err(format!(
            "偏移 {base} 处的魔数是 {:02x?} 而不是 {}",
            &head[..4],
            String::from_utf8_lossy(magic)
        ));
    }
    let count = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    let strings = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
    if count > MAX_ENTRIES || strings > MAX_STRING_TABLE {
        return Err(format!(
            "分区头声称有 {count} 条、字符串表 {strings} 字节，超过这一层认的上限"
        ));
    }
    let table = usize::try_from(count)
        .ok()
        .and_then(|count| count.checked_mul(entry_size))
        .ok_or_else(|| "条目表长度算不下来".to_string())?;
    let strings = usize::try_from(strings).map_err(|_| "字符串表长度算不下来".to_string())?;
    let want = table + strings;
    let body = source
        .at(base + 0x10, want)
        .ok_or_else(|| format!("偏移 {} 处读不到条目表", base + 0x10))?;
    if body.len() < want {
        return Err(format!(
            "条目表要 {want} 字节，只读到 {}——这一份只有前缀，够不着它",
            body.len()
        ));
    }
    let data = base + 0x10 + u64::try_from(want).unwrap_or(0);
    let mut out = Vec::new();
    for index in 0..usize::try_from(count).unwrap_or(0) {
        let entry = &body[index * entry_size..(index + 1) * entry_size];
        let at = u64::from_le_bytes(entry[0..8].try_into().unwrap_or([0; 8]));
        let size = u64::from_le_bytes(entry[8..16].try_into().unwrap_or([0; 8]));
        let name_at = u32::from_le_bytes(entry[16..20].try_into().unwrap_or([0; 4]));
        let name = name_at
            .try_into()
            .ok()
            .and_then(|at: usize| body.get(table + at..))
            .map(|rest| {
                let end = rest
                    .iter()
                    .position(|byte| *byte == 0)
                    .unwrap_or(rest.len());
                String::from_utf8_lossy(&rest[..end]).into_owned()
            })
            .unwrap_or_default();
        out.push(Row {
            name,
            at: data.saturating_add(at),
            size,
        });
    }
    Ok(out)
}

/// 把一张文件名表折成事实。
fn from_rows(wrapper: Wrapper, partitions: Vec<String>, rows: &[Row]) -> Facts {
    let entries: Vec<String> = rows.iter().map(|row| row.name.clone()).collect();
    let mut content_ids: Vec<String> = entries.iter().filter_map(|name| content_id(name)).collect();
    content_ids.sort_unstable();
    content_ids.dedup();
    let ids: Vec<Ident> = entries
        .iter()
        .filter_map(|name| ticket_id(name, wrapper))
        .collect();
    let compressed = entries
        .iter()
        .any(|name| name.to_ascii_lowercase().ends_with(".ncz"));
    let kind = ids
        .first()
        .and_then(|id| Kind::of(&id.key))
        .map(|kind| kind.code().to_string());
    Facts {
        wrapper: Some(wrapper.code().to_string()),
        structure: Some(
            structure_of(wrapper, &partitions, &entries)
                .code()
                .to_string(),
        ),
        partitions,
        entries,
        content_ids,
        ids,
        compressed,
        kind,
        note: None,
    }
}

/// 这个文件名是不是一份 NCA，是的话交出它的 **ContentId**。
///
/// 四种写法一条判据：`<32 位 hex>` 加 `.nca` / `.cnmt.nca` / `.ncz` / `.cnmt.ncz`。
/// **压缩过的只改最后一个字符**，所以 hex 词干原样保留（nsz 的
/// `newFileName = path[0:-1]+"z"`）。
#[must_use]
pub fn content_id(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();
    let stem = ["cnmt.nca", "cnmt.ncz", "nca", "ncz"]
        .iter()
        .find_map(|ext| lower.strip_suffix(&format!(".{ext}")))?;
    is_hex(stem, ID_HEX).then(|| stem.to_string())
}

/// 这个文件名是不是一份票据，是的话交出它说的 **TitleID**。
///
/// RightsId 的布局是 `TitleID(8 字节大端) ‖ 7 字节 0x00 ‖ KeyGeneration(1 字节)`
/// ——**调研对 71,182 条样本实测：中间那 14 位 hex 100% 恒为零，零反例**。三个一手实现
/// （nsz 的 `ExtractTitlekeys.py`、nxdumptool 的 `tik.c`、switch-library-manager）
/// 都按这个布局解析。所以前 16 位 hex 就是 TitleID，末 2 位是 KeyGeneration。
#[must_use]
pub fn rights_id(name: &str) -> Option<(String, String)> {
    let lower = name.to_ascii_lowercase();
    let stem = lower.strip_suffix(".tik")?;
    if !is_hex(stem, ID_HEX) {
        return None;
    }
    // 中间那 14 位不是零就不是一条 RightsId——如实拒了，别把一串别的东西切成 TitleID。
    if stem.get(TITLE_HEX..ID_HEX - 2)? != "00000000000000" {
        return None;
    }
    Some((
        stem.get(..TITLE_HEX)?.to_ascii_uppercase(),
        stem.get(ID_HEX - 2..)?.to_ascii_uppercase(),
    ))
}

/// 把一条 `.tik` 折成**标识**。
fn ticket_id(name: &str, wrapper: Wrapper) -> Option<Ident> {
    let (title_id, generation) = rights_id(name)?;
    Some(Ident {
        key: title_id.clone(),
        shown: title_id,
        kind: IdKind::TitleId,
        from: format!(
            "{} 的明文文件名表里 {name} 的前 16 位 hex（KeyGeneration {generation}）",
            wrapper.label()
        ),
        title: None,
        version: None,
    })
}

/// 这一串是不是恰好 `len` 个 hex 字符。
fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len && text.chars().all(|c| c.is_ascii_hexdigit())
}

/// 结构指纹：判据全是文件名与分区名，一个密钥都不要。
#[must_use]
pub fn structure_of(wrapper: Wrapper, partitions: &[String], entries: &[String]) -> Structure {
    let has = |suffix: &str| {
        entries
            .iter()
            .any(|name| name.to_ascii_lowercase().ends_with(suffix))
    };
    if wrapper == Wrapper::Xci {
        let named = |what: &str| partitions.iter().any(|name| name == what);
        // NX Game Info 的判据：三个分区都在是 scene，只有 secure 是从 NSP 转来的。
        return if named("update") && named("normal") && named("secure") {
            Structure::SceneXci
        } else {
            Structure::ConvertedXci
        };
    }
    if has("authoringtoolinfo.xml") {
        return Structure::Homebrew;
    }
    if !has(".cnmt.nca") && !has(".cnmt.ncz") {
        return Structure::Incomplete;
    }
    if has("legalinfo.xml") && has("nacp.xml") && has("programinfo.xml") && has("cardspec.xml") {
        return Structure::SceneNsp;
    }
    if has(".tik") {
        Structure::CdnNsp
    } else {
        Structure::ConvertedNsp
    }
}

/// 候选表里，靠**容器自己**认出来的那条候选的「数据源」叫什么。
///
/// 它与 `No-Intro`、`TOSEC`、`中文离线源` 平级地出现在报告的数据源那一栏里——
/// **免密钥那一层贡献了多少覆盖率，要一眼看得出来**，与查表那一层分开数
/// （同 `fuzzy::SOURCE`）。
pub const SOURCE_CONTAINER: &str = "Switch 容器";

/// 候选表里，靠 titledb 反查出来的那条候选的「数据源」叫什么。
pub const SOURCE_TITLEDB: &str = "titledb";

/// 候选表里，**只剩文件名**那条候选的「数据源」叫什么。
///
/// 它与上面两个分开，判据是**这一条读没读过字节**。报告有一列「只靠名字」，
/// 存在的理由是不让「命中」两个字被只看名字的层撑起来（票 11 的中文离线源
/// 就在那一列里）；这一层同样一个字节都不读，落进同一列——真机上它给了
/// 63 条候选，混进容器那一层里就看不出来了。
pub const SOURCE_NAME: &str = "Switch 文件名";

/// 这一层的候选写在 `dat` 那一列里的东西。别的层写「哪一份 DAT」，
/// 而这一层**没有 DAT**（No-Intro 的每日镜像里 334 份一个 Switch 都没有），
/// 写的是判据的出处。
const FROM_NAMES: &str = "明文文件名表";

/// 同上，查表那一层的出处。
const FROM_TITLEDB: &str = "blawar/titledb cnmts.json";

/// 折候选这一路上处处都要的三样：查表的索引、平台、名字里那个 TitleID。
///
/// 捏成一个类型而不是散着传，是因为它们**同进同出**：每一档候选都要问过它们才敢说话，
/// 而三样散成三个参数，加第四样就要改遍每一处签名。
struct Ctx<'a> {
    store: Option<&'a Store>,
    platform: &'a str,
    /// 文件名里 `[…]` 括着的那几个 TitleID；一个都没有就是空的。
    named: Vec<String>,
}

/// 要拿去撞的一份内容：哪个成员、容器内部哪一条、探出来的事实。
#[derive(Debug, Clone, Copy)]
pub struct Probe<'a> {
    /// 是变体的哪个成员。
    pub member: &'a str,
    /// **透明容器**内部路径；裸文件是空串。
    pub inner: &'a str,
    /// 这一份探出来的东西。
    pub facts: &'a Facts,
}

/// 这一层这一趟干了什么。
///
/// 几个数永远同进同出（一层的全部账目），所以是一个类型而不是几个字段——
/// 同 `identify::CartCount` 的道理：加一个计数器要改三处，而漏掉一处不会有任何人告诉你。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Found {
    /// 产出的候选。
    #[serde(skip)]
    pub candidates: Vec<Candidate>,
    /// 探了几份内容（读出容器头的）。
    pub probed: u64,
    /// 其中 `.tik` 说出了 TitleID 的。
    pub ticketed: u64,
    /// 其中靠 ContentId 反查出**精确版本**的。
    pub resolved: u64,
    /// 其中是压缩过的（`.nsz` / `.xcz`）——**ES-DE 看不见这一批**（ADR-0017）。
    pub compressed: u64,
    /// 容器里装着不止一个 TitleID 的（合集卡带）。这一档不自动通过。
    pub multi: u64,
    /// 文件名里那个 TitleID 与容器里读出来的**对不上**的。极强的「被改过」信号。
    pub name_conflicts: u64,
}

impl Found {
    /// 把一趟的账并进总账。
    ///
    /// 它存在的理由与这个类型本身一样：**这几个数永远同进同出**。调用处手抄六行
    /// `a += b`，加第七个计数器时必有一处漏掉，而漏掉的那一处不会有任何人告诉你。
    /// 候选不并——那一批由调用处自己接手。
    pub fn merge(&mut self, other: &Self) {
        self.probed += other.probed;
        self.ticketed += other.ticketed;
        self.resolved += other.resolved;
        self.compressed += other.compressed;
        self.multi += other.multi;
        self.name_conflicts += other.name_conflicts;
    }
}

/// 文件名里 `[…]` 括着的**每一个** TitleID。
///
/// Switch 场景的事实标准命名是 `Game Name [0100XXXXXXXXX000][v65536].nsp`，判据取自
/// switch-library-manager 的 `titleIdRegex`（`\[(?P<titleId>[A-Za-z0-9]{16})]`），
/// 这里再收紧一道：**必须是 16 位 hex**。
///
/// 它**永远不是这一层的主判据**——文件名可以被任意改写，而真库里 33.6% 的文件名含汉字、
/// 多半已经丢掉了这一段。它的用处是与容器里读出来的那个**互相印证**：对得上，
/// 人在裁决队列里一眼就能拍板；对不上，那是极强的「文件被改过 / 命名错误」信号。
///
/// **交出全部而不是第一个。** 真机上有一份合集卡带的名字里写着两个 TitleID
/// （`Double Pack [0100D49020B84000][0100E7C020B82000][v0].xci`），而卡里确实装着
/// 两个游戏——只取第一个，第二个游戏那条候选就会被报成「被改过」。
#[must_use]
pub fn title_ids_in_name(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut out: Vec<String> = Vec::new();
    for (at, ch) in chars.iter().enumerate() {
        if *ch != '[' {
            continue;
        }
        let Some(body) = chars.get(at + 1..at + 1 + TITLE_HEX) else {
            continue;
        };
        if chars.get(at + 1 + TITLE_HEX) != Some(&']') {
            continue;
        }
        if body.iter().all(char::is_ascii_hexdigit) {
            let one = body.iter().collect::<String>().to_ascii_uppercase();
            if !out.contains(&one) {
                out.push(one);
            }
        }
    }
    out
}

/// ⭐ **把探出来的事实折成候选。**
///
/// 三档，按 ADR-0002 的三个置信度落位：
///
/// | 档 | 判据 | 置信度 | 自动通过 |
/// |---|---|---|---|
/// | **反查** | 容器里的 ContentId 在 titledb 里查得出唯一一个 (TitleID, 版本) | 高 | **是** |
/// | **票据** | `.tik` 的文件名说出 TitleID，但版本无从谈起 | 中 | 否 |
/// | **名字** | 只有文件名里那个 `[TitleID]`，容器什么都没说 | 低 | 否 |
///
/// **反查那一档为什么敢自动通过**：ContentId 是整个 NCA 的 SHA-256 前 16 字节，
/// 而调研实测 173,502 个 ContentId **100.00% 唯一映射**到单个 (titleId, version)，
/// 零冲突。撞上它等同于一次精确哈希命中（ADR-0002 的高置信那一档）。
///
/// **票据那一档为什么只到中置信**：`.tik` 只说得出 TitleID 与 KeyGeneration，
/// 说不出版本；更要紧的是**魔改整合版通常保留原票据**——Program NCA 被重建、
/// ContentId 全变，而 `.tik` 原样留着。于是「票据说得出、反查说不出」这个组合
/// 本身就是一个可靠的**疑似魔改**信号（调研 §5.3），它该做的事正是进待确认队列。
///
/// # Errors
/// 读 titledb 索引失败时返回错误。
pub fn candidates(
    store: Option<&Store>,
    platform: Option<&str>,
    probes: &[Probe<'_>],
    named: Option<&str>,
) -> Result<Found, StoreError> {
    let mut found = Found::default();
    let ctx = Ctx {
        store,
        platform: platform.unwrap_or("SWITCH"),
        named: named.map(title_ids_in_name).unwrap_or_default(),
    };
    for probe in probes {
        let facts = probe.facts;
        if facts.wrapper.is_none() {
            continue;
        }
        found.probed += 1;
        if facts.compressed {
            found.compressed += 1;
        }
        if !facts.ids.is_empty() {
            found.ticketed += 1;
        }
        // 一、反查。容器里每一个 ContentId 都查一次——**任意一个都该指向同一个
        // (TitleID, 版本)**，指不到同一处才是要报出来的事。
        let mut hits: BTreeMap<Content, u64> = BTreeMap::new();
        let mut looked = 0_u64;
        if let Some(store) = store {
            for content_id in &facts.content_ids {
                looked += 1;
                if let Some(content) = store.content(content_id)? {
                    *hits.entry(content).or_insert(0) += 1;
                }
            }
        }
        if hits.len() > 1 {
            found.multi += 1;
        }
        if !hits.is_empty() {
            found.resolved += 1;
        }
        let hit: u64 = hits.values().sum();
        for (content, count) in &hits {
            found.candidates.push(resolved_candidate(
                &ctx,
                probe,
                content,
                *count,
                looked,
                Verdict::of(hits.len(), hit, looked),
                &mut found.name_conflicts,
            )?);
        }
        // 二、票据。**反查已经说过话的那个 TitleID 不再重复出一条**——那只会让人在
        // 队列里读同一件事两遍；票据与反查互相印证这件事写进反查那一条的依据里。
        for id in &facts.ids {
            if hits.keys().any(|content| content.title_id == id.key) {
                continue;
            }
            found.candidates.push(ticket_candidate(
                &ctx,
                probe,
                id,
                &mut found.name_conflicts,
            )?);
        }
    }
    // 三、名字。**只有容器一个字都没说出来时才出这一条**——容器说得出的时候，
    // 名字的用处是印证不是另立一条候选（上面两档已经把印证写进依据了）。
    if found.candidates.is_empty()
        && let Some(probe) = probes.first()
    {
        for title_id in ctx.named.clone() {
            found
                .candidates
                .push(name_candidate(&ctx, probe, &title_id)?);
        }
    }
    Ok(found)
}

/// 反查敢不敢自动通过，不敢的话为什么。
///
/// ⭐ **两道闸，第二道是真机数据逼出来的。**
///
/// 1. **这个容器只指向一个 (TitleID, 版本)**。指向好几个的是合集卡带那一类，
///    哪一个代表这个变体说不清。
/// 2. **查不到的 ContentId 至多一个**。真机 67 条反查的分布是这样的：
///    **60 条恰好漏一个**——那一个是 Meta NCA 本身，它按构造就不在自己的
///    `contentEntries` 里；而漏掉 5 / 6 / 13 / 24 个的那 7 条，逐条看过全是
///    「一个容器里装着不止一档内容」：`(1G+1U+9D)(MOD14)` 的整合卡、双游戏合集、
///    本体加更新打包的三部曲。**那正是模块文档说的「疑似魔改」形状**，该进裁决队列
///    （ADR-0002），不该自动通过。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verdict {
    /// 敢：只指向一个 (TitleID, 版本)，而且没查到的至多一个。
    Trust,
    /// 不敢：这个容器指向好几个 (TitleID, 版本)。
    Several,
    /// 不敢：容器里有一批 ContentId 在 titledb 里查不到。
    Extra(u64),
}

impl Verdict {
    fn of(titles: usize, hit: u64, looked: u64) -> Self {
        if titles > 1 {
            return Self::Several;
        }
        // Meta NCA 不在自己的清单里，所以漏一个是常态；漏两个起就是另有内容。
        match looked.saturating_sub(hit) {
            0 | 1 => Self::Trust,
            extra => Self::Extra(extra),
        }
    }

    fn trusted(self) -> bool {
        self == Self::Trust
    }

    /// 不敢自动通过时，依据里那一句。
    fn why(self) -> Option<String> {
        match self {
            Self::Trust => None,
            Self::Several => Some(
                "；**这个容器里装着不止一个 TitleID**（合集卡带那一类），\
                 哪一个代表这个变体说不清，所以不自动通过"
                    .to_string(),
            ),
            Self::Extra(extra) => Some(format!(
                "；⚠ 容器里另有 {extra} 个 ContentId 在 titledb 里**查不到**\
                 （Meta NCA 不在自己的清单里，所以漏一个是常态，漏这么多不是）——\
                 这一份多半装着不止一档内容（本体加更新加 DLC 的整合包），\
                 或者被重打包过，所以不自动通过"
            )),
        }
    }
}

/// 反查出来的那一条。
fn resolved_candidate(
    ctx: &Ctx<'_>,
    probe: &Probe<'_>,
    content: &Content,
    count: u64,
    looked: u64,
    verdict: Verdict,
    conflicts: &mut u64,
) -> Result<Candidate, StoreError> {
    let facts = probe.facts;
    let wrapper = wrapper_of(facts).map_or("容器", Wrapper::label);
    let mut evidence = format!(
        "{wrapper} 的{FROM_NAMES}里 {looked} 个 ContentId，其中 {count} 个在 titledb 的 \
         cnmts.json 里查得到，全部指向 TitleID {} 版本 {}（调研实测 173,502 个 ContentId \
         100.00% 唯一映射到单个 (TitleID, 版本)，零冲突）。**免密钥**：一个字节的 NCA \
         都没解开",
        content.title_id, content.version
    );
    describe_shape(&mut evidence, facts, &content.title_id);
    cross_check(&mut evidence, &content.title_id, &ctx.named, conflicts);
    if let Some(why) = verdict.why() {
        evidence.push_str(&why);
    }
    let (title, chinese) = named(ctx.store, &content.title_id, Some(content.version))?;
    Ok(Candidate {
        member_key: probe.member.to_string(),
        inner: probe.inner.to_string(),
        confidence: if verdict.trusted() {
            Confidence::High
        } else {
            Confidence::Medium
        },
        accepted: verdict.trusted(),
        source: SOURCE_TITLEDB.to_string(),
        dat: FROM_TITLEDB.to_string(),
        platform: ctx.platform.to_string(),
        game: title,
        // 这一列在别的层里是「DAT 里那条 `<rom>` 记录的名字」。这一层没有文件记录，
        // 写**撞上的那个 ContentId**——编一个文件名顶上去，事后复核的人会以为真有
        // 那么一条记录（与序列号那一层在这一列写编号是同一个道理）。
        rom: format!("{count}/{looked} 个 ContentId"),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence,
        chinese,
        serial: Some(content.title_id.clone()),
        release_id: None,
    })
}

/// 票据说出来的那一条。
fn ticket_candidate(
    ctx: &Ctx<'_>,
    probe: &Probe<'_>,
    id: &Ident,
    conflicts: &mut u64,
) -> Result<Candidate, StoreError> {
    let facts = probe.facts;
    let mut evidence = format!("{}。**免密钥**：容器层不加密，读它一个密钥都不要", id.from);
    describe_shape(&mut evidence, facts, &id.key);
    cross_check(&mut evidence, &id.key, &ctx.named, conflicts);
    // **「没查过」与「查了没有」是两件事**，混成一句下一个人只会重下一遍数据源
    // （同 `CONTEXT.md` 里「穿不透」与「还没读过」那一条）。
    evidence.push_str(if ctx.store.is_none() {
        "；**本机还没取过 titledb**，所以说不出这是哪个版本——\
         `romcat switch sync` 取一次就能把这一条抬到高置信"
    } else if facts.content_ids.is_empty() {
        "；容器里一个 ContentId 都没有，反查无从谈起"
    } else {
        "；容器里那几个 ContentId 在 titledb 里**一个都查不到**。\
         XCI 本来就该是这样（titledb 抓的是 eShop 的 CDN，卡带上的 NCA \
         DistributionType 不同、ContentId 也就不同，实测 isGameCard 只有 121 条）；\
         而一份 NSP 落到这一档，高度提示它是**重打包的魔改整合版**——\
         Program NCA 被重建、ContentId 全变，而票据原样留着"
    });
    let (title, chinese) = named(ctx.store, &id.key, None)?;
    Ok(Candidate {
        member_key: probe.member.to_string(),
        inner: probe.inner.to_string(),
        // 中置信是这一档的天花板：它说得出「这是哪个游戏」，说不出「这是哪个版本」。
        confidence: Confidence::Medium,
        accepted: false,
        source: SOURCE_CONTAINER.to_string(),
        dat: format!(
            "{} {FROM_NAMES}",
            wrapper_of(facts).map_or("", Wrapper::label)
        ),
        platform: ctx.platform.to_string(),
        game: title,
        rom: id.shown.clone(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence,
        chinese,
        serial: Some(id.key.clone()),
        release_id: None,
    })
}

/// 只剩文件名的那一条。
fn name_candidate(
    ctx: &Ctx<'_>,
    probe: &Probe<'_>,
    title_id: &str,
) -> Result<Candidate, StoreError> {
    let mut evidence =
        format!("文件名里直接写着的 TitleID `[{title_id}]`。**容器一个字都没说出来**");
    if let Some(note) = &probe.facts.note {
        evidence.push_str(&format!("（{note}）"));
    }
    evidence
        .push_str("，所以这一条**连内容都没看**——文件名可以被任意改写，永不自动通过（ADR-0011）");
    let (title, chinese) = named(ctx.store, title_id, None)?;
    Ok(Candidate {
        member_key: probe.member.to_string(),
        inner: probe.inner.to_string(),
        confidence: Confidence::Low,
        accepted: false,
        source: SOURCE_NAME.to_string(),
        dat: "文件名".to_string(),
        platform: ctx.platform.to_string(),
        game: title,
        rom: title_id.to_string(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence,
        chinese,
        serial: Some(title_id.to_string()),
        release_id: None,
    })
}

/// 结构指纹与「本体 / 补丁 / 附属内容」写进依据。
///
/// **两样都必须说出来**：结构指纹说的是「这份转储是怎么来的」（裁决的人靠它判断一份
/// 包完不完整、是不是被人转换过），而那三种内容不分开，库体检就会把一堆更新包报成游戏。
fn describe_shape(evidence: &mut String, facts: &Facts, title_id: &str) {
    if let Some(structure) = facts.structure.as_deref().and_then(Structure::from_code) {
        evidence.push_str(&format!("；结构是{}", structure.label()));
    }
    // ⚠ **这一串取自这一条候选自己**，不是 `facts.title_id()`（容器里的第一张票据）。
    // 真机上撞见过：一份合集卡带里装着两个游戏、两张票据，两条候选的「本体是」
    // 会同时印成第一张票据的那一串——两条候选里必有一条在撒谎。
    if let Some(kind) = Kind::of(title_id) {
        evidence.push_str(&format!("，这一份是**{}**", kind.label()));
        if kind != Kind::Base
            && let Some(base) = Kind::base_of(title_id)
        {
            evidence.push_str(&format!("（本体是 {base}）"));
        }
    }
    if facts.compressed {
        evidence.push_str(
            "；里面是 `.ncz`，**这一份是压缩过的**——识别不受影响（零解压），\
             但 ES-DE 的扩展名表里没有 `.nsz` / `.xcz`，它在那个前端里默认看不见\
             （ADR-0017）",
        );
    }
}

/// 文件名里那个 TitleID 与容器读出来的对不对得上，写进依据。
fn cross_check(evidence: &mut String, found: &str, named: &[String], conflicts: &mut u64) {
    if named.is_empty() {
        return;
    }
    if named.iter().any(|it| it.eq_ignore_ascii_case(found)) {
        evidence.push_str("；文件名里写的 TitleID 与容器里读出来的**一致**");
        return;
    }
    // **文件名里可能写着好几个**：真机上那份合集卡带的名字里就有两个 TitleID，
    // 而卡里确实装着两个游戏。只比第一个，第二个游戏那条候选会被报成「被改过」。
    let named = named
        .iter()
        .find(|it| Kind::base_of(it) == Kind::base_of(found))
        .unwrap_or(&named[0]);
    // ⭐ **先按本体那一串比，两串不等不等于对不上。** 真机上五条「对不上」全是同一种
    // 形状：文件名写的是**本体**的 TitleID（`…4000`），而卡带里那张票据是它的**补丁**
    // （`…4800`）——那正是一张「本体加更新」的卡该有的样子，不是被改过。按原样比会把
    // 五条正常的卡全报成警告，而真出事时那句警告就没人信了。
    if Kind::base_of(named) == Kind::base_of(found) {
        let kind = Kind::of(found).map_or("另一档", Kind::label);
        evidence.push_str(&format!(
            "；文件名里写的是 `[{named}]`，容器里读出来的是同一部作品的{kind} {found}\
             ——**同一个本体号段，对得上**"
        ));
        return;
    }
    *conflicts += 1;
    evidence.push_str(&format!(
        "；⚠ 文件名里写的是 `[{named}]`，容器里读出来的却是 {found}，\
         **连本体那一串都对不上**——这是极强的「文件被改过 / 命名错误」信号"
    ));
}

/// 这一份探出来的是哪一种容器。**一处判据**——散成两处 `match`，改一个枚举成员
/// 就会有一处忘了改。
fn wrapper_of(facts: &Facts) -> Option<Wrapper> {
    facts.wrapper.as_deref().and_then(Wrapper::from_code)
}

/// 一个 TitleID 该叫什么。
///
/// **查得到就用 eShop 的名字，查不到就如实写 TitleID 本身**——编一个名字顶上去，
/// 下游的**收敛**会把它当成一部真作品。
///
/// 名字折成 No-Intro 那种写法（`正题 (地区) (语言)`），于是下游那一整条路
/// （`naming::parse` 读作品与语言、`chinese::mark_of` 认官中、`title` 判世代裂缝）
/// 一处都不必为 Switch 分叉（[`Title::entry_name`](crate::titledb::Title::entry_name)）。
///
/// **补丁与附属内容取本体的名字**：三者是同一部作品下的东西（`Kind::base_of`），
/// 而尾巴上那一段说清它是哪一种——不这么做，一份 DLC 会在库里长成一部叫
/// `0100EAE01990512C` 的独立作品。
fn named(
    store: Option<&Store>,
    title_id: &str,
    version: Option<u32>,
) -> Result<(String, Option<ChineseMark>), StoreError> {
    let kind = Kind::of(title_id);
    let anchor = match kind {
        Some(Kind::Base) | None => title_id.to_string(),
        Some(_) => Kind::base_of(title_id).unwrap_or_else(|| title_id.to_string()),
    };
    // **一次查表办两件事**：名字与中文记号问的是同一行。分成两个函数不只是多一次
    // 查询，更是两处各算一遍 anchor——那个算法哪天改了，两处必有一处忘了跟。
    let found = match store {
        Some(store) => store.title(&anchor)?,
        None => None,
    };
    let mut out = found
        .as_ref()
        .map_or_else(|| format!("TitleID {anchor}"), Title::entry_name);
    match kind {
        Some(Kind::Patch) => out.push_str(&match version {
            Some(version) => format!(" (Update v{version})"),
            None => " (Update)".to_string(),
        }),
        Some(Kind::AddOn) => out.push_str(&format!(" (DLC {title_id})")),
        _ => {}
    }
    // **官中**的判据是 titledb 的 `languages` 里有没有 `Zh`，与 DAT 那一侧
    // `(Ja,Zh)` 语言标记组认出来的是同一件事。**Switch 上没有汉化版这一档**：
    // 那边的汉化生态是 LayeredFS 补丁目录，原包一个字节都没改（调研 §5.3），
    // 所以这一层永不产出 `FanTranslated`。
    let chinese = found
        .and_then(|title| title.languages)
        .is_some_and(|languages| {
            languages
                .split(',')
                .any(|code| code.eq_ignore_ascii_case("zh"))
        })
        .then_some(ChineseMark::Official);
    Ok((out, chinese))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::testing::switch::{pfs0, xci};

    #[test]
    fn 明文文件名表读得出_不必解密() {
        let bytes = pfs0(&[
            "0100a0c01bed88000000000000000010.tik",
            "0100a0c01bed88000000000000000010.cert",
            "150cf9022bfb2e72527669f2701ee31b.cnmt.nca",
            "dfdb0f5bc5c5056a2f35d0379ee23020.nca",
        ]);
        let facts = probe("某作.nsp", &mut Prefix(&bytes));
        assert_eq!(facts.wrapper.as_deref(), Some("pfs0"));
        assert_eq!(facts.entries.len(), 4);
        assert_eq!(
            facts.content_ids,
            vec![
                "150cf9022bfb2e72527669f2701ee31b".to_string(),
                "dfdb0f5bc5c5056a2f35d0379ee23020".to_string(),
            ]
        );
        assert_eq!(facts.title_id(), Some("0100A0C01BED8800"));
        assert_eq!(facts.kind.as_deref(), Some("patch"), "尾 800 是补丁");
        assert_eq!(facts.structure.as_deref(), Some("cdn-nsp"));
    }

    #[test]
    fn 压缩过的照样认_一个字节都不解压() {
        // nsz 只改最后一个字符，32 位 hex 的词干原样保留。
        let bytes = pfs0(&[
            "150cf9022bfb2e72527669f2701ee31b.cnmt.ncz",
            "dfdb0f5bc5c5056a2f35d0379ee23020.ncz",
        ]);
        let facts = probe("某作.nsz", &mut Prefix(&bytes));
        assert!(facts.compressed, "里面是 .ncz");
        assert_eq!(facts.content_ids.len(), 2);
        assert_eq!(
            facts.structure.as_deref(),
            Some("converted-nsp"),
            "没有 .tik 也没有 .cert"
        );
    }

    #[test]
    fn 票据文件名的前十六位就是_title_id() {
        assert_eq!(
            rights_id("0100EAE0199040000000000000000010.tik"),
            Some(("0100EAE019904000".to_string(), "10".to_string()))
        );
        // 中间那 14 位不是零就不是一条 RightsId。
        assert_eq!(rights_id("0100eae01990400011111111111111 10.tik"), None);
        assert_eq!(rights_id("0100eae0199040001111111111111110.tik"), None);
        // 不是 hex、长度不对的一律拒。
        assert_eq!(rights_id("说明.tik"), None);
    }

    #[test]
    fn title_id_的尾三位分出本体补丁与附属内容() {
        assert_eq!(Kind::of("0100EAE019904000"), Some(Kind::Base));
        assert_eq!(Kind::of("0100EAE019904800"), Some(Kind::Patch));
        assert_eq!(Kind::of("0100EAE01990512C"), Some(Kind::AddOn));
        // 补丁与附属内容折回本体那一串，三者才归得到同一部作品下。
        // ⚠ DLC 的号段在本体之上一档：抹成 000 会折到一个根本不存在的 TitleID 上。
        for (id, base) in [
            ("0100EAE019904000", "0100EAE019904000"),
            ("0100EAE019904800", "0100EAE019904000"),
            ("0100EAE01990512C", "0100EAE019904000"),
        ] {
            assert_eq!(Kind::base_of(id).as_deref(), Some(base), "{id}");
        }
    }

    #[test]
    fn xci_的两套偏移都探得到() {
        // ⚠ scene 风格的转储从 CardHeader 开始（HEAD 在 0x100），FullXCI 前面补了
        // 0x1000 字节的 CardKeyArea（HEAD 在 0x1100）。两套都真实存在，别硬编码。
        for base in [0_usize, 0x1000] {
            let bytes = xci(base, &["secure"], &["0100a0c01bed80000000000000000010.tik"]);
            let facts = probe("某作.xci", &mut Prefix(&bytes));
            assert_eq!(facts.wrapper.as_deref(), Some("xci"), "base {base:#x}");
            assert_eq!(facts.partitions, vec!["secure".to_string()]);
            assert_eq!(facts.title_id(), Some("0100A0C01BED8000"));
            assert_eq!(
                facts.structure.as_deref(),
                Some("converted-xci"),
                "只有 secure 的是从 NSP 转来的"
            );
        }
    }

    #[test]
    fn scene_风格的卡带三个分区都在() {
        let bytes = xci(
            0,
            &["update", "logo", "normal", "secure"],
            &[
                "0100a0c01bed80000000000000000010.tik",
                "2458773bdce28491e9e0e5043f86015a.nca",
                "8eed26260dbdb1ea545119cc0368fa06.cnmt.nca",
            ],
        );
        let facts = probe("某作.xci", &mut Prefix(&bytes));
        assert_eq!(facts.structure.as_deref(), Some("scene-xci"));
        // **`update` 分区里那些系统更新的 NCA 一条都不收**——它们的 ContentId 查出来
        // 是系统 title 不是游戏，收进来每张卡都会多一条指向系统更新的候选。
        assert_eq!(facts.content_ids.len(), 2);
        assert_eq!(facts.kind.as_deref(), Some("base"));
    }

    #[test]
    fn 手上只有前缀时如实说够不着() {
        let bytes = pfs0(&["0100a0c01bed88000000000000000010.tik"]);
        let facts = probe("某作.nsp", &mut Prefix(&bytes[..0x14]));
        assert!(facts.note.is_some(), "够不着条目表要说出来");
        assert!(!facts.usable());
    }

    #[test]
    fn 不是_switch_容器的照样如实说() {
        let facts = probe("某作.nsp", &mut Prefix(&[0_u8; 0x2000]));
        assert!(facts.note.unwrap().contains("不是 Switch 的容器"));
    }

    #[test]
    fn 四种扩展名都认_别的不认() {
        for name in ["a.nsp", "A.NSZ", "b/c.xci", "d.xcz"] {
            assert!(by_name(name), "{name}");
        }
        for name in ["a.nca", "a.iso", "a.nsp.zip"] {
            assert!(!by_name(name), "{name}");
        }
    }

    #[test]
    fn 合集卡带里两条候选各说各的本体() {
        // ⚠ 真机上撞见过一份合集卡带：一个容器里装着两个游戏、两张票据。
        // 「本体是」这一句取自**这一条候选自己**的 TitleID，不是容器里第一张票据的
        // ——取错了，两条候选里必有一条在撒谎。
        let bytes = xci(
            0,
            &["update", "logo", "normal", "secure"],
            &[
                "0100d49020b848000000000000000012.tik",
                "0100e7c020b828000000000000000012.tik",
            ],
        );
        let facts = probe("合集.xci", &mut Prefix(&bytes));
        let probes = [Probe {
            member: "switch/合集.xci",
            inner: "",
            facts: &facts,
        }];
        let found = candidates(None, Some("SWITCH"), &probes, None).expect("折得出候选");
        assert_eq!(found.candidates.len(), 2);
        for (candidate, base) in found
            .candidates
            .iter()
            .zip(["0100D49020B84000", "0100E7C020B82000"])
        {
            assert!(
                candidate.evidence.contains(&format!("本体是 {base}")),
                "{}",
                candidate.evidence
            );
        }
    }

    #[test]
    fn 文件名里写着好几个_title_id_时逐个比() {
        // 真机上那份合集卡带的名字里就有两个 TitleID，而卡里确实装着两个游戏。
        assert_eq!(
            title_ids_in_name("Double Pack [0100D49020B84000][0100E7C020B82000][v0].xci"),
            vec![
                "0100D49020B84000".to_string(),
                "0100E7C020B82000".to_string()
            ]
        );
        assert!(title_ids_in_name("没有编号的名字.nsp").is_empty());
        // `[v65536]` 这类不是 16 位 hex，不许被切成 TitleID。
        assert!(title_ids_in_name("某作 [v65536].nsp").is_empty());
    }

    #[test]
    fn 文件名写本体而容器里是它的补丁_不是对不上() {
        // 真机五条「对不上」全是这个形状：`[…4000].xci` 里那张票据是 `…4800`。
        // 按原样比会把五张正常的卡全报成警告，而真出事时那句警告就没人信了。
        let bytes = pfs0(&["0100a0c01bed88000000000000000010.tik"]);
        let facts = probe("某作.nsp", &mut Prefix(&bytes));
        let probes = [Probe {
            member: "switch/某作.nsp",
            inner: "",
            facts: &facts,
        }];
        let found = candidates(
            None,
            Some("SWITCH"),
            &probes,
            Some("某作 [0100A0C01BED8000][v0].nsp"),
        )
        .expect("折得出候选");
        assert_eq!(found.name_conflicts, 0);
        assert!(
            found.candidates[0]
                .evidence
                .contains("同一个本体号段，对得上"),
            "{}",
            found.candidates[0].evidence
        );

        let 真对不上 = candidates(
            None,
            Some("SWITCH"),
            &probes,
            Some("某作 [0100FFFFFFFFF000][v0].nsp"),
        )
        .expect("折得出候选");
        assert_eq!(真对不上.name_conflicts, 1);
        assert!(
            真对不上.candidates[0]
                .evidence
                .contains("连本体那一串都对不上"),
            "{}",
            真对不上.candidates[0].evidence
        );
    }

    #[test]
    fn 短码来回折得回来() {
        // 短码是中立库里那张缓存的键，读不回来就等于整张缓存作废。
        for it in [Wrapper::Pfs0, Wrapper::Xci] {
            assert_eq!(Wrapper::from_code(it.code()), Some(it));
        }
        for it in Structure::all() {
            assert_eq!(Structure::from_code(it.code()), Some(it));
        }
        for it in [Kind::Base, Kind::Patch, Kind::AddOn] {
            assert_eq!(Kind::from_code(it.code()), Some(it));
        }
    }
}
