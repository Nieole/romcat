//! **只读几百字节就认出来**：光盘镜像里的内部标识。
//!
//! 识别一个 8 GB 的 ISO 不需要读 8 GB。每一种光盘世代的载体都把身份**明文**写在最前面：
//!
//! | 载体 | 标识在哪 | 读多少 |
//! |---|---|---|
//! | GC / Wii 的 `.iso` / `.gcm` | 逻辑偏移 0 起 6 字节 Game ID，`0x18`/`0x1C` 是两个魔数 | 0x440 |
//! | PS1 / PS2 的 ISO9660 | 根目录下 `SYSTEM.CNF` 的 `BOOT` / `BOOT2` 行 | 前几百 KB |
//! | PSP 的 UMD | 主卷描述符的 Application Used 区（`0x8373`），或 `PSP_GAME/PARAM.SFO` | 33 KB 起 |
//! | PS3 的蓝光 | `PS3_GAME/PARAM.SFO` | 前几百 KB |
//! | **PBP** | `param_sfo_offset`（通常 `0x28`）处的 `PARAM.SFO`，**未压缩** | < 4 KB |
//! | **WIA / RVZ** | 光盘头 `0x80` 字节原样躺在**文件偏移 `0x58`** | 0xD8 |
//! | **WBFS** | Wii 光盘头 256 字节在 `hd_sector_size`（通常 `0x200`） | 0x300 |
//! | **CSO / ZSO / DAX** | 头部给出原始大小与块索引，前几个块解出来就是 ISO 的开头 | 按块 |
//! | **CHD** | 头部 `+64` `rawsha1`、`+84` `sha1`；**都对不上 Redump** | 124 |
//!
//! 出处逐条记在 `docs/research/containers-and-compressed-images.md` 第 2 部分与
//! `rom-identification.md` A.11–A.16。
//!
//! ## 压缩镜像不是容器（ADR-0014）
//!
//! 这里一个也不「解包」。CSO 是解了**头几个块**——那是格式自带的随机访问，不是把它
//! 当归档展开；其余全是定长偏移上的明文读取。**压缩镜像就是变体本身的形态**，
//! 它的身份从它自己的结构里读，不从一个想象中的内部文件读。
//!
//! ## ⭐ NKit 在这里，而且在撞 CRC 之前
//!
//! Dolphin 的原话：NKit 处理过的镜像**的 CRC32 可能和好转储的相同，即使两个文件并不
//! 完全一样**。判据是**光盘逻辑偏移** `0x200` 处的四字节 `NKIT`——注意是**逻辑**偏移，
//! 所以 `.nkit.iso` 与 `.nkit.gcz` 要各自先按壳子打开再读那一处。漏掉这一步会把 NKit
//! 文件当成完好转储，而那是**静默的错**：报告上会写着「命中」。
//!
//! ## 读不到就说读不到
//!
//! 每一种壳子都可能给不出想要的那一段：CHD 与 GCZ 的负载要解块才碰得到、ZSO 用的是
//! lz4、WBFS 只交出 256 字节够不到 `0x200`。这些一律落进 [`Facts::note`] 说人话，
//! **不假装验过 NKit**（`nkit` 留 `None`），也不编一个标识出来。

use std::borrow::Cow;
use std::path::Path;

use crate::path::{extension_lower, file_name_of_key};

use super::ident::{IdKind, Ident};
use super::iso9660;
use super::sfo::Sfo;

/// 一份内容外面套着的那层壳子。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// 未压缩的光盘镜像：字节就是逻辑光盘。
    Raw,
    /// PSP 的 PBP（也是 PS1 Classics 的载体）。
    Pbp,
    /// CSO / CISO（deflate 分块）。
    Cso,
    /// ZSO（lz4 分块）。
    Zso,
    /// DAX（8 KB 定长帧）。
    Dax,
    /// MAME 的 CHD。
    Chd,
    /// WIA / RVZ。
    WiaRvz,
    /// WBFS。
    Wbfs,
    /// Dolphin 的旧压缩格式 GCZ。
    Gcz,
    /// 一张裸的 `PARAM.SFO` / `param.sfo`。目录树转储里那一份就是它。
    Sfo,
}

