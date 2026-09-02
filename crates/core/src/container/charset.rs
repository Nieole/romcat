//! **容器内部文件名的编码探测**：不是 UTF-8 的那些，先探是哪一种，再解码。
//!
//! ## 为什么要探
//!
//! zip 只有在通用标志位第 11 位置位时才保证名字是 UTF-8，其余是「本地代码页」——
//! 而这个库是中文用户十几年攒出来的，真库实测 **21,901 条内部名字不是合法 UTF-8**。
//!
//! 票 03 当时的处置是**有损转换**（`String::from_utf8_lossy`），理由写得很清楚：
//! **识别靠 CRC-32，名字不参与命中**。那句话在票 03 成立。
//!
//! **到票 11 就不成立了**：文件名规则那一层要拿名字去撞中文离线数据源，而有损转换
//! 是不可逆的——每个坏字节变成一个 `U+FFFD`，原来那个字再也回不来。所以名字要在
//! **读容器的时候**就解对，而不是事后补救。
//!
//! ## 三种编码的字节分布相近，猜错等于把一种乱码换成另一种
//!
//! GBK、Big5、Shift_JIS 的双字节区间大面积重叠，光看「解得出来吗」是分不开的：
//! 一串 GBK 的汉字用 Big5 解出来照样是汉字，只不过是**一串没人这么用的生僻字**
//! （`上海大亨` 用 Big5 解出来是 `奻漆湮箋`）。
//!
//! 所以判据不是「解不解得出」，而是**这串字节落在哪一区**——每种编码都把最常用的那批字
//! 放在自己的一个特定区间里：
//!
//! | 编码 | 常用区（首字节） | 这一区意味着 |
//! |---|---|---|
//! | GBK | `B0`–`F7` 且次字节 ≥ `A1` | GB2312 的一二级汉字，日常中文几乎全在这儿 |
//! | Big5 | `A4`–`C6` | 常用字；`C9`–`F9` 是次常用字 |
//! | Shift_JIS | `81`–`9F` / `E0`–`EF` | 假名与汉字 |
//!
//! 再加一条**反向**判据：Shift_JIS 把 `A1`–`DF` 当**半角片假名**（单字节）。一串 GBK
//! 汉字在 Shift_JIS 眼里正是一串半角片假名（`ﾊ･ｶｷﾊｿ`）——而真实文件名里几乎没人用
//! 半角片假名。所以那一档**倒扣分**，这一条比什么都灵。
//!
//! ## 平局归 GBK
//!
//! 分数一样时选 GBK。这不是随便定的：票 03 实测这个库里那批非 UTF-8 的名字**多半是
//! GBK**，而按「哪种更可能」定平局的方向，是这类探测唯一诚实的做法。

use encoding_rs::{BIG5, Encoding, GBK, SHIFT_JIS};

/// 一条名字是按哪种编码解出来的。
///
/// **它没有 `label()`**，与这个库里别的枚举不一样：眼下没有一处报告要印这几个词
/// ——[`decode_path`] 把结论压成一个「解不解得出来」的布尔，而重解那一趟
/// （`scan::names::recheck`）报的是**改前改后的名字本身**，那比一列「GBK ×20,020」
/// 有用得多：三种编码字节分布相近，猜错会把一种乱码换成另一种，而一列数字看不出
/// 猜没猜对，一眼扫过 `ð�յ�2 → 冒险岛2` 看得出。真要印那几个词时再加。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Charset {
    /// 合法 UTF-8，没有猜。
    Utf8,
    /// 探出来是 GBK（含 GB2312 与 GB18030 的双字节部分）。
    Gbk,
    /// 探出来是 Big5。
    Big5,
    /// 探出来是 Shift_JIS。
    ShiftJis,
    /// **三种都解不出来**：有损转换，坏字节变成 `U+FFFD`。这一档的名字**不许参与匹配**。
    Lossy,
}

/// 探一条**内部路径**的编码，解码，并折成中立库要的形状。
///
/// 三种格式的名字解码是同一件事，所以只写一处（zip 与 zst 都调它）：探编码、
/// 把 `\` 换成 `/`（规范要求用 `/`，但确实有工具写 `\`；键的分隔符统一成 `/`，
/// 与 ADR-0020 同一条）、规范化成 NFC。
///
/// 第二个返回值是**「连编码都探不出来」**，不是「不是 UTF-8」——含义见
/// [`InnerEntry::name_lossy`](super::InnerEntry::name_lossy)。
#[must_use]
pub fn decode_path(raw: &[u8]) -> (String, bool) {
    let (text, charset) = decode(raw);
    (
        crate::path::nfc(&text.replace('\\', "/")).into_owned(),
        charset == Charset::Lossy,
    )
}

