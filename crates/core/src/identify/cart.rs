//! **第三命中层：卡带内部头。** 哈希撞不上时，从卡带自己的字节里读出「这是哪个游戏」。
//!
//! 这一层正面回答最初那个痛点。票 07 的命中率按平台拆开是两个世界：GB 91.5%、FC 82.8%、
//! SFC 80.1%，而 **GBA 5.8%、NDS 4.4%、GBC 0%**——而 GBA 与 NDS 恰恰是中文玩家存量最大
//! 的两个平台，抽样确认那批未命中全是汉化版。**汉化补丁通常不改内部头**（调研 C.2 与
//! D.3.1），于是即使一份汉化版对不上任何数据库，卡带头里那串游戏码照样说得出它基于
//! 哪一次发行。
//!
//! ## 各平台的头在哪、长什么样
//!
//! | 平台 | 判据（magic） | 能撞库的编号 | 出处 |
//! |---|---|---|---|
//! | **GBA** | `[0x03]=0xEA` 且 `[0xB2]=0x96` | Game Code `0x0AC`，4 字节 | GBATEK A.4 |
//! | **NDS** | `u16le[0x15C]=0xCF56`（Logo CRC16 的固定值） | Gamecode `0x00C`，4 字节 | GBATEK A.5 |
//! | **GB / GBC** | `0x104` 起的任天堂 logo | Manufacturer Code `0x13F`，4 字节 | Pan Docs A.3 |
//! | **MD** | `0x100` 起 `"SEGA "` | Serial `0x180`，14 字节 | plutiedev A.8 |
//! | **N64** | 归一化后 `80 37 12 40` | Game Code `0x3B`，4 字节 | n64brew A.7 |
//! | **SFC** | 无 magic，四点打分 | Game Code 扩展头 `−0x0E`，4 字节 | bsnes A.2 |
//! | **FC** | `NES\x1A` | **没有** | NESdev A.1 |
//!
//! **FC 那一行不是遗漏。** NESdev 的 iNES 页全页检索不出 CRC / checksum / serial 三个词
//! ——iNES 是本文所有平台里唯一完全没有自校验结构、也没有内部编号的格式（A.18.1）。
//! 它照样解析，因为解析出来的东西还有两个用处：**验平台**（一份 `.nes` 躺在 `gba/` 里
//! 是真实会发生的，ADR-0011）与**说清楚它为什么撞不上**。FC 那 3,491 个未命中要靠
//! 别的路（GoodNES 那 646 条汉化条目以 SHA-1 为键，见 [`fingerprint`](super::fingerprint)）。
//!
//! ## 归一化在解析之前
//!
//! 两件事在读任何字段之前做完，否则每一个偏移都是错的：
//!
//! - **N64 的字节序**。`.z64` 是大端原生、`.v64` 是半字交换、`.n64` 是字交换
//!   （mupen64plus `src/main/rom.c`）。判据是头四个字节而不是扩展名——n64brew 明确警告
//!   过不要拿那四个字节当固定 ID，但拿来认字节序正是它们的用途。
//! - **SFC 的 512 字节拷贝机头**。`(filesize AND 3FFh)=200h` 时全部偏移 `+0x200`
//!   （fullsnes、snes.nesdev 同一条）。拷贝机头**没有魔数**，唯一的判据是总长的余数,
//!   所以 [`probe`] 一定要拿到总长。
//! - **MD 的 `.smd` 交错**。PicoDrive 的判据是 `size >= 0x4200 && (size & 0x3fff) == 0x200`，
//!   载荷还要去交错才是真正的 ROM。
//!
//! ## 一条卡带游戏码值多少：**发行版级，永不自动通过**
//!
//! 票据原话：「内部头命中产出发行版级候选，置信度低于精确哈希但高于文件名」。这不是
//! 打折，是如实说——**同一个游戏码底下躺着原版转储和一堆汉化版**，正因为补丁不改它。
//! 撞上它意味着「这是哪个游戏」有了答案，「这是谁汉化的第几版」没有，而后者归**裁决**
//! （ADR-0008）。落到代码上是 [`ident::IdKind::release_level_only`](super::ident::IdKind::release_level_only)。
//!
//! ## 为什么这些偏移住在代码里而不是 `platforms.toml`
//!
//! `docs/platforms.md` 把这个选择留给了这张票。答案与票 07 的**外挂头**同源：一张
//! 「偏移 + 长度」的表**表达不了这里真正要做的事**——N64 要先按头四个字节判字节序再整段
//! 交换、SFC 要对四个候选位置按 opcode 打分、GB 的标题长度随卡带年代在 16 / 15 / 11
//! 之间变、MD 的产品码要按 `XX YYYYYYYY-ZZ` 拆开再折平、NDS 要按 ndstool 的表算 CRC-16。
//! 调研 B.5.5 记着同样的结论：数据驱动的 skipper 表达不了「从文件里读出长度」这类规则，
//! RomVault 与 igir 都硬编码在代码里。**搬进清单只会得到一张说不全实情的表。**

use std::borrow::Cow;
use std::path::Path;

use crate::dat::serial as folding;
use crate::path::{extension_lower, file_name_of_key};

use super::ident::{IdKind, Ident};

/// 一种卡带的内部头。**按格式家族分，不按扩展名**——`.gb` 与 `.gbc` 是同一份头，
/// 差别只在 `0x143` 那一个字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cart {
    /// FC / NES 的 iNES / NES 2.0 头。**没有内部编号**。
    Nes,
    /// SFC / SNES 的内部头（LoROM / HiROM / ExHiROM / ExLoROM 四个候选位置）。
    Snes,
    /// GB 与 GBC 共用的卡带头。
    GameBoy,
    /// GBA 卡带头。
    Gba,
    /// NDS / DSi 卡带头。
    Nds,
    /// MD / Genesis（含 32X）卡带头。
    Genesis,
    /// N64 卡带头。**先归一化字节序**。
    N64,
}