impl Shell {
    /// 存进**中立库**用的短码。
    ///
    /// 与 [`label`](Self::label) 分开写，理由与 `container::ContainerKind::code` 一样：
    /// 那个是给人看的，**改一次报告用词不该让已经存进库里的记录读不回来**。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Pbp => "pbp",
            Self::Cso => "cso",
            Self::Zso => "zso",
            Self::Dax => "dax",
            Self::Chd => "chd",
            Self::WiaRvz => "wia",
            Self::Wbfs => "wbfs",
            Self::Gcz => "gcz",
            Self::Sfo => "sfo",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::all().into_iter().find(|it| it.code() == code)
    }

    /// 全部十种，顺序固定。
    #[must_use]
    pub fn all() -> [Self; 10] {
        [
            Self::Raw,
            Self::Pbp,
            Self::Cso,
            Self::Zso,
            Self::Dax,
            Self::Chd,
            Self::WiaRvz,
            Self::Wbfs,
            Self::Gcz,
            Self::Sfo,
        ]
    }

    /// 报告与依据里写的那个名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Raw => "未压缩镜像",
            Self::Pbp => "PBP",
            Self::Cso => "CSO",
            Self::Zso => "ZSO",
            Self::Dax => "DAX",
            Self::Chd => "CHD",
            Self::WiaRvz => "WIA/RVZ",
            Self::Wbfs => "WBFS",
            Self::Gcz => "GCZ",
            Self::Sfo => "PARAM.SFO",
        }
    }

    /// 按魔数认出壳子；认不出时回退到扩展名。
    ///
    /// **魔数优先**：ADR-0011 那条「目录只是强先验」在这里的推论是文件名也只是先验，
    /// 一个叫 `.iso` 的 CSO 真实存在。
    #[must_use]
    pub fn detect(name: &str, head: &[u8]) -> Option<Self> {
        let magic = |m: &[u8]| head.starts_with(m);
        if magic(b"\0PBP") {
            return Some(Self::Pbp);
        }
        if magic(b"\0PSF") {
            return Some(Self::Sfo);
        }
        if magic(b"CISO") {
            return Some(Self::Cso);
        }
        if magic(b"ZISO") {
            return Some(Self::Zso);
        }
        if magic(b"DAX\0") {
            return Some(Self::Dax);
        }
        if magic(b"MComprHD") {
            return Some(Self::Chd);
        }
        if magic(b"WIA\x01") || magic(b"RVZ\x01") {
            return Some(Self::WiaRvz);
        }
        if magic(b"WBFS") {
            return Some(Self::Wbfs);
        }
        if magic(&[0x01, 0xC0, 0x0B, 0xB1]) {
            return Some(Self::Gcz);
        }
        by_name(name)
    }
}

/// 名字里写着 `.nkit.` 吗（NKit 工具自己的命名）。
#[must_use]
pub fn named_nkit(name: &str) -> bool {
    file_name_of_key(name)
        .to_ascii_lowercase()
        .contains(".nkit.")
}

/// 这份内容**有没有可能**是一张 GC / Wii 的光盘，因而必须验 NKit。
///
/// **判据只看内容自己的形态，不看它躺在哪个目录里**（ADR-0011）。NKit 只处理 GC / Wii
/// 的镜像，而那几种镜像的壳子就这几样；`.7z` 里那份 `.iso` 照样算——那个名字是内容的，
/// 不是目录的。
///
/// 它存在的理由是**顺序**：序列号是第二命中层，撞上了 CRC 的内容本来不必再探
/// （`worth_probing`）。唯独这一类不行——「NKit 检测在任何 CRC 匹配之前执行」不许拿
/// CRC 的结果来决定验不验。
#[must_use]
pub fn may_hold_nkit(name: &str) -> bool {
    named_nkit(name)
        || matches!(
            by_name(name),
            Some(Shell::Raw | Shell::WiaRvz | Shell::Wbfs | Shell::Gcz | Shell::Cso)
        )
}

/// 光靠名字猜壳子。头部还没读到手时用它决定「要不要读、读多少」。
#[must_use]
pub fn by_name(name: &str) -> Option<Shell> {
    let file = file_name_of_key(name);
    if file.eq_ignore_ascii_case("param.sfo") {
        return Some(Shell::Sfo);
    }
    match extension_lower(Path::new(file))?.as_str() {
        "iso" | "gcm" | "img" | "wud" => Some(Shell::Raw),
        "pbp" => Some(Shell::Pbp),
        "cso" | "ciso" => Some(Shell::Cso),
        "zso" => Some(Shell::Zso),
        "dax" | "jso" => Some(Shell::Dax),
        "chd" => Some(Shell::Chd),
        "rvz" | "wia" => Some(Shell::WiaRvz),
        "wbfs" => Some(Shell::Wbfs),
        "gcz" => Some(Shell::Gcz),
        "sfo" => Some(Shell::Sfo),
        _ => None,
    }
}

/// 一份内容探出来的全部事实。
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    /// 认出来的壳子，存的是 [`Shell::code`] 那个**短码**；认不出是 `None`。
    pub shell: Option<String>,
    /// 验过 NKit 没有、结论是什么。`None` = **没验过**，与 `Some(false)` 是两回事。
    pub nkit: Option<bool>,
    /// 读出来的标识，按可信程度排好。
    pub ids: Vec<Ident>,
    /// 读不到想要的那一段时，说人话的一句。
    pub note: Option<String>,
}

/// 探一份内容要读多少字节。
///
/// 分两档而不是一个数：**GC / Wii 与几种压缩镜像的身份全在头部**，为它们读一兆是白读；
/// 而 ISO9660 那几家要走目录树，`SYSTEM.CNF` 与 `PSP_GAME/PARAM.SFO` 落在盘的前几百 KB。
/// 真库里 `.iso` 有 1,755 份、合计 2,566 GiB——这两档之差就是「读 1.7 GiB」与
/// 「读 27 TiB」之差。
#[must_use]
pub fn probe_len(shell: Shell) -> usize {
    match shell {
        // 头部就够：CHD 124 字节、WIA 0xD8、GCZ 头。
        Shell::Chd | Shell::WiaRvz | Shell::Gcz => 0x400,
        // WBFS 的光盘头从 `1 << hd_sector_shift` 起。真库里那个位移恒为 9（512 字节），
        // 但**它是文件自己写的**——只按 512 读，位移大一点就取不到光盘头，而那会表现成
        // 「这份 WBFS 认不出来」而不是「读少了」。按位移 13（8 KiB）留够。
        Shell::Wbfs => (1 << 13) + 0x100,
        // SFO 最大的一份是 Vita 的，实测 1,664 字节；`PUBTOOLINFO` 一个键就 0x200。
        Shell::Sfo => 0x4000,
        // PBP 的 `PARAM.SFO` 紧跟在 0x28 的头后面，`icon0_png_offset` 就是它的终点。
        Shell::Pbp => 0x8000,
        // 要走 ISO9660 的目录树。ISO 的主卷描述符在 0x8000，根目录通常在扇区 20 上下，
        // 而 `SYSTEM.CNF` / `PSP_GAME/` 是最先写进盘的几个文件。
        Shell::Raw | Shell::Cso | Shell::Zso | Shell::Dax => 1 << 20,
    }
}

