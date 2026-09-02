//! **序列号**在 DAT 里长什么样，以及怎么把两边折到同一个形状上。
//!
//! 光盘世代的识别锚点不是哈希而是**印在盘上、也写在盘里**的那串编号。麻烦在于同一个
//! 编号在各处的写法都不一样，而**只要有一处不折平，整条链就永远撞不上**：
//!
//! | 出处 | 样子 |
//! |---|---|
//! | PS1 盘内 `SYSTEM.CNF` 的 `BOOT` | `cdrom:\SLPS_021.70;1` |
//! | Redump / MAME 记的 | `SLPS-02170` |
//! | PSV 的 `param.sfo` `TITLE_ID` | `PCSG00245` |
//! | No-Intro 的 `<rom serial>` | `PCSG-00245` |
//! | No-Intro 的 `<game_id>` | `JP0103-PCSG00245_00-APP0000000000000` |
//!
//! 于是[归一化](normalize)只留字母与数字并折成大写：`.` `_` `-` `;1` 这些分隔符
//! 在不同的库里各写各的，留着它们等于给每条链路各准备一套写法。
//!
//! ## CONTENT_ID 要拆出里面那段 TITLE_ID
//!
//! No-Intro 的 Vita 与 PSP (PSN) 集用 **CONTENT_ID** 当 `<game_id>`，形如
//! `XXYYYY-<TITLE_ID>_NN-<LICENSE_LABEL>`（psdevwiki 原文）。而**磁盘上那份转储**
//! 的 `param.sfo` 两样都有：`CONTENT_ID` 与 `TITLE_ID`。
//!
//! 两样都要能撞：`CONTENT_ID` 一模一样时那是**同一次发行**，最准；只有 `TITLE_ID`
//! 对上时说明是同一个游戏的另一份内容（本体 / 更新 / DLC），仍然值一条候选。所以入库
//! 时**一条 CONTENT_ID 落两行索引**——完整那行与里面那段 TITLE_ID。
//!
//! `TITLE_ID = CONTENT_ID[7..16]` 由两份源码独立证实（Vita3K 的
//! `license_content_id.substr(7, 9)`、pkg2zip 的 `content + 7`，
//! 见 `docs/research/rom-identification-part2.md` 2.1.3）。
//!
//! ## 一条记录可以写好几个序列号
//!
//! MAME 的 software list 把再版与廉价版并在一条里：`SLUS-01300, SLUS-01300CE`、
//! `SLUS-00845 / SLUS-00858`。**逗号与斜杠都是分隔符**，不拆开的话这些条目一个都撞不上。

/// 序列号短到这个长度以下就不当序列号。
///
/// 挡的是真库里实测存在的两种占位符：3DS 集里 BIOS 条目的 `serial="n/a"`（折平之后
/// 剩 `NA`）与 `<game_id>################</game_id>`（折平之后什么都不剩）。
/// 拿它们去撞，一个 `n/a` 就能把几十条毫不相干的条目连成一片。
const MIN_LEN: usize = 4;

/// 一条记录里几个序列号之间的分隔符。
const SEPARATORS: &[char] = &[',', '/', ';'];

/// 折平一个序列号：只留字母与数字，折成大写。
///
/// 太短的（见 [`MIN_LEN`]）与一个字母都没有的返回 `None`——纯数字串撞上的多半是
/// 别的东西，而序列号在每一个源里都带前缀字母。
#[must_use]
pub fn normalize(text: &str) -> Option<String> {
    let folded: String = text
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();
    if folded.len() < MIN_LEN || !folded.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    Some(folded)
}

/// 这串东西是不是一个 **CONTENT_ID**；是的话返回里面那段 TITLE_ID。
///
/// 判据取自 psdevwiki 的格式定义 `XXYYYY-NP_COMMUNICATION_ID-LICENSE_ID`：
/// 两位地区字母、四位发行商数字、`-`、九位 TITLE_ID、`_`、两位子号、`-`、
/// 十六位授权标签。**要求整串都对得上**，不是找一段像的——宽判据会把
/// `SLPS-02170` 这种普通序列号也切一刀。
#[must_use]
pub fn title_id_in(text: &str) -> Option<String> {
    let bytes: Vec<char> = text.chars().collect();
    if bytes.len() != 36 {
        return None;
    }
    let is = |at: usize, f: fn(&char) -> bool| bytes.get(at).is_some_and(f);
    let range = |from: usize, to: usize, f: fn(&char) -> bool| bytes[from..to].iter().all(f);
    if !range(0, 2, char::is_ascii_alphabetic)
        || !range(2, 6, char::is_ascii_digit)
        || !is(6, |c| *c == '-')
        || !range(7, 16, char::is_ascii_alphanumeric)
        || !is(16, |c| *c == '_')
        || !range(17, 19, char::is_ascii_digit)
        || !is(19, |c| *c == '-')
    {
        return None;
    }
    normalize(&bytes[7..16].iter().collect::<String>())
}