impl Cart {
    /// 存进**中立库**用的短码。与 [`label`](Self::label) 分开写，理由与
    /// `disc::Shell::code` 一样：改一次报告用词不该让已经存进库里的记录读不回来。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Nes => "nes",
            Self::Snes => "snes",
            Self::GameBoy => "gb",
            Self::Gba => "gba",
            Self::Nds => "nds",
            Self::Genesis => "md",
            Self::N64 => "n64",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::all().into_iter().find(|it| it.code() == code)
    }

    /// 全部七种，顺序固定。
    #[must_use]
    pub fn all() -> [Self; 7] {
        [
            Self::Nes,
            Self::Snes,
            Self::GameBoy,
            Self::Gba,
            Self::Nds,
            Self::Genesis,
            Self::N64,
        ]
    }

    /// 报告与**依据**里写的那个名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Nes => "iNES / NES 2.0 头",
            Self::Snes => "SFC 内部头",
            Self::GameBoy => "GB / GBC 卡带头",
            Self::Gba => "GBA 卡带头",
            Self::Nds => "NDS 卡带头",
            Self::Genesis => "MD 卡带头",
            Self::N64 => "N64 卡带头",
        }
    }

    /// 这份头说这张卡属于哪几个**平台**（用 `platforms.toml` 里的那套名字）。
    ///
    /// 一份头可以对应不止一个平台：GB 与 GBC 共用一份头，MD 的头在 32X 卡上一模一样。
    /// [`serial`](super::serial) 拿它把撞出来的候选限在同一族里——一个 4 字符的游戏码
    /// 在几万条记录上跨平台撞车是现实存在的（`A83J` 在 SFC 与 GBA 各有一条）。
    #[must_use]
    pub fn platforms(self) -> &'static [&'static str] {
        match self {
            Self::Nes => &["FC", "FDS"],
            Self::Snes => &["SFC"],
            Self::GameBoy => &["GB", "GBC"],
            Self::Gba => &["GBA"],
            Self::Nds => &["NDS"],
            Self::Genesis => &["MD", "32X", "Mega-CD"],
            Self::N64 => &["N64"],
        }
    }
}

/// 光靠名字猜这是哪一种卡带；`hint` 是**目录声明的平台**（ADR-0011 的强先验）。
///
/// **这张表与 `platforms.toml` 的 `扩展名` 那一列不是同一件事**，所以没有读那一份。
/// 清单那一列问的是「这个扩展名只可能属于哪个平台」，用来报目录与内容不一致；
/// 这里问的是「按哪一种**头**解析」——`.gb` 与 `.gbc` 是同一种头两个平台，
/// `.bin` 谁都可能是，而清单里 GB / GBC 干脆没有那一列（`.gb` 与 `.gbc` 互相串目录
/// 是常态，填了只会把报告淹掉）。两张表放在一起，改一处就得记着另一处的用途。
///
/// **`.bin` 只在目录说 MD 时才当卡带**。真库里 `.bin` 有 46,920 条、140 GiB，其中绝大
/// 多数是光盘轨道与游戏内部的封包数据（挂账 D108）——不设这道闸，这一层会为它们每一条
/// 读 16 KiB，把「只读几百字节」翻两个数量级。
#[must_use]
pub fn by_name(name: &str, hint: Option<&str>) -> Option<Cart> {
    let file = file_name_of_key(name);
    let ext = extension_lower(Path::new(file))?;
    match ext.as_str() {
        "nes" | "unf" | "unif" => Some(Cart::Nes),
        "sfc" | "smc" | "swc" | "fig" => Some(Cart::Snes),
        "gb" | "gbc" | "sgb" | "cgb" => Some(Cart::GameBoy),
        "gba" | "agb" => Some(Cart::Gba),
        "nds" | "dsi" | "srl" => Some(Cart::Nds),
        "md" | "gen" | "smd" | "32x" | "68k" => Some(Cart::Genesis),
        "z64" | "v64" | "n64" | "u64" => Some(Cart::N64),
        "bin" if hint == Some("MD") => Some(Cart::Genesis),
        _ => None,
    }
}

/// 探一份内容要读多少字节。`size` 是这份内容的总长度。
///
/// 底线是 `0x400`：**名字可能是错的**（ADR-0011 对文件名同样成立），一份叫 `.gba` 的
/// NDS 卡带要读到 `0x15C` 才认得出来，而按 GBA 只读 `0xC0` 就永远看不见那一处。
/// 一千字节摊在几千个变体上是几 MB，换来的是「目录说 A、文件是 B」这件事报得出来。
///
/// 只有两种超出这个底线：
///
/// - **MD**：`.smd` 的交错载荷要连着 512 字节拷贝机头读满一个 16 KiB 块才去得了交错。
/// - **SFC**：内部头有四个候选位置，LoROM 在 `0x7FC0`、HiROM 在 `0xFFC0`，读 64 KiB
///   就够；**ExHiROM 在 `0x40FFC0`**，那要 4 MiB——而 ExHiROM 恰恰是《幻想传说》
///   《星之海洋》这些最常被汉化的大卡。于是按体积分档：4 MiB 以下的卡不可能是 ExHiROM，
///   读 64 KiB；4 MiB 以上的才付那 4 MiB。
#[must_use]
pub fn probe_len(cart: Cart, size: u64) -> usize {
    /// 名字可能是错的，每一份至少读这么多。
    const FLOOR: u64 = 0x400;
    /// LoROM `0x7FC0` 与 HiROM `0xFFC0` 都在这之内。
    const SNES_NEAR: u64 = 0x1_0000;
    /// ExHiROM / ExLoROM 的头在 `0x40FFC0`。
    const SNES_FAR: u64 = 0x41_0000;
    let want = match cart {
        Cart::Genesis => 0x4200,
        Cart::Snes => {
            // 拷贝机头没有魔数，唯一的判据是总长的余数。它在解析前被剥掉，所以要**多读**
            // 那 512 字节，剥完剩下的才够得到内部头。
            let copier = u64::from(size % 1024 == 512) * 0x200;
            let far = size.saturating_sub(copier) > SNES_FAR - 0x1_0000;
            copier + if far { SNES_FAR } else { SNES_NEAR }
        }
        _ => FLOOR,
    };
    usize::try_from(want.max(FLOOR).min(size.max(FLOOR))).unwrap_or(usize::MAX)
}