/// 探一份内容。`head` 是它的前若干字节（长度见 [`probe_len`]），`total` 是总长度。
#[must_use]
pub fn probe(name: &str, head: &[u8], total: u64) -> Facts {
    let Some(shell) = Shell::detect(name, head) else {
        return Facts::default();
    };
    let mut facts = Facts {
        shell: Some(shell.code().to_string()),
        ..Facts::default()
    };
    match shell {
        Shell::Sfo => {
            read_sfo(&mut facts, head, "这份 param.sfo");
            return facts;
        }
        Shell::Pbp => {
            read_pbp(&mut facts, head);
            return facts;
        }
        Shell::Chd => {
            read_chd(&mut facts, head);
            return facts;
        }
        _ => {}
    }
    let logical: Cow<'_, [u8]> = match shell {
        Shell::Raw => Cow::Borrowed(head),
        Shell::Cso => match cso_prefix(head, total) {
            Ok(bytes) => Cow::Owned(bytes),
            Err(why) => {
                facts.note = Some(why);
                return facts;
            }
        },
        Shell::Zso => {
            facts.note = Some(
                "ZSO 认不了：块是 lz4 压的，本程序还解不了——标识读不出来，NKit 也验不了"
                    .to_string(),
            );
            return facts;
        }
        Shell::Dax => {
            facts.note = Some("DAX 认不了：帧还解不了——标识读不出来，NKit 也验不了".to_string());
            return facts;
        }
        // WIA / RVZ 把光盘头 0x80 字节**原样**放在文件偏移 0x58（Dolphin
        // `docs/WiaAndRvz.md`）。0x80 够读 Game ID 与两个魔数，够不到 0x200 的 NKit 标记。
        Shell::WiaRvz => Cow::Borrowed(head.get(0x58..0x58 + 0x80).unwrap_or_default()),
        // WBFS 在 `hd_sector_size` 处放 Wii 光盘头的 256 字节（Dolphin `WbfsBlob.cpp`）。
        Shell::Wbfs => match wbfs_head(head) {
            Some(bytes) => Cow::Borrowed(bytes),
            None => {
                facts.note = Some("WBFS 认不了：头不全，取不到里面那份 Wii 光盘头".to_string());
                return facts;
            }
        },
        Shell::Gcz => {
            facts.note = Some(
                "GCZ 认不了：块还解不了——标识读不出来，NKit 也验不了（Dolphin 没给这个旧格式的规范）"
                    .to_string(),
            );
            return facts;
        }
        Shell::Chd | Shell::Pbp | Shell::Sfo => unreachable!("上面已经 return"),
    };
    read_logical(&mut facts, &logical, name, shell);
    facts
}

/// 逻辑光盘的开头到手之后，能读出来的全部东西。
fn read_logical(facts: &mut Facts, image: &[u8], name: &str, shell: Shell) {
    // ⭐ **NKit 的判据是字节，不是名字。** 票据原话：判据为光盘逻辑偏移处的标记
    //（Dolphin `VolumeDisc::IsNKit()` 读逻辑偏移 `0x200` 处的 `NKIT`）。够不到那一处
    // 就留 `None`——「没验过」与「验过、不是」是两回事，混起来会让一份没验过的
    // GC 镜像自动通过。
    if image.len() >= 0x204 {
        facts.nkit = Some(image[0x200..0x204] == *b"NKIT");
    }
    // 名字里写着 `.nkit.` 只**补充**这条结论，绝不代替它：ADR-0011 那条「目录只是强先验」
    // 对文件名同样成立。它只会把 `None` / `Some(false)` 抬成 `Some(true)`，也就是只会让
    // 一份镜像**更难**自动通过——反过来（名字没写就当好转储）才是危险的那个方向。
    if named_nkit(name) && facts.nkit != Some(true) {
        let seen = match image.len() >= 0x204 {
            true => "在逻辑偏移 0x200 处没看到 NKIT",
            false => "够不到逻辑偏移 0x200",
        };
        facts.nkit = Some(true);
        facts.note = Some(format!(
            "名字里写着 `.nkit.`：字节这一头{seen}，按名字算它是 NKit"
        ));
        return;
    }
    if facts.nkit.is_none() && facts.note.is_none() {
        facts.note = Some(format!(
            "NKit 验不了：{} 这一层只交得出前 {} 字节，够不到光盘逻辑偏移 0x200",
            shell.label(),
            image.len()
        ));
    }
    if let Some(id) = gamecube_or_wii(image) {
        facts.ids.push(id);
        return;
    }
    if !iso9660::is_iso9660(image) {
        if facts.note.is_none() && facts.ids.is_empty() {
            facts.note = Some(format!(
                "认不出光盘结构：{} 读到了，但开头既不是 GC / Wii 光盘头也不是 ISO9660",
                shell.label()
            ));
        }
        return;
    }
    if let iso9660::Found::At(range) = iso9660::find(image, &["SYSTEM.CNF"])
        && let Some(id) = system_cnf(&image[range])
    {
        facts.ids.push(id);
    }
    for (path, label) in [
        (["PSP_GAME", "PARAM.SFO"], "盘内 PSP_GAME/PARAM.SFO"),
        (["PS3_GAME", "PARAM.SFO"], "盘内 PS3_GAME/PARAM.SFO"),
    ] {
        match iso9660::find(image, &path) {
            iso9660::Found::At(range) => read_sfo(facts, &image[range], label),
            iso9660::Found::TooFar { at } => {
                facts.note = Some(format!(
                    "读得不够远：{label} 在 0x{at:X}，超出这一趟读进来的 {} 字节",
                    image.len()
                ));
            }
            iso9660::Found::Missing => {}
        }
    }
    // PSP 的**免解析通路**：主卷描述符的 Application Used 区里写着 `ULUS-10339|…`，
    // 在 0x8373，比走目录树便宜一个数量级。
    //
    // **它排在最后，而且同一串编号已经有了就不再落一条。** 它只给得出编号；
    // `PSP_GAME/PARAM.SFO` 那条还带着标题与版本，而下游按顺序取第一条。
    // 它真正的用处是那份 SFO 落在读取上限之外时顶上。
    if let Some(id) = psp_application_use(image)
        && !facts.ids.iter().any(|seen| seen.key == id.key)
    {
        facts.ids.push(id);
    }
    if facts.ids.is_empty() && facts.note.is_none() {
        facts.note = Some("ISO9660 里没有认得出来的启动配置或参数文件".to_string());
    }
}