/// 一条记录里写着的序列号，拆开、折平、去重。
///
/// 每一项是 `(折平后的键, 原样)`——**依据里要写原样**，那是人去 Redump 上核对时
/// 输进去的那一串。
#[must_use]
pub fn spread(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |key: String, shown: &str| {
        if !out.iter().any(|(seen, _)| *seen == key) {
            out.push((key, shown.to_string()));
        }
    };
    for piece in text.split(SEPARATORS) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some(key) = normalize(piece) {
            push(key, piece);
        }
        // CONTENT_ID 再落一行里面那段 TITLE_ID：本体、更新与 DLC 的 CONTENT_ID 各不
        // 相同，而磁盘上那份转储只有可能对上其中一条——另外两条要靠 TITLE_ID 才认得出。
        //
        // **这一行的 `shown` 记的是完整的 CONTENT_ID，不是切出来的那一段。** 两个理由：
        // 依据里该写 DAT 上真正写着的那一串；而查询方靠「折平后的 `shown` 等不等于查询键」
        // 分辨「精确对上」与「只对上里面那一段」（`identify::serial`），`shown` 记成切片
        // 就分不出来了——本体、更新与 DLC 会一起被当成精确命中。
        if let Some(inner) = title_id_in(piece) {
            push(inner, piece);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 各处的写法折到同一个形状上() {
        // 盘内、Redump、No-Intro、param.sfo 四种写法，折平之后必须是同一串。
        for text in ["SLPS_021.70", "SLPS-02170", "slps02170", "SLPS 021 70"] {
            assert_eq!(normalize(text).as_deref(), Some("SLPS02170"), "{text}");
        }
        assert_eq!(normalize("PCSG-00245").as_deref(), Some("PCSG00245"));
    }

    #[test]
    fn 占位符不当序列号() {
        // 真库的 3DS 集里这两种都有。放过去一个，`n/a` 就能把几十条条目连成一片。
        assert_eq!(normalize("n/a"), None, "折平只剩 NA，太短");
        assert_eq!(normalize("################"), None, "折平之后什么都不剩");
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("12345678"), None, "一个字母都没有的不算");
    }

    #[test]
    fn content_id_拆得出里面那段_title_id() {
        assert_eq!(
            title_id_in("JP0103-PCSG00245_00-APP0000000000000").as_deref(),
            Some("PCSG00245")
        );
        assert_eq!(
            title_id_in("HP9000-NPHG00061_00-100MANTONDEMOASI").as_deref(),
            Some("NPHG00061")
        );
        // 普通序列号不许被切一刀。
        assert_eq!(title_id_in("SLPS-02170"), None);
        assert_eq!(title_id_in("PCSG00245"), None);
        // 长度对但形状不对的也不算。
        assert_eq!(title_id_in(&"X".repeat(36)), None);
    }

    #[test]
    fn 一条记录里的几个序列号都拆出来() {
        // MAME 的 psx.xml 真实形态：再版与廉价版并在一条里。
        let got = spread("SLUS-01300, SLUS-01300CE");
        assert_eq!(
            got,
            vec![
                ("SLUS01300".to_string(), "SLUS-01300".to_string()),
                ("SLUS01300CE".to_string(), "SLUS-01300CE".to_string()),
            ]
        );
        assert_eq!(spread("SLUS-00845 / SLUS-00858").len(), 2, "斜杠也是分隔符");
    }

    #[test]
    fn content_id_落两行索引() {
        let got = spread("JP0103-PCSG00245_00-APP0000000000000");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].0, "JP0103PCSG0024500APP0000000000000".to_string());
        assert_eq!(got[0].1, "JP0103-PCSG00245_00-APP0000000000000".to_string());
        assert_eq!(got[1].0, "PCSG00245".to_string());
        assert_eq!(
            got[1].1, "JP0103-PCSG00245_00-APP0000000000000",
            "内嵌那一行记完整的 CONTENT_ID：查询方靠它分辨精确与「只对上里面那一段」"
        );
    }

    #[test]
    fn 同一条里重复的只落一行() {
        assert_eq!(spread("SLPS-02170 / SLPS_021.70").len(), 1);
    }
}