/// 一份内容的卡带头探出来的全部事实。
///
/// 它落进中立库的 `content_cart`（与光盘那一层的 `content_disc` 同一条路）：**算过的
/// 不再算**，第二趟识别在这一层上是零字节。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    /// 认出来的是哪一种卡带头，存的是 [`Cart::code`] 那个**短码**；认不出是 `None`。
    pub cart: Option<String>,
    /// 这份头说它属于哪个平台。**与目录声明的平台不一致就是一条冲突**（ADR-0011）。
    pub platform: Option<String>,
    /// 读出来的标识。FC 永远是空的——iNES 头里没有编号。
    pub ids: Vec<Ident>,
    /// 头里写的标题。
    pub title: Option<String>,
    /// 头里写的发行商代码。
    pub maker: Option<String>,
    /// 头里写的版本号。
    pub version: Option<String>,
    /// 头里写的地区码。
    pub region: Option<String>,
    /// 头部校验和自洽吗。`None` = **这份头没有硬件强制的校验和**（SFC 与 MD 都不校验），
    /// 与 `Some(false)`（有、但对不上）是两回事。
    ///
    /// 它不参与命中，只写进依据：调研 D.3.3 的那条推断——**校验和自洽而整文件哈希撞不上
    /// 任何 DAT**，高度提示这是一份「被人改过内容、又把头修回去」的**变体**——
    /// 汉化版正是这个形状。
    pub checksum: Option<bool>,
    /// 解析之前做了什么归一化（字节序、拷贝机头、去交错）；什么都没做时是 `None`。
    pub normalized: Option<String>,
    /// 读不出想要的那一段时，说人话的一句。
    pub note: Option<String>,
}

/// 探一份内容的卡带头。
///
/// `head` 是它的前若干字节（长度见 [`probe_len`]），`size` 是**总长度**——拷贝机头与
/// `.smd` 交错的判据都只能从总长来。`hint` 是目录声明的平台。
///
/// **magic 优先于名字**：每一种带 magic 的头都验一遍，谁的 magic 对上算谁的。名字只在
/// 一处说了算——SFC 没有 magic，只能靠四点打分，而在任意字节上打分会打出假阳性，
/// 所以那一档要名字或目录先说「这是一张 SFC 卡」。
#[must_use]
pub fn probe(name: &str, head: &[u8], size: u64, hint: Option<&str>) -> Facts {
    // 归一化在读任何字段之前：偏移全都建立在归一化之后的字节上。
    let (bytes, normalized) = normalize(name, head, size);
    let mut facts = Facts {
        normalized,
        ..Facts::default()
    };
    // 归一化已经把字节序与拷贝机头处理掉了，所以每一种头的判据都只看字节——
    // 总长在这之后一个解析器都用不着。
    for probe in [
        read_nes as fn(&mut Facts, &[u8]) -> bool,
        read_gba,
        read_nds,
        read_gameboy,
        read_genesis,
        read_n64,
    ] {
        if probe(&mut facts, &bytes) {
            return facts;
        }
    }
    // SFC 没有 magic。**只在名字或目录说它是 SFC 时才打分**——bsnes 那套启发式在
    // 随便一段字节上也打得出分来，那会给每一份认不出的文件安一个假的游戏码。
    let expected = by_name(name, hint);
    if expected == Some(Cart::Snes) || hint == Some("SFC") {
        if read_snes(&mut facts, &bytes) {
            return facts;
        }
        facts.note = Some(
            "SFC 内部头认不出来：LoROM / HiROM / ExHiROM / ExLoROM 四个候选位置按 bsnes 的\
             打分法一个都不及格（复位向量 < 0x8000，或者读得不够远）"
                .to_string(),
        );
        return facts;
    }
    facts.note = Some(format!(
        "{}：读到了前 {} 字节，但里面没有任何一种卡带头的 magic",
        expected.map_or("卡带头认不出来".to_string(), |cart| format!(
            "名字说这是 {}，字节里却对不上",
            cart.label()
        )),
        head.len()
    ));
    facts
}

/// 归一化：把磁盘上那串字节折成「解析器认得的那种排法」。
///
/// 返回归一化之后的字节，以及**做了什么**——后者要写进依据。做了归一化却不说，事后
/// 没人分得清一条读错的字段是解析器错了还是归一化没做。
fn normalize<'a>(name: &str, head: &'a [u8], size: u64) -> (Cow<'a, [u8]>, Option<String>) {
    // N64：判据是头四个字节，不是扩展名。mupen64plus `src/main/rom.c` 的三种。
    match head.get(..4) {
        Some([0x37, 0x80, 0x40, 0x12]) => {
            return (
                Cow::Owned(swap16(head)),
                Some("N64 字节序：半字交换（.v64）→ 大端 z64".to_string()),
            );
        }
        Some([0x40, 0x12, 0x37, 0x80]) => {
            return (
                Cow::Owned(swap32(head)),
                Some("N64 字节序：字交换（.n64）→ 大端 z64".to_string()),
            );
        }
        _ => {}
    }
    // MD 的 `.smd`：512 字节拷贝机头 + 16 KiB 一块的交错载荷（PicoDrive `pico/media.c`
    // 的 `romsize >= 0x4200 && (romsize & 0x3fff) == 0x200`）。
    //
    // **那条判据要连扩展名一起看**：它与 SFC 拷贝机头那条（`总长 % 1024 == 512`）在
    // 「2 MiB + 512」这类体积上完全重合，只按余数判会把一份带拷贝机头的 `.smc` 当成
    // 交错的 MD 卡去交错——交错完连 magic 都不剩。交错是 `.smd` 这个格式自己的形态，
    // 扩展名正是它唯一的声明。
    let smd = extension_lower(Path::new(file_name_of_key(name))).is_some_and(|ext| ext == "smd");
    if smd && size >= 0x4200 && (size & 0x3fff) == 0x200 && head.len() > 0x200 {
        let payload = &head[0x200..];
        if payload.len() >= 0x4000 {
            return (
                Cow::Owned(deinterleave(&payload[..0x4000])),
                Some(
                    "MD `.smd`：剥掉 512 字节拷贝机头，再把 16 KiB 一块的交错载荷去交错"
                        .to_string(),
                ),
            );
        }
    }
    // SFC 的**拷贝机头**：判据是总长的余数（拷贝机头没有魔数），判定复用票 07 那一层
    // ——同一件事只该有一处判据，两处各写一遍迟早会分岔。**剥掉而不是把偏移后推 512**：
    // bsnes 就是先剥再打分，而打分要按 `(address & ~0x7fff) | (reset & 0x7fff)` 去取
    // 复位入口那条指令，偏移一后推，那个取址就落到文件外面去了。
    //
    // 别的四种外挂头这里一概不剥：iNES 那 16 字节**正是** [`read_nes`] 要读的东西，
    // 而 FDS / Lynx / A7800 这一层压根不解析。判据里已经带了扩展名（`is_snes`），
    // 所以一份 `.smd` 走不到这儿。
    if super::header::dump_header(name, head, size) == Some(super::header::DumpHeader::SnesCopier)
        && head.len() > 0x200
    {
        return (
            Cow::Borrowed(&head[0x200..]),
            Some(
                "SFC 拷贝机头：总长 % 1024 == 512，剥掉开头那 512 字节再解析（fullsnes）"
                    .to_string(),
            ),
        );
    }
    (Cow::Borrowed(head), None)
}