/// GC / Wii 的光盘头。两个魔数各自只在一边出现（WiiBrew：另一边为零）。
fn gamecube_or_wii(image: &[u8]) -> Option<Ident> {
    let magic_at = |at: usize, m: [u8; 4]| image.get(at..at + 4) == Some(&m[..]);
    let which = if magic_at(0x18, [0x5D, 0x1C, 0x9E, 0xA3]) {
        "Wii"
    } else if magic_at(0x1C, [0xC2, 0x33, 0x9F, 0x3D]) {
        "GameCube"
    } else {
        return None;
    };
    // Redump 的「Internal Serial」是 4 字符 game ID 加 2 字符 publisher ID
    // （A.16.4：即 Dolphin 里显示的 Game ID）。
    let raw = image.get(..6)?;
    if !raw.iter().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let shown = String::from_utf8_lossy(raw).into_owned();
    Some(Ident {
        key: crate::dat::serial::normalize(&shown)?,
        shown,
        kind: IdKind::DiscId,
        from: format!("{which} 光盘头（逻辑偏移 0 起 6 字节）"),
        title: ascii_text(image.get(0x20..0x60).unwrap_or_default()),
        version: image.get(7).map(|byte| format!("v{byte}")),
    })
}

/// PSP 的 UMD 把序列号明文写在主卷描述符的 Application Used 区里。
fn psp_application_use(image: &[u8]) -> Option<Ident> {
    let region = iso9660::application_use(image)?;
    // 形如 `ULUS-10339|D8E7…`：竖线前那一段就是序列号。
    //
    // **形状要卡死。** 那 512 字节在别的盘上装的是什么谁也说不准，只要求「够长、
    // 首字母大写」的话，一段随机字节就能变成一条自称高置信的序列号。调研 A.13.3 给的
    // 形状是四个字母、连字符、五位数字。
    let text = String::from_utf8_lossy(region);
    let head = text.split('|').next()?.trim();
    let chars: Vec<char> = head.chars().collect();
    if chars.len() != 10
        || !chars[..4].iter().all(char::is_ascii_uppercase)
        || chars[4] != '-'
        || !chars[5..].iter().all(char::is_ascii_digit)
    {
        return None;
    }
    Some(Ident {
        key: crate::dat::serial::normalize(head)?,
        shown: head.to_string(),
        kind: IdKind::Serial,
        from: "主卷描述符的 Application Used 区（0x8373，明文）".to_string(),
        title: None,
        version: None,
    })
}

/// `SYSTEM.CNF` 的 `BOOT` / `BOOT2` 行。
///
/// 归一化照 DuckStation 的 `System::GetGameCodeForPath`（PS1）与 PCSX2 的
/// `ExecutablePathToSerial`（PS2）：取最后一段路径、在第一个 `;` 处截断、删掉所有 `.`、
/// `_` 换成 `-`、其余大写。`SCES_123.45` → `SCES-12345`。
fn system_cnf(bytes: &[u8]) -> Option<Ident> {
    let text = String::from_utf8_lossy(bytes);
    for line in text.lines() {
        // 没有 `=` 的行跳过而不是收工：`SYSTEM.CNF` 里空行与注释都真实存在，
        // 在这儿 `?` 一下就会把后面那条 `BOOT` 行整个丢掉。
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().to_ascii_uppercase();
        if key != "BOOT" && key != "BOOT2" {
            continue;
        }
        // 引导参数跟在文件名后面，空格切掉。
        let value = value.split_whitespace().next().unwrap_or("");
        let tail = value
            .rsplit(['\\', '/', ':'])
            .next()
            .unwrap_or(value)
            .trim();
        let tail = tail.split(';').next().unwrap_or(tail);
        if tail.is_empty() {
            continue;
        }
        // **`BOOT2` 要过格式校验，`BOOT` 不要。** 两个模拟器在这一点上是分开的，
        // 调研 A.12.2 专门标了「⚠ 与 DuckStation 的关键差异」：PCSX2 的
        // `ExecutablePathToSerial` 要求匹配 `????_???.??*` 或 `????-???.??*`，
        // 否则**把序列号清空**；DuckStation 一概不校验（`SCUS_946.06` 放在
        // `EXE\` 子目录里那种也照收）。照抄两边，别自己发明第三套。
        if key == "BOOT2" && !ps2_shaped(tail) {
            continue;
        }
        let shown: String = tail
            .chars()
            .filter(|c| *c != '.')
            .map(|c| {
                if c == '_' {
                    '-'
                } else {
                    c.to_ascii_uppercase()
                }
            })
            .collect();
        return Some(Ident {
            key: crate::dat::serial::normalize(&shown)?,
            shown,
            kind: IdKind::Serial,
            from: format!("盘内 SYSTEM.CNF 的 {key} 行"),
            title: None,
            version: None,
        });
    }
    None
}