/// 探一条名字的编码并解码。
///
/// 先按 UTF-8 严格验一遍——**验字节比信标志位稳**：库里大量中文名是 GBK 却没置
/// 第 11 位，反过来也有置了位却不是 UTF-8 的。
#[must_use]
pub fn decode(raw: &[u8]) -> (String, Charset) {
    if let Ok(text) = std::str::from_utf8(raw) {
        return (text.to_string(), Charset::Utf8);
    }
    let mut best: Option<(i32, String, Charset)> = None;
    // 顺序即平局的方向：GBK 优先（见模块文档）。
    for (encoding, charset) in [
        (GBK, Charset::Gbk),
        (BIG5, Charset::Big5),
        (SHIFT_JIS, Charset::ShiftJis),
    ] {
        let Some(text) = decode_strictly(encoding, raw) else {
            continue;
        };
        let score = plausibility(raw, charset);
        if best.as_ref().is_none_or(|(seen, _, _)| score > *seen) {
            best = Some((score, text, charset));
        }
    }
    match best {
        Some((_, text, charset)) => (text, charset),
        // 三种都解不出来：如实有损转换，并**记下这一档**——那批名字不许参与匹配。
        None => (String::from_utf8_lossy(raw).into_owned(), Charset::Lossy),
    }
}

/// 按某种编码解，**一个替换字符都不许出现**。
///
/// `encoding_rs` 遇到解不了的字节会塞一个 `U+FFFD` 进去并把 `had_errors` 置真。
/// 那正是要躲开的东西：一条「解出来了但里面有三个问号」的名字比乱码更危险，
/// 它看上去是成功的。
fn decode_strictly(encoding: &'static Encoding, raw: &[u8]) -> Option<String> {
    let (text, had_errors) = encoding.decode_without_bom_handling(raw);
    (!had_errors).then(|| text.into_owned())
}