/// 半字（16 位）交换：`.v64` 的排法。
///
/// 公开出去是为了**测试能拿同一份实现去造那两种排法**——测试自己抄一份，抄错了就
/// 变成「两份错得一样的代码互相点头」。
#[must_use]
pub fn swap16(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for pair in out.as_chunks_mut::<2>().0 {
        pair.swap(0, 1);
    }
    out
}

/// 字（32 位）交换：`.n64` 的排法。
#[must_use]
pub fn swap32(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    for word in out.as_chunks_mut::<4>().0 {
        word.swap(0, 3);
        word.swap(1, 2);
    }
    out
}

/// `.smd` 的去交错：一块 16 KiB 里，前 8 KiB 是奇数位字节，后 8 KiB 是偶数位字节。
fn deinterleave(block: &[u8]) -> Vec<u8> {
    let half = block.len() / 2;
    let mut out = vec![0u8; half * 2];
    for at in 0..half {
        out[at * 2] = block[half + at];
        out[at * 2 + 1] = block[at];
    }
    out
}

// ── FC ────────────────────────────────────────────────────────────────────

/// iNES / NES 2.0 头。**读得出字段，读不出编号。**
fn read_nes(facts: &mut Facts, head: &[u8]) -> bool {
    if !head.starts_with(b"NES\x1A") {
        return false;
    }
    facts.cart = Some(Cart::Nes.code().to_string());
    facts.platform = Some("FC".to_string());
    let Some(bytes) = head.get(..16) else {
        facts.note = Some("iNES 头不全：读到的字节不够 16 个".to_string());
        return true;
    };
    // NESdev 的四步判定（A.1.4）。第三步那句「bytes 12–15 全为 0」是唯一能把
    // 「干净的 iNES 1.0」与「被 ripper 署名污染的脏头」分开的判据——最著名的污染是
    // `"DiskDude!"`，它令 byte7 = 'D' = 0x44，`0x44 & 0x0C = 0x04`，于是被误判成
    // archaic iNES。**NES 头部字节整体不可信，识别绝不能依赖它**（A.1.4 原文）。
    let kind = match bytes[7] & 0x0C {
        0x08 => "NES 2.0",
        0x04 => "archaic iNES",
        0x00 if bytes[12..16].iter().all(|b| *b == 0) => "iNES 1.0",
        _ => "iNES 0.7 或 archaic iNES（头部被 ripper 署名污染过）",
    };
    let mapper = u16::from(bytes[6] >> 4) | (u16::from(bytes[7] & 0xF0));
    let trainer = bytes[6] & 0x04 != 0;
    facts.note = Some(format!(
        "{kind}：PRG {} KiB、CHR {} KiB、mapper {mapper}{}。\
         **iNES 格式本身没有任何编号与校验和**（NESdev 全页检索不出 CRC / checksum / serial），\
         这一层给不出可以撞 DAT 的编号——FC 只能靠哈希",
        u32::from(bytes[4]) * 16,
        u32::from(bytes[5]) * 8,
        if trainer {
            "，带 512 字节 trainer（PRG 数据整体后移）"
        } else {
            ""
        },
    ));
    true
}

// ── GBA ───────────────────────────────────────────────────────────────────

/// GBA 卡带头（GBATEK A.4）。
///
/// magic 照 mGBA 的 `GBAIsROM`：`[0x03] == 0xEA`（入口那条 ARM 分支指令）加
/// `[0xB2] == 0x96`（固定值）。**两个字节就够，误判率极低，而且容忍被改过的 logo**
/// ——整段 156 字节的 logo 反倒不能整段比：GBATEK 明说 `0x09C` 与 `0x09E` 有可变位。
fn read_gba(facts: &mut Facts, head: &[u8]) -> bool {
    if head.get(0x03) != Some(&0xEA) || head.get(0xB2) != Some(&0x96) {
        return false;
    }
    facts.cart = Some(Cart::Gba.code().to_string());
    facts.platform = Some("GBA".to_string());
    facts.title = ascii_text(head.get(0xA0..0xAC).unwrap_or_default());
    facts.maker = ascii_text(head.get(0xB0..0xB2).unwrap_or_default());
    facts.version = head.get(0xBC).map(|byte| format!("v{byte}"));
    // Complement check（GBATEK 原文）：`chk=0 : for i=0A0h to 0BCh : chk=chk-[i] :
    // next : chk=(chk-19h) and 0FFh`，覆盖 29 字节，**硬件强制**。
    if let (Some(span), Some(stored)) = (head.get(0xA0..0xBD), head.get(0xBD)) {
        let sum = span.iter().fold(0u8, |acc, byte| acc.wrapping_sub(*byte));
        facts.checksum = Some(sum.wrapping_sub(0x19) == *stored);
    }
    let code = head.get(0xAC..0xB0).unwrap_or_default();
    // Game Code 的第四位是 Destination/Language（GBATEK 的 `UTTD` 拆分）。
    facts.region = code.get(3).map(|byte| (*byte as char).to_string());
    push_code(
        facts,
        code,
        "GBA 卡带头 0x0AC 的 Game Code（AGB-UTTD，GBATEK）",
    );
    true
}

// ── NDS ───────────────────────────────────────────────────────────────────

/// NDS 卡带头（GBATEK A.5）。
///
/// magic 用 `0x15C` 处那个**固定的 logo CRC-16 `CF56h`**——GBATEK 原文：BIOS 只验
/// `[15Ch]=CF56h`，压根不验 `[0C0h-15Bh]` 那段真实数据。调研 A.18.3 第 9 条专门更正过
/// 一个常见错误：`0x15C` **不是** secure area 的 CRC（那个在 `0x06C`）。
fn read_nds(facts: &mut Facts, head: &[u8]) -> bool {
    if u16_le(head, 0x15C) != Some(0xCF56) {
        return false;
    }
    facts.cart = Some(Cart::Nds.code().to_string());
    // Unitcode（`0x012`）：`00h`=NDS、`02h`=NDS+DSi、`03h`=DSi。这个库里 DSi 专属卡
    // 没有单独的平台，一律记 NDS。
    facts.platform = Some("NDS".to_string());
    facts.title = ascii_text(head.get(0x00..0x0C).unwrap_or_default());
    facts.maker = ascii_text(head.get(0x10..0x12).unwrap_or_default());
    facts.version = head.get(0x1E).map(|byte| format!("v{byte}"));
    facts.region = head.get(0x1D).map(|byte| {
        match byte {
            0x00 => "Normal",
            0x40 => "Korea",
            0x80 => "China",
            _ => "未知",
        }
        .to_string()
    });
    // Header CRC-16（`0x15E`，覆盖 `0x000..0x15E`）。**按 ndstool 实现，不按 GBATEK 的
    // 伪代码**——调研 A.5 实测那段伪代码逐字实现复现不出标准校验值。
    if let (Some(span), Some(stored)) = (head.get(..0x15E), u16_le(head, 0x15E)) {
        facts.checksum = Some(crc16(span) == stored);
    }
    push_code(
        facts,
        head.get(0x00C..0x010).unwrap_or_default(),
        "NDS 卡带头 0x00C 的 Gamecode（NTR-<code>，GBATEK）",
    );
    true
}