/// PCSX2 那道格式校验：`????_???.??*` 或 `????-???.??*`。
fn ps2_shaped(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 11 {
        return false;
    }
    chars[..4].iter().all(char::is_ascii_alphanumeric)
        && (chars[4] == '_' || chars[4] == '-')
        && chars[5..8].iter().all(char::is_ascii_digit)
        && chars[8] == '.'
        && chars[9..11].iter().all(char::is_ascii_digit)
}

/// 一张 PARAM.SFO / param.sfo 里的标识。
fn read_sfo(facts: &mut Facts, bytes: &[u8], from: &str) {
    let Some(sfo) = Sfo::parse(bytes) else {
        if facts.note.is_none() {
            facts.note = Some(format!("PARAM.SFO 读不成：{from}"));
        }
        return;
    };
    // 标题优先取默认那一条。本地化标题（`TITLE_xx`）不在这里挑——那是刮削与
    // **标题集合**的事（票 15），这一层只要一个够用的名字进依据。
    let title = sfo.text("TITLE").map(ToString::to_string);
    let version = sfo
        .first_text(&["APP_VER", "DISC_VERSION", "VERSION"])
        .map(ToString::to_string);
    // **CONTENT_ID 比 TitleID 细一档**：本体、更新与 DLC 的 TitleID 相同而 CONTENT_ID
    // 不同。两条都产出，排序上 CONTENT_ID 在前。
    if let Some(text) = sfo.text("CONTENT_ID")
        && let Some(key) = crate::dat::serial::normalize(text)
    {
        facts.ids.push(Ident {
            key,
            shown: text.to_string(),
            kind: IdKind::ContentId,
            from: format!("{from} 的 CONTENT_ID"),
            title: title.clone(),
            version: version.clone(),
        });
    }
    // 键名随代机而变：PSP 叫 `DISC_ID`，PSV 与 PS3 叫 `TITLE_ID`。
    for (name, kind) in [("DISC_ID", IdKind::Serial), ("TITLE_ID", IdKind::TitleId)] {
        if let Some(text) = sfo.text(name)
            && let Some(key) = crate::dat::serial::normalize(text)
        {
            facts.ids.push(Ident {
                key,
                shown: text.to_string(),
                kind,
                from: format!("{from} 的 {name}"),
                title: title.clone(),
                version: version.clone(),
            });
        }
    }
}

/// PBP：`+0x08` 是 `param_sfo_offset`（通常 `0x28`），那儿的 `PARAM.SFO` **没被压缩**。
fn read_pbp(facts: &mut Facts, head: &[u8]) {
    let Some(raw) = head.get(8..12) else {
        facts.note = Some("PBP 读不了：头不全".to_string());
        return;
    };
    let at = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    let Some(bytes) = head.get(at..) else {
        facts.note = Some(format!(
            "PBP 读不了：它说 PARAM.SFO 在 0x{at:X}，超出这一趟读进来的 {} 字节",
            head.len()
        ));
        return;
    };
    read_sfo(facts, bytes, &format!("PBP 内 0x{at:X} 处的 PARAM.SFO"));
}

/// CHD：头部那两个 SHA-1 **一个都不许拿去撞 DAT**。
///
/// `chdman info` 的 `SHA1:`（V5 头 `+84`，raw+meta 组合）与 `Data SHA1:`（`+64`，
/// rawsha1）**都对不上 Redump**——CHD 的 raw stream 是整张盘按 CD 帧连续拼接并按 hunk
/// 补齐的单一流，Redump 是逐轨分开的 `.bin`，粒度与扇区布局双重不同（ADR-0014 的修订段）。
/// 所以这里只如实记下「这是一个 CHD、里面那条逻辑流有多长」，**不产出标识**。
fn read_chd(facts: &mut Facts, head: &[u8]) {
    let version = head
        .get(12..16)
        .map(|raw| u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]));
    let logical = head
        .get(32..40)
        .map(|raw| u64::from_be_bytes(raw.try_into().unwrap_or_default()));
    facts.note = Some(format!(
        "CHD 认不了：头里那两个 SHA-1 与 Redump 的粒度和扇区布局都不同，撞不了 DAT；\
         盘内的启动配置要解 hunk 才读得到，这一层还不做（v{}，逻辑流 {} 字节）",
        version.unwrap_or(0),
        logical.unwrap_or(0),
    ));
}