/// 这串字节按某种编码读**像不像真话**。
///
/// 判据全在字节上，不在解出来的字上——解出来的字在三种编码下都是汉字，分不开
/// （见模块文档那张表）。
fn plausibility(raw: &[u8], charset: Charset) -> i32 {
    let mut score = 0i32;
    let mut at = 0usize;
    while at < raw.len() {
        let byte = raw[at];
        if byte < 0x80 {
            // ASCII：三种编码都一样，给一分只为让「全是 ASCII」的名字不至于零分。
            score += 1;
            at += 1;
            continue;
        }
        match charset {
            Charset::Gbk => {
                let Some(&trail) = raw.get(at + 1) else { break };
                // GB2312 的一二级汉字区：日常中文几乎全在这儿。
                score += if (0xB0..=0xF7).contains(&byte) && trail >= 0xA1 {
                    3
                } else {
                    1
                };
                at += 2;
            }
            Charset::Big5 => {
                score += match byte {
                    0xA4..=0xC6 => 3,
                    0xC9..=0xF9 => 1,
                    _ => 0,
                };
                at += 2;
            }
            Charset::ShiftJis => {
                // ⭐ **半角片假名倒扣分**：一串 GBK 汉字在 Shift_JIS 眼里正是一串
                // 半角片假名，而真实文件名里几乎没人用它。这一条比什么都灵。
                if (0xA1..=0xDF).contains(&byte) {
                    score -= 4;
                    at += 1;
                    continue;
                }
                score += 3;
                at += 2;
            }
            Charset::Utf8 | Charset::Lossy => at += 1,
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真库里那四条名字的**原始字节**，从 `/Volumes/新加卷/Game` 的 zip 中央目录里
    /// 逐字节读出来（只读，ADR-0004）。**不编造字节**：编出来的样本会把「真实世界里
    /// 长得不太一样的那些」全抹平（同 `testing::cart` 那七段）。
    const 真库里的四条: [(&[u8], &str); 4] = [
        (
            &[
                0xc9, 0xcf, 0xba, 0xa3, 0xb4, 0xf3, 0xba, 0xe0, 0x2e, 0x6e, 0x65, 0x73,
            ],
            "上海大亨.nes",
        ),
        (
            &[
                0xca, 0xa5, 0xb6, 0xb7, 0xca, 0xbf, 0xd0, 0xc7, 0xca, 0xb8, 0x32, 0xba, 0xba, 0xbb,
                0xaf, 0x2e, 0x6e, 0x65, 0x73,
            ],
            "圣斗士星矢2汉化.nes",
        ),
        (
            &[0xb7, 0xe2, 0xc9, 0xf1, 0xb0, 0xf1, 0x2e, 0x6e, 0x65, 0x73],
            "封神榜.nes",
        ),
        (
            &[
                0xbf, 0xec, 0xb4, 0xf2, 0xd0, 0xfd, 0xb7, 0xe7, 0x2e, 0x6e, 0x65, 0x73,
            ],
            "快打旋风.nes",
        ),
    ];

    #[test]
    fn 真库里那批_gbk_名字解得对() {
        // 这四条都是真库 `FC/【HACK版游戏】/…/1.FC经典游戏/` 下面 zip 里的内部名字。
        // 票 03 把它们变成了 `\u{fffd}\u{fffd}\u{fffd}…`，而票 11 要拿名字去撞库。
        for (raw, want) in 真库里的四条 {
            let (text, charset) = decode(raw);
            assert_eq!(text, want);
            assert_eq!(charset, Charset::Gbk, "{want}");
        }
    }

    #[test]
    fn 不许把一种乱码换成另一种() {
        // `上海大亨` 用 Big5 解出来是 `奻漆湮箋`——解得出来，而且没有一个替换字符。
        // 光看「解不解得出」是分不开的，判据只能是「这串字节落在哪一区」。
        let (text, _) = super::decode_strictly(BIG5, 真库里的四条[0].0)
            .map(|text| (text, ()))
            .expect("Big5 也解得出来，这正是麻烦所在");
        assert_ne!(text, "上海大亨.nes");
        // 而探测选的是 GBK。
        assert_eq!(decode(真库里的四条[0].0).1, Charset::Gbk);
    }

    #[test]
    fn 日文名字探得出_shift_jis() {
        // `ドラゴンクエスト.nes`。Big5 解不出来（首字节 0x83 不是合法 Big5 前导），
        // GBK 解得出来但落在生僻区，Shift_JIS 落在假名区。
        let raw: &[u8] = &[
            0x83, 0x68, 0x83, 0x89, 0x83, 0x53, 0x83, 0x93, 0x83, 0x4e, 0x83, 0x47, 0x83, 0x58,
            0x83, 0x67, 0x2e, 0x6e, 0x65, 0x73,
        ];
        let (text, charset) = decode(raw);
        assert_eq!(charset, Charset::ShiftJis, "解成了「{text}」");
        assert_eq!(text, "ドラゴンクエスト.nes");
    }

    #[test]
    fn 繁体名字探得出_big5() {
        // `快打旋風.nes` 的 Big5 字节。同一个游戏名，GBK 那一版在上面那张表里。
        let raw: &[u8] = &[
            0xa7, 0xd6, 0xa5, 0xb4, 0xb1, 0xdb, 0xad, 0xb7, 0x2e, 0x6e, 0x65, 0x73,
        ];
        let (text, charset) = decode(raw);
        assert_eq!(charset, Charset::Big5, "解成了「{text}」");
        assert_eq!(text, "快打旋風.nes");
    }

    #[test]
    fn 合法_utf8_不猜() {
        let (text, charset) = decode("超级马里奥.nes".as_bytes());
        assert_eq!(text, "超级马里奥.nes");
        assert_eq!(charset, Charset::Utf8);
    }

    #[test]
    fn 内部路径的分隔符统一成斜杠() {
        // 规范要求用 `/`，但确实有工具写 `\`（与 ADR-0020 的键同一条规矩）。
        let (path, lossy) = decode_path(b"dir\\\xc9\xcf\xba\xa3.nes");
        assert_eq!(path, "dir/上海.nes");
        assert!(!lossy);
    }

    #[test]
    fn 三种都解不出来时如实有损转换() {
        // 单独一个 0xFF：GBK 与 Big5 的前导字节都到不了它，Shift_JIS 也不认。
        let (text, charset) = decode(&[0xff, 0xfe, 0x41]);
        assert_eq!(charset, Charset::Lossy);
        assert!(text.contains('\u{fffd}'), "{text}");
    }
}