/// ndstool `source/crc.h` 的 CRC-16：init `0xFFFF`，无最终取反。
///
/// 形式上是 **CRC-16/MODBUS**（多项式 `0x8005` 反射即 `0xA001`）。调研 A.5 专门更正过：
/// **GBATEK 那段伪代码逐字实现复现不出标准校验值，要照 ndstool 写。**
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for byte in bytes {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            let carry = crc & 1 != 0;
            crc >>= 1;
            if carry {
                crc ^= 0xA001;
            }
        }
    }
    crc
}

// ── GB / GBC ──────────────────────────────────────────────────────────────

/// 任天堂 logo 的前四个字节。CGB 只验前半段（`0x0104`–`0x011B`），而 mGBA 的实现
/// 只比对这四个（`src/gb/gb.c`）——盗版卡与汉化版改过后半段的真实存在。
const GB_LOGO_HEAD: [u8; 4] = [0xCE, 0xED, 0x66, 0x66];

/// GB / GBC 卡带头（Pan Docs A.3）。
fn read_gameboy(facts: &mut Facts, head: &[u8]) -> bool {
    if head.get(0x104..0x108) != Some(&GB_LOGO_HEAD[..]) {
        return false;
    }
    facts.cart = Some(Cart::GameBoy.code().to_string());
    // CGB flag（`0x143`）：`$80` = 兼容单色的 CGB 卡，`$C0` = 仅 CGB。
    let cgb = matches!(head.get(0x143), Some(0x80 | 0xC0));
    facts.platform = Some(if cgb { "GBC" } else { "GB" }.to_string());
    facts.version = head.get(0x14C).map(|byte| format!("v{byte}"));
    facts.region = head.get(0x14A).map(|byte| {
        match byte {
            0x00 => "Japan",
            _ => "Overseas",
        }
        .to_string()
    });
    // Header checksum（`0x14D`，覆盖 `0x134..=0x14C`）。**boot ROM 会验，不符则不启动**
    // ——所以合法卡上它必然正确，而一份改过标题却仍然自洽的卡说明汉化者修过头
    // （调研 D.3.3）。
    if let (Some(span), Some(stored)) = (head.get(0x134..0x14D), head.get(0x14D)) {
        let sum = span
            .iter()
            .fold(0u8, |acc, byte| acc.wrapping_sub(*byte).wrapping_sub(1));
        facts.checksum = Some(sum == *stored);
    }
    // **Manufacturer Code（`0x13F`，4 字节）只在新卡上存在**。Pan Docs 原话：
    // 「In older cartridges these bytes were part of the Title」。判据取
    // `0x14B == 0x33`（改用 New licensee code，也就是后期卡）——老卡上那四个字节是
    // 标题的一部分，当成编号去撞库会撞出一堆毫不相干的条目。
    let modern = head.get(0x14B) == Some(&0x33);
    let title_len = if modern { 0x0B } else { 0x10 };
    facts.title = ascii_text(head.get(0x134..0x134 + title_len).unwrap_or_default());
    facts.maker = ascii_text(head.get(0x144..0x146).unwrap_or_default());
    if modern {
        push_code(
            facts,
            head.get(0x13F..0x143).unwrap_or_default(),
            "GB / GBC 卡带头 0x13F 的 Manufacturer Code（Pan Docs）",
        );
    }
    if facts.ids.is_empty() {
        facts.note = Some(format!(
            "{}：GB 卡带头里没有任何编号可以撞库{}",
            if cgb { "GBC" } else { "GB" },
            if modern {
                "——0x13F 那四个字节不是大写字母数字，不当编号用"
            } else {
                "——0x14B 不是 33h，这是一张老卡，0x13F 那四个字节是标题的一部分（Pan Docs）"
            }
        ));
    }
    true
}

// ── MD ────────────────────────────────────────────────────────────────────

/// MD / Genesis 卡带头（plutiedev A.8）。
///
/// magic 是 `0x100` 起那 16 字节的主机名。**TMSS 机型没有 `"SEGA"` 前缀就不启动**，
/// 所以它是这个平台最硬的判据；已知取值有 `"SEGA MEGA DRIVE"`、`"SEGA GENESIS"`、
/// `"SEGA 32X"`、`"SEGA PICO"` 等。
fn read_genesis(facts: &mut Facts, head: &[u8]) -> bool {
    let Some(system) = head.get(0x100..0x110) else {
        return false;
    };
    if !system.starts_with(b"SEGA") && !system.starts_with(b" SEGA") {
        return false;
    }
    facts.cart = Some(Cart::Genesis.code().to_string());
    let system_text = ascii_text(system).unwrap_or_default();
    facts.platform = Some(
        if system_text.contains("32X") {
            "32X"
        } else {
            "MD"
        }
        .to_string(),
    );
    // 两栏标题都读，**取看得懂的那一栏**：国内标题是唯一允许非 ASCII（Shift-JIS）的
    // 字段，而汉化版常把其中一栏整个改写成 GBK 中文——两栏里哪一栏是乱码事先说不准，
    // 固定取一栏就会有一半的卡交出一串空格。判据是「可打印 ASCII 字符多的那一栏」。
    facts.title = pick_title(
        head.get(0x150..0x180).unwrap_or_default(),
        head.get(0x120..0x150).unwrap_or_default(),
    );
    facts.maker = ascii_text(head.get(0x110..0x120).unwrap_or_default());
    facts.region = ascii_text(head.get(0x1F0..0x1F3).unwrap_or_default());
    // Checksum（`0x18E`，大端 16 位，范围 `0x000200` 到 ROM 末尾）。**主机不校验**，
    // 而这一层只读了头，算不出实算值——所以这里留 `None`：没有硬件强制的校验和
    // 与「有、但对不上」是两回事。
    //
    // Serial（`0x180`，14 字节 `"XX YYYYYYYY-ZZ"`）：前两字节是软件类型（`GM`=Game、
    // `AI`=Aid、`BR`=Sega CD 的 Boot ROM），**编号从 `0x183` 起**。No-Intro 记的是
    // `T-48073-00` / `MK-1281` 这种形式，正好等于 `0x183..0x18E` 折平之后那一串。
    let full = ascii_text(head.get(0x180..0x18E).unwrap_or_default());
    push_code_text(
        facts,
        head.get(0x183..0x18E).unwrap_or_default(),
        &format!(
            "MD 卡带头 0x180 的 Serial{}（plutiedev）",
            full.map_or(String::new(), |text| format!("「{text}」"))
        ),
    );
    if facts.ids.is_empty() {
        facts.note = Some(
            "MD 卡带头 0x180 的 Serial 是空的（改过头的变体与 homebrew 常这样），撞不了库"
                .to_string(),
        );
    }
    true
}