/// WBFS：`hd_sector_shift` 在 `+0x08`，Wii 光盘头的 256 字节从 `1 << shift` 起。
fn wbfs_head(head: &[u8]) -> Option<&[u8]> {
    let shift = u32::from(*head.get(8)?);
    // 位移是文件自己写的。真库里它恒为 9（512 字节），但一个坏文件写 200 也不该
    // 让这里算出个天文数字再去切片。
    if shift > 24 {
        return None;
    }
    let at = 1usize << shift;
    head.get(at..at + 256)
}

/// CSO / CISO：头部给出原始大小与块索引，前几个块解出来就是 ISO 的开头。
///
/// 索引项的低 31 位左移 `index_shift` 是该块压缩数据在文件里的偏移，长度是与下一项
/// 之差；**最高位置 1 表示这一块没压缩**（maxcso `README_CSO.md` 逐字）。块用的是
/// **raw deflate**，窗口 15。
///
/// v2 不认：它把最高位改成了「lz4 还是 deflate」，而 lz4 本程序还没有——PPSSPP 干脆
/// 也不支持（源码 `hdr.ver > 1` 直接报错）。
fn cso_prefix(head: &[u8], total: u64) -> Result<Vec<u8>, String> {
    let le32 = |at: usize| -> Option<u32> {
        head.get(at..at + 4)
            .map(|raw| u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
    };
    let uncompressed = head
        .get(8..16)
        .map(|raw| u64::from_le_bytes(raw.try_into().unwrap_or_default()))
        .ok_or_else(|| "CSO 头读不全".to_string())?;
    let block = le32(16).unwrap_or(0) as usize;
    let version = *head.get(20).unwrap_or(&0);
    let shift = u32::from(*head.get(21).unwrap_or(&0));
    if version > 1 {
        return Err(format!(
            "CSO 读不了：v{version} 的索引语义与 v1 不同（高位表示 lz4），本程序还读不了；PPSSPP 同样不支持"
        ));
    }
    if block == 0 || block > 1 << 20 || shift > 24 {
        return Err(format!(
            "CSO 读不了：头里的块大小 {block} 或对齐 {shift} 说不通"
        ));
    }
    let want = probe_len(Shell::Cso).min(usize::try_from(uncompressed).unwrap_or(usize::MAX));
    let blocks = want.div_ceil(block);
    let mut out = Vec::with_capacity(want);
    for index in 0..blocks {
        let at = 0x18 + index * 4;
        let (Some(entry), Some(next)) = (le32(at), le32(at + 4)) else {
            return Err(format!(
                "CSO 读不了：索引只读进来 {} 项，不够解出前 {want} 字节",
                head.len().saturating_sub(0x18) / 4
            ));
        };
        let plain = entry & 0x8000_0000 != 0;
        let from = usize::try_from(u64::from(entry & 0x7FFF_FFFF) << shift).unwrap_or(usize::MAX);
        let to = usize::try_from(u64::from(next & 0x7FFF_FFFF) << shift).unwrap_or(usize::MAX);
        let Some(data) = head.get(from..to.max(from)) else {
            return Err(format!(
                "CSO 读不了：第 {index} 块在 0x{from:X}，超出这一趟读进来的 {} 字节（原始镜像 {total} 字节）",
                head.len()
            ));
        };
        if plain {
            out.extend_from_slice(data.get(..block.min(data.len())).unwrap_or_default());
            continue;
        }
        let mut sink = Vec::new();
        let mut decoder = flate2::bufread::DeflateDecoder::new(data);
        std::io::Read::read_to_end(&mut decoder, &mut sink)
            .map_err(|error| format!("CSO 读不了：第 {index} 块解不开（{error}）"))?;
        out.extend_from_slice(&sink);
    }
    Ok(out)
}

/// 一段字节里那串可打印的 ASCII，去掉首尾空白；一个字都没有时是 `None`。
fn ascii_text(bytes: &[u8]) -> Option<String> {
    let text: String = bytes
        .iter()
        .take_while(|b| **b != 0)
        .filter(|b| (0x20..0x7F).contains(*b))
        .map(|b| char::from(*b))
        .collect();
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 空盘() -> Vec<u8> {
        vec![0u8; 1 << 16]
    }

    #[test]
    fn 壳子按魔数认而不是按扩展名() {
        // 一个叫 `.iso` 的 CSO 真实存在——名字只是先验（ADR-0011 的推论）。
        assert_eq!(Shell::detect("x.iso", b"CISO\0"), Some(Shell::Cso));
        assert_eq!(Shell::detect("x.bin", b"MComprHD"), Some(Shell::Chd));
        assert_eq!(Shell::detect("x.iso", b"\0\0\0\0"), Some(Shell::Raw));
        assert_eq!(Shell::detect("说明.txt", b"\0\0\0\0"), None);
        assert_eq!(Shell::detect("a/sce_sys/param.sfo", b""), Some(Shell::Sfo));
    }

    #[test]
    fn nkit_在逻辑偏移_0x200_上认出来并且盖在标识之前() {
        let mut image = 空盘();
        image[0x18..0x1C].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
        image[..6].copy_from_slice(b"RSBE01");
        image[0x200..0x204].copy_from_slice(b"NKIT");
        let facts = probe("WII/游戏.iso", &image, image.len() as u64);
        assert_eq!(facts.nkit, Some(true));
        assert_eq!(facts.ids[0].shown, "RSBE01");
    }

    #[test]
    fn 名字里写着_nkit_的只补充不代替字节判据() {
        // 字节这一头验过了、说不是，名字仍然把它抬成 NKit——**只往难通过的方向抬**。
        let facts = probe("WII/游戏.nkit.iso", &空盘(), 1 << 16);
        assert_eq!(facts.nkit, Some(true));
        assert!(
            facts
                .note
                .as_deref()
                .is_some_and(|s| s.contains("名字里写着"))
        );
        // 反过来不成立：名字没写而字节里有标记的，照样验得出来。
        let mut image = 空盘();
        image[0x200..0x204].copy_from_slice(b"NKIT");
        assert_eq!(probe("WII/游戏.iso", &image, 1 << 16).nkit, Some(true));
    }

    #[test]
    fn 没验过与验过不是一回事() {
        // WIA 只交得出 0x80 字节，够不到 0x200。这时 `nkit` 必须是 None——
        // 记成 Some(false) 会让一份没验过的镜像自动通过。
        let mut file = vec![0u8; 0x400];
        file[..4].copy_from_slice(b"RVZ\x01");
        file[0x58..0x58 + 4].copy_from_slice(b"GALE");
        file[0x58 + 4..0x58 + 6].copy_from_slice(b"01");
        file[0x58 + 0x1C..0x58 + 0x20].copy_from_slice(&[0xC2, 0x33, 0x9F, 0x3D]);
        let facts = probe("NGC/游戏.rvz", &file, file.len() as u64);
        assert_eq!(facts.nkit, None, "够不到 0x200 就是没验过");
        assert!(facts.note.as_deref().is_some_and(|s| s.contains("0x200")));
        assert_eq!(facts.ids[0].shown, "GALE01");
        assert_eq!(facts.ids[0].kind, IdKind::DiscId);
    }

    #[test]
    fn wbfs_的光盘头在_hd_sector_size_处() {
        let mut file = vec![0u8; 0x400];
        file[..4].copy_from_slice(b"WBFS");
        file[8] = 9; // hd_sector_shift → 512
        file[0x200..0x206].copy_from_slice(b"RMCE01");
        file[0x200 + 0x18..0x200 + 0x1C].copy_from_slice(&[0x5D, 0x1C, 0x9E, 0xA3]);
        let facts = probe("WII/游戏.wbfs", &file, file.len() as u64);
        assert_eq!(facts.ids[0].shown, "RMCE01");
        assert_eq!(facts.ids[0].key, "RMCE01");
        assert_eq!(facts.nkit, None, "只交得出 256 字节，验不了");
    }

    #[test]
    fn ps1_的_system_cnf_读出序列号并归一化() {
        let got = system_cnf(b"BOOT = cdrom:\\SLPS_021.70;1\r\nTCB = 4\r\n").expect("读得出");
        assert_eq!(got.shown, "SLPS-02170");
        assert_eq!(got.key, "SLPS02170");
        assert_eq!(got.kind, IdKind::Serial);
        assert!(got.from.contains("BOOT"));
    }

    #[test]
    fn ps2_的_boot2_与引导参数都处理得了() {
        let got =
            system_cnf(b"BOOT2 = cdrom0:\\SLPS_256.04;1 arg\r\nVER = 1.00\r\n").expect("读得出");
        assert_eq!(got.shown, "SLPS-25604");
        // 版本后缀 `;2` 也要在第一个分号处截断（PCSX2 的注释说 BIOS 忽略它）。
        assert_eq!(
            system_cnf(b"BOOT2 = cdrom0:\\SLUS_213.86;2\r\n")
                .expect("读得出")
                .shown,
            "SLUS-21386"
        );
    }

    #[test]
    fn ps2_的_boot2_过格式校验而_ps1_的_boot_不过() {
        // 调研 A.12.2 标了「⚠ 与 DuckStation 的关键差异」：PCSX2 校验、DuckStation 不校验。
        assert!(
            system_cnf(b"BOOT2 = cdrom0:\\SYSTEM.ELF;1\r\n").is_none(),
            "形状不对，清空"
        );
        // 同一串在 PS1 的 `BOOT` 行上照收——DuckStation 一概不校验。
        assert_eq!(
            system_cnf(b"BOOT = cdrom:\\SYSTEM.ELF;1\r\n")
                .expect("PS1 那边不校验")
                .shown,
            "SYSTEMELF"
        );
        // Wild Arms 那种放在子目录里的 PS1 引导文件照样读得出来。
        assert_eq!(
            system_cnf(b"BOOT = cdrom:\\EXE\\SCUS_946.06;1\r\n")
                .expect("读得出")
                .shown,
            "SCUS-94606"
        );
    }

    #[test]
    fn psp_的免解析通路只认那个形状() {
        // 那 512 字节在别的盘上装的是什么谁也说不准，形状不卡死就会凭空造出序列号。
        let mut image = vec![0u8; 64 * iso9660::SECTOR];
        image[iso9660::PVD_OFFSET] = 0x01;
        image[iso9660::PVD_OFFSET + 1..iso9660::PVD_OFFSET + 6].copy_from_slice(b"CD001");
        let at = iso9660::PVD_OFFSET + iso9660::APPLICATION_USE;
        image[at..at + 20].copy_from_slice(b"SOME RANDOM RUBBISH ");
        assert!(psp_application_use(&image).is_none(), "不是那个形状");
    }

    #[test]
    fn chd_只记下它是什么绝不产出能撞_dat_的标识() {
        let mut file = vec![0u8; 0x400];
        file[..8].copy_from_slice(b"MComprHD");
        file[12..16].copy_from_slice(&5u32.to_be_bytes());
        file[32..40].copy_from_slice(&700_000_000u64.to_be_bytes());
        let facts = probe("PS1/游戏.chd", &file, file.len() as u64);
        assert!(facts.ids.is_empty(), "两个 SHA-1 都对不上 Redump");
        assert!(
            facts
                .note
                .as_deref()
                .is_some_and(|s| s.contains("CHD 认不了"))
        );
        assert!(facts.note.as_deref().is_some_and(|s| s.contains("v5")));
    }

    #[test]
    fn pbp_的_param_sfo_在固定偏移处且没被压缩() {
        let mut file = vec![0u8; 0x400];
        file[..4].copy_from_slice(b"\0PBP");
        file[8..12].copy_from_slice(&0x28u32.to_le_bytes());
        let sfo = 造一张sfo("DISC_ID", "ULJM05800");
        file[0x28..0x28 + sfo.len()].copy_from_slice(&sfo);
        let facts = probe("PSP/EBOOT.PBP", &file, file.len() as u64);
        assert_eq!(facts.ids[0].shown, "ULJM05800");
        assert_eq!(facts.ids[0].kind, IdKind::Serial);
    }

    #[test]
    fn psp_的免解析通路读得到序列号() {
        let mut image = vec![0u8; 64 * iso9660::SECTOR];
        image[iso9660::PVD_OFFSET] = 0x01;
        image[iso9660::PVD_OFFSET + 1..iso9660::PVD_OFFSET + 6].copy_from_slice(b"CD001");
        let at = iso9660::PVD_OFFSET + iso9660::APPLICATION_USE;
        image[at..at + 21].copy_from_slice(b"ULUS-10339|D8E7B2C1A0");
        let facts = probe("psp/游戏.iso", &image, image.len() as u64);
        assert_eq!(facts.ids[0].shown, "ULUS-10339");
        assert_eq!(facts.ids[0].key, "ULUS10339");
    }

    #[test]
    fn 认不出来时说人话而不是编一个标识() {
        let facts = probe("PSP/游戏.zso", b"ZISO\0\0\0\0", 1024);
        assert!(facts.ids.is_empty());
        assert!(facts.note.as_deref().is_some_and(|s| s.contains("lz4")));
        assert_eq!(facts.nkit, None);

        let facts = probe("x/游戏.iso", &空盘(), 1 << 16);
        assert!(facts.ids.is_empty());
        assert!(facts.note.is_some());
        assert_eq!(facts.nkit, Some(false), "裸镜像够得到 0x200，验过了");
    }

    #[test]
    fn cso_的前几块解得出来() {
        // 造一份 v1 CSO：两个块，第一个压过、第二个原样。
        let block = 2048usize;
        let mut plain = vec![0u8; block * 2];
        plain[..4].copy_from_slice(b"HEAD");
        plain[block] = 0xAB;
        let mut squeezed = Vec::new();
        {
            use std::io::Write as _;
            let mut encoder =
                flate2::write::DeflateEncoder::new(&mut squeezed, flate2::Compression::fast());
            encoder.write_all(&plain[..block]).expect("压得动");
            encoder.finish().expect("收尾");
        }
        let index_at = 0x18;
        let data_at = index_at + 3 * 4;
        let mut file = vec![0u8; data_at];
        file[..4].copy_from_slice(b"CISO");
        file[4..8].copy_from_slice(&0x18u32.to_le_bytes());
        file[8..16].copy_from_slice(&((block * 2) as u64).to_le_bytes());
        file[16..20].copy_from_slice(&u32::try_from(block).expect("小").to_le_bytes());
        file[20] = 1;
        file[21] = 0;
        let first = u32::try_from(data_at).expect("小");
        let second = first + u32::try_from(squeezed.len()).expect("小");
        let third = second + u32::try_from(block).expect("小");
        file[index_at..index_at + 4].copy_from_slice(&first.to_le_bytes());
        file[index_at + 4..index_at + 8].copy_from_slice(&(second | 0x8000_0000).to_le_bytes());
        file[index_at + 8..index_at + 12].copy_from_slice(&third.to_le_bytes());
        file.extend_from_slice(&squeezed);
        file.extend_from_slice(&plain[block..]);
        let got = cso_prefix(&file, (block * 2) as u64).expect("解得出");
        assert_eq!(&got[..4], b"HEAD");
        assert_eq!(got[block], 0xAB);
    }

    #[test]
    fn cso_v2_明说读不了而不是解出一堆乱码() {
        let mut file = vec![0u8; 0x40];
        file[..4].copy_from_slice(b"CISO");
        file[16..20].copy_from_slice(&2048u32.to_le_bytes());
        file[20] = 2;
        let facts = probe("PSP/游戏.cso", &file, 1 << 20);
        assert!(facts.note.as_deref().is_some_and(|s| s.contains("v2")));
        assert!(facts.ids.is_empty());
    }

    fn 造一张sfo(key: &str, value: &str) -> Vec<u8> {
        let key_start = 0x14 + 0x10;
        let data_start = key_start + key.len() + 1;
        let mut out = Vec::new();
        out.extend_from_slice(b"\0PSF");
        out.extend_from_slice(&[0x01, 0x01, 0x00, 0x00]);
        out.extend_from_slice(&u32::try_from(key_start).expect("小").to_le_bytes());
        out.extend_from_slice(&u32::try_from(data_start).expect("小").to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0x0204u16.to_le_bytes());
        let len = u32::try_from(value.len() + 1).expect("小");
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(key.as_bytes());
        out.push(0);
        out.extend_from_slice(value.as_bytes());
        out.push(0);
        out
    }
}