// ── N64 ───────────────────────────────────────────────────────────────────

/// N64 卡带头（n64brew A.7）。**偏移全部针对归一化之后的大端 z64。**
fn read_n64(facts: &mut Facts, head: &[u8]) -> bool {
    if head.get(..4) != Some(&[0x80, 0x37, 0x12, 0x40][..]) {
        return false;
    }
    facts.cart = Some(Cart::N64.code().to_string());
    facts.platform = Some("N64".to_string());
    facts.title = ascii_text(head.get(0x20..0x34).unwrap_or_default());
    facts.version = head.get(0x3F).map(|byte| format!("v{byte}"));
    facts.region = head.get(0x3E).map(|byte| (*byte as char).to_string());
    // `0x10` 那 8 字节是 IPL3 算出来的 check code（**不是 CRC**，n64brew 原话），
    // 各数据库实际拿它当主键。这一层不拿它撞 DAT——No-Intro 的 N64 集把 4 字符的
    // Game Code 写在 `<rom serial>` 上，那才是撞得着的那一串——但它写进依据：
    // 人在裁决队列里拿它去 n64brew 上核对是一眼的事。
    if let Some(check) = head.get(0x10..0x18) {
        let hex: String = check.iter().map(|byte| format!("{byte:02X}")).collect();
        facts.maker = Some(format!("check code {hex}"));
    }
    push_code(
        facts,
        head.get(0x3B..0x3F).unwrap_or_default(),
        "N64 卡带头 0x3B 的 Game Code（归一化到 z64 之后，n64brew）",
    );
    true
}

// ── SFC ───────────────────────────────────────────────────────────────────

/// SFC 内部头（bsnes `heuristics/super-famicom.cpp` 的打分法，A.2.5）。
///
/// 四个候选位置各打一次分，取最高的那个。**分值与判据逐条照抄 bsnes**，不自己发明
/// 第五套：这套启发式已经在一个成熟模拟器上跑过无数张卡，而「换个位置也能凑合读出
/// 一串字」正是自创判据最容易犯的错。
fn read_snes(facts: &mut Facts, head: &[u8]) -> bool {
    // **拷贝机头已经在 [`normalize`] 里剥掉了**，这里看到的偏移就是卡带上的偏移。
    let mut best: Option<(i32, usize)> = None;
    for at in [0x7FB0usize, 0xFFB0, 0x40_7FB0, 0x40_FFB0] {
        let score = snes_score(head, at);
        if score > 0 && best.is_none_or(|(seen, _)| score > seen) {
            best = Some((score, at));
        }
    }
    let Some((_, at)) = best else {
        return false;
    };
    facts.cart = Some(Cart::Snes.code().to_string());
    facts.platform = Some("SFC".to_string());
    // 主头在扩展头之后 0x10 处。标题 21 字节、地区 +0x19、版本 +0x1B。
    let main = at + 0x10;
    facts.title = ascii_text(head.get(main..main + 0x15).unwrap_or_default());
    facts.version = head.get(main + 0x1B).map(|byte| format!("v{byte}"));
    // **region 字段不可信**——bsnes 源码注释原话（"Unlicensed software (homebrew,
    // ROM hacks, etc) often change the standard region code, and then neglect to change
    // the extended header region code"）：于是「完整的序列号 + 地区码」压根解不出来。
    // 所以这里只把它当一条写进依据的字段，不拿它做任何判断。
    facts.region = head
        .get(main + 0x19)
        .map(|byte| format!("0x{byte:02X}（bsnes：这个字段不可信）"));
    // Checksum 与它的补码（`+0x1C` / `+0x1E`）：主机不校验，这里只报「这一对自洽吗」。
    if let (Some(complement), Some(checksum)) = (u16_le(head, at + 0x2C), u16_le(head, at + 0x2E)) {
        facts.checksum = Some(complement.wrapping_add(checksum) == 0xFFFF);
    }
    // **扩展头要 `+0x1A == 0x33` 才存在**（`0x33` 是「改用扩展头」的开关）。
    // 没开的老卡上 `at..at+0x10` 那 16 字节是别的东西，当编号用会撞出一堆无关条目。
    if head.get(main + 0x1A) == Some(&0x33) {
        facts.maker = ascii_text(head.get(at..at + 2).unwrap_or_default());
        push_code(
            facts,
            head.get(at + 2..at + 6).unwrap_or_default(),
            "SFC 扩展头 0x7FB2 / 0xFFB2 的 Game Code（fullsnes；要 0x7FDA == 33h 才存在）",
        );
    }
    if facts.ids.is_empty() {
        facts.note = Some(
            "SFC 内部头读出来了，但**没有扩展头**（0x7FDA / 0xFFDA 不是 33h）——\
             老卡没有 Game Code 这样东西，这一层给不出可以撞库的编号"
                .to_string(),
        );
    }
    true
}

/// bsnes 对一个候选位置的打分。`at` 是**扩展头**的地址，主头在它之后 `0x10` 处。
fn snes_score(head: &[u8], at: usize) -> i32 {
    // 前置：够不到 0x50 字节就 0 分。
    if head.len() < at + 0x50 {
        return 0;
    }
    let Some(reset) = u16_le(head, at + 0x4C) else {
        return 0;
    };
    // 「$00:0000-7fff is never ROM data」——复位向量落在那儿就不是一份 SFC 头。
    if reset < 0x8000 {
        return 0;
    }
    let mut score = 0;
    // 复位入口那条指令是什么。取址照 bsnes：`data[(address & ~0x7fff) | (reset & 0x7fff)]`。
    let opcode = head
        .get((at & !0x7FFF) | usize::from(reset & 0x7FFF))
        .copied();
    match opcode {
        Some(0x78 | 0x18 | 0x38 | 0x9C | 0x4C | 0x5C) => score += 8,
        Some(0xC2 | 0xE2 | 0xAD | 0xAE | 0xAC | 0xAF | 0xA9 | 0xA2 | 0xA0 | 0x20 | 0x22) => {
            score += 4;
        }
        Some(0x40 | 0x60 | 0x6B | 0xCD | 0xEC | 0xCC) => score -= 4,
        Some(0x00 | 0x02 | 0xDB | 0x42 | 0xFF) => score -= 8,
        _ => {}
    }
    if let (Some(complement), Some(checksum)) = (u16_le(head, at + 0x2C), u16_le(head, at + 0x2E))
        && complement.wrapping_add(checksum) == 0xFFFF
    {
        score += 4;
    }
    let map_mode = head.get(at + 0x25).map(|byte| byte & !0x10);
    if at == 0x7FB0 && map_mode == Some(0x20) {
        score += 2;
    }
    if at == 0xFFB0 && map_mode == Some(0x21) {
        score += 2;
    }
    // Snes9x 的两条附加判据（`memmap.cpp`）：标题与扩展头前 6 字节要全 ASCII。
    // 它们挡的是「随便一段字节恰好凑够分」，正是这一层最怕的那种假阳性。
    if !all_ascii(head.get(at + 0x10..at + 0x24).unwrap_or_default()) {
        score -= 1;
    }
    if !all_ascii(head.get(at..at + 6).unwrap_or_default()) {
        score -= 1;
    }
    score
}

// ── 共用 ──────────────────────────────────────────────────────────────────

/// 把一串**四字符游戏码**折成一条标识。不是那个形状就一条都不落。
///
/// 形状卡死在「全是大写字母或数字」：卡带头里那几个字节在改过头的变体上什么都可能是，
/// 而 `dat::serial::normalize` 只管折平不管形状——放一段随机字节过去，就会得到一条
/// 自称读出了编号的假依据。
fn push_code(facts: &mut Facts, raw: &[u8], from: &str) {
    if raw.len() != 4
        || !raw
            .iter()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    {
        return;
    }
    push_code_text(facts, raw, from);
}

/// 把一段任意长度的编号折成一条标识（MD 的产品码是 11 字节，还带空格与连字符）。
fn push_code_text(facts: &mut Facts, raw: &[u8], from: &str) {
    let Some(shown) = ascii_text(raw) else {
        return;
    };
    let Some(key) = folding::normalize(&shown) else {
        return;
    };
    let title = facts.title.clone();
    let version = facts.version.clone();
    facts.ids.push(Ident {
        key,
        shown,
        kind: IdKind::GameCode,
        from: from.to_string(),
        title,
        version,
    });
}

/// 两栏标题里挑看得懂的那一栏。可打印 ASCII 字符多的赢；一样多时取前一栏。
fn pick_title(first: &[u8], second: &[u8]) -> Option<String> {
    let graphic = |bytes: &[u8]| bytes.iter().filter(|byte| byte.is_ascii_graphic()).count();
    if graphic(second) > graphic(first) {
        return ascii_text(second).or_else(|| ascii_text(first));
    }
    ascii_text(first).or_else(|| ascii_text(second))
}

/// 一段字节里的可打印 ASCII，去掉首尾空白与填充；一个可打印字符都没有时是 `None`。
fn ascii_text(bytes: &[u8]) -> Option<String> {
    let text: String = bytes
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| {
            if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                ' '
            }
        })
        .collect();
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn all_ascii(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
}

fn u16_le(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::cart as real;

    #[test]
    fn 短码能来回折() {
        for cart in Cart::all() {
            assert_eq!(Cart::from_code(cart.code()), Some(cart));
        }
        assert_eq!(Cart::from_code("GBA 卡带头"), None, "展示词不是键");
    }

    #[test]
    fn bin_只在目录说_md_时才当卡带() {
        // 真库里 `.bin` 有 46,920 条、140 GiB，绝大多数是光盘轨道与封包数据（挂账 D108）。
        assert_eq!(by_name("MD/游戏.bin", Some("MD")), Some(Cart::Genesis));
        assert_eq!(by_name("PS1/Track01.bin", Some("PS1")), None);
        assert_eq!(by_name("某处/x.bin", None), None);
    }

    #[test]
    fn 每一份至少读到_0x400_免得名字骗了人() {
        // 一份叫 `.gba` 的 NDS 卡带要读到 0x15C 才认得出来。
        assert!(probe_len(Cart::Gba, 8 << 20) >= 0x400);
        assert_eq!(probe_len(Cart::Genesis, 8 << 20), 0x4200);
        // 4 MiB 以下的 SFC 卡不可能是 ExHiROM，只读 64 KiB。
        assert!(probe_len(Cart::Snes, 2 << 20) < 0x2_0000);
        assert!(
            probe_len(Cart::Snes, 6 << 20) > 0x40_0000,
            "ExHiROM 要读到 4 MiB"
        );
        // 拷贝机头把每一个偏移往后推 512。
        assert!(probe_len(Cart::Snes, (2 << 20) + 512) > probe_len(Cart::Snes, 2 << 20));
        // 比想读的还短的文件只读到它自己那么长。
        assert_eq!(probe_len(Cart::Genesis, 64), 0x400);
    }

    #[test]
    fn gba_的真实头读得出游戏码() {
        let facts = probe("x.gba", &real::GBA_ROCKMAN_EXE6, 1 << 26, Some("GBA"));
        assert_eq!(facts.cart.as_deref(), Some("gba"));
        assert_eq!(facts.platform.as_deref(), Some("GBA"));
        assert_eq!(facts.title.as_deref(), Some("ROCKEXE6_RXX"));
        assert_eq!(facts.maker.as_deref(), Some("08"));
        assert_eq!(facts.region.as_deref(), Some("J"));
        assert_eq!(facts.ids.len(), 1);
        assert_eq!(facts.ids[0].shown, "BR6J");
        assert_eq!(facts.ids[0].key, "BR6J");
        assert_eq!(facts.ids[0].kind, IdKind::GameCode);
        assert_eq!(facts.checksum, Some(true), "硬件强制校验的那一位对得上");
    }

    #[test]
    fn nds_的真实头读得出_gamecode() {
        let facts = probe("x.nds", &real::NDS_GYAKUTEN_KENJI, 1 << 27, Some("NDS"));
        assert_eq!(facts.cart.as_deref(), Some("nds"));
        assert_eq!(facts.platform.as_deref(), Some("NDS"));
        assert_eq!(facts.title.as_deref(), Some("GYAKUKEN"));
        assert_eq!(facts.ids.len(), 1);
        assert_eq!(facts.ids[0].shown, "C32J");
        assert_eq!(facts.region.as_deref(), Some("Normal"));
    }

    #[test]
    fn gbc_的真实头读得出_manufacturer_code() {
        let facts = probe("x.gbc", &real::GBC_TWINE, 2 << 20, Some("GBC"));
        assert_eq!(facts.cart.as_deref(), Some("gb"));
        assert_eq!(
            facts.platform.as_deref(),
            Some("GBC"),
            "0x143 是 C0，仅 CGB"
        );
        assert_eq!(facts.title.as_deref(), Some("TWINE CGB"));
        assert_eq!(facts.ids.len(), 1);
        assert_eq!(facts.ids[0].shown, "BO7E");
        assert_eq!(
            facts.checksum,
            Some(true),
            "boot ROM 会验这一位，汉化者改了标题就得修好它"
        );
    }

    #[test]
    fn md_的真实头读得出产品码而且与_no_intro_对得上() {
        let facts = probe("x.md", &real::MD_SHINING_FORCE_2, 3 << 20, Some("MD"));
        assert_eq!(facts.cart.as_deref(), Some("md"));
        assert_eq!(facts.platform.as_deref(), Some("MD"));
        assert_eq!(facts.ids.len(), 1);
        // No-Intro 记的是 `G-5521-00`，折平之后两边是同一串。
        assert_eq!(facts.ids[0].key, "G552100");
        assert_eq!(facts.region.as_deref(), Some("JI"));
    }

    #[test]
    fn sfc_的真实头四点打分找得到_hirom() {
        // 这一份是 2 MiB 整、没有拷贝机头，头在 HiROM 的 0xFFB0。
        let rom = real::snes_rom();
        let facts = probe("x.smc", &rom, 2 << 20, Some("SFC"));
        assert_eq!(facts.cart.as_deref(), Some("snes"));
        assert_eq!(facts.ids.len(), 1, "0x7FDA == 33h，扩展头在");
        assert_eq!(facts.ids[0].shown, "A83J");
        assert_eq!(facts.checksum, Some(true), "补码与校验和相加是 0xFFFF");
        assert_eq!(facts.normalized, None, "总长是 1024 的整数倍，没有拷贝机头");
    }

    #[test]
    fn sfc_带拷贝机头时先剥掉再解析() {
        // 同一份字节，前面挂 512 字节拷贝机头：剥掉之后认出来的东西必须一模一样。
        let mut smc = vec![0u8; 0x200];
        smc.extend_from_slice(&real::snes_rom());
        let facts = probe("x.smc", &smc, (2 << 20) + 512, Some("SFC"));
        assert_eq!(facts.ids.len(), 1);
        assert_eq!(facts.ids[0].shown, "A83J");
        assert!(facts.normalized.is_some_and(|it| it.contains("拷贝机头")));
    }

    #[test]
    fn n64_三种字节序都归一化到同一个游戏码() {
        let z64 = &real::N64_PAPER_MARIO;
        let facts = probe("x.z64", z64, 42 << 20, Some("N64"));
        assert_eq!(facts.ids[0].shown, "NMQE");
        assert_eq!(facts.title.as_deref(), Some("PAPER MARIO"));
        assert_eq!(facts.normalized, None, "z64 本来就是大端");

        // `.v64` 是半字交换，`.n64` 是字交换——同一份卡带的另外两种排法。
        let v64 = swap16(z64);
        let facts = probe("x.v64", &v64, 42 << 20, Some("N64"));
        assert_eq!(facts.ids[0].shown, "NMQE");
        assert!(facts.normalized.is_some_and(|it| it.contains("半字交换")));

        let n64 = swap32(z64);
        let facts = probe("x.n64", &n64, 42 << 20, Some("N64"));
        assert_eq!(facts.ids[0].shown, "NMQE");
        assert!(facts.normalized.is_some_and(|it| it.contains("字交换")));
    }

    #[test]
    fn fc_解析得出头却给不出编号() {
        let facts = probe("x.nes", &real::NES_KUAIJIE, 262_160, Some("FC"));
        assert_eq!(facts.cart.as_deref(), Some("nes"));
        assert_eq!(facts.platform.as_deref(), Some("FC"));
        assert!(facts.ids.is_empty(), "iNES 格式里没有编号（NESdev）");
        let note = facts.note.expect("要说清楚为什么给不出");
        assert!(note.contains("没有任何编号"));
        assert!(note.contains("iNES 1.0"), "干净的 iNES 1.0：{note}");
    }

    #[test]
    fn 目录说一个平台字节说另一个() {
        // ADR-0011：目录只是强先验，文件内容可以推翻它。真实会发生（下错、放错、混装）。
        let facts = probe(
            "gba/某游戏.gba",
            &real::NDS_GYAKUTEN_KENJI,
            1 << 27,
            Some("GBA"),
        );
        assert_eq!(facts.platform.as_deref(), Some("NDS"), "字节说了算");
    }

    #[test]
    fn 认不出来时说清楚读了多少() {
        let facts = probe("x.gba", &[0u8; 0x400], 1024, Some("GBA"));
        assert!(facts.cart.is_none());
        assert!(facts.ids.is_empty());
        let note = facts.note.expect("要有一句话");
        assert!(note.contains("GBA 卡带头"), "{note}");
    }

    #[test]
    fn 随机字节不许凑出一条_sfc_游戏码() {
        // 这一层最怕的假阳性：在任意字节上打分，打出一个自称是编号的东西。
        let noise: Vec<u8> = (0..0x1_0200u32)
            .map(|at| (at.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let facts = probe("x.sfc", &noise, 0x1_0200, Some("SFC"));
        assert!(facts.ids.is_empty(), "凑出来的编号是假依据");
    }

    #[test]
    fn ndstool_的_crc16_对得上标准校验值() {
        // 调研 A.5 的更正：按 ndstool 实现，不按 GBATEK 的伪代码。
        assert_eq!(crc16(b"123456789"), 0x4B37);
    }
}
