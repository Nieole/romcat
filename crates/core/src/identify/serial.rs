//! **第二命中层：光盘序列号。**
//!
//! 第一命中层（CRC-32 加大小）对光盘世代结构性地不够用，理由和 ADR-0002 说汉化版的
//! 那条一模一样，只是换了个地方发作：
//!
//! - **压缩镜像的 CRC-32 算在压缩之后的字节上**，DAT 记的是原始转储的（ADR-0014）。
//! - **汉化版改过字节**，而序列号写在盘里、补丁通常不动它。
//! - **目录树转储压根没有一个「整个文件」可以算哈希**。
//!
//! 而序列号**几百字节就读得到**（[`disc`](super::disc)）。所以这一层的口号是：
//! **识别一个 8 GB 的 ISO 不需要读 8 GB。**
//!
//! ## 一条序列号命中值多少
//!
//! 票据定的是「序列号命中的候选置信度等同于精确哈希命中」。落到实现上要分两档，
//! 因为**「对上」这件事本身有粗细之分**：
//!
//! - **精确**：DAT 里那条记录写的就是这一串（折平之后完全相等）→ **高置信、自动通过**。
//! - **含在里面**：DAT 写的是一条 CONTENT_ID，而对上的只是里面那段 TitleID
//!   → **中置信、通过但标记**。这不是打折，是如实说：同一个 TitleID 下面躺着本体、
//!   更新和一堆 DLC，光凭 TitleID 说不出是哪一个（`dat::serial` 的模块文档）。
//!
//! ## 目录名里那个 TitleID 是**一条独立的依据**
//!
//! 真库的 PSV 目录名里直接写着 TitleID（`AIME00001(wan华镜 v3.1)`、
//! `A11 …[PCSG00245][日版]…`）。它值一条候选，但**永远不自动通过**：目录只是强先验
//! （ADR-0011），而这一条连内容都没看。它的用处是与盘里读出来的那个**互相印证**
//! ——两个对上，人在裁决队列里一眼就能拍板。

use crate::catalog::identify::{Candidate, Confidence};
use crate::dat::{Convention, DatRepo, RepoError, SerialHit, serial as folding};

use super::disc::{DiscId, IdKind};

/// 一条标识，连同它是从哪一份内容上读出来的。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    /// 是变体的哪个成员。
    pub member: String,
    /// 容器内部路径；裸文件是空串。
    pub inner: String,
    /// 读出来的标识。
    pub id: DiscId,
    /// 这一条是从**内容**里读出来的，还是从**名字**上看出来的。
    pub from_content: bool,
    /// 这份内容在 NKit 这一关上是干净的吗。
    ///
    /// **两件事同时成立才算干净**：验出来不是 NKit，而且——如果它是一张 GC / Wii 光盘
    /// ——**确实验过**。没验过不算干净，理由与第一命中层那条一模一样：NKit 处理过的
    /// 镜像的 CRC32 可能与好转储相同（Dolphin），序列号更是原样保留，两样都骗得过。
    pub nkit_clean: bool,
}

/// 名字**开头**那一串 TitleID（`AIME00001(wan华镜 v3.1)`、`PCSG00718(恋爱复仇战)`）。
///
/// [`scope`](super::scope) 拿它判「这个前缀是不是 Sony 发出去的」，这里拿它当依据的一半
/// ——**两处看的是同一个形状，所以只写一处**。判据是「四个字母加五位数字，前后都是边界」。
#[must_use]
pub fn title_id_head(name: &str) -> Option<String> {
    let chars: Vec<char> = name.chars().collect();
    title_id_at(&chars, 0)
}

/// 名字里**任意位置**写着的 TitleID，取第一个。
///
/// PSV 的目录名把它塞在中间：`A11 新罗罗的炼金工房[PCSG00245][日版][v1.03][NONPDRM]`。
#[must_use]
pub fn title_id_in_name(name: &str) -> Option<DiscId> {
    let chars: Vec<char> = name.chars().collect();
    let shown = (0..chars.len().saturating_sub(8)).find_map(|at| title_id_at(&chars, at))?;
    Some(DiscId {
        key: folding::normalize(&shown)?,
        shown,
        kind: IdKind::TitleId,
        from: "名字里直接写着的 TitleID".to_string(),
        title: None,
        version: None,
    })
}

/// `chars[at..at + 9]` 是不是一串 TitleID；是的话交出大写形式。
///
/// **前后都要是边界**——不然 `XPCSG002451` 里也能切出一串来。
fn title_id_at(chars: &[char], at: usize) -> Option<String> {
    let slice = chars.get(at..at + 9)?;
    let boundary = at == 0 || !chars[at - 1].is_ascii_alphanumeric();
    let shaped = slice[..4].iter().all(char::is_ascii_alphabetic)
        && slice[4..].iter().all(char::is_ascii_digit);
    let ends = chars.get(at + 9).is_none_or(|c| !c.is_ascii_alphanumeric());
    (boundary && shaped && ends).then(|| slice.iter().collect::<String>().to_uppercase())
}

/// 拿收集到的标识去撞 DAT，产出**候选**。
///
/// **验不了或者验出来是 NKit 的一律不自动通过**（[`Evidence::nkit_clean`]）——NKit 前置于
/// 任何命中，序列号这一层也不例外：一份 NKit 镜像的序列号照样是对的，但它不是一份好
/// 转储，认下它等于把「命中」两个字用在一份需要先转回 ISO 的文件上。
///
/// 每一项还带着 DAT 那条记录的**父条目名**（No-Intro 的 parent/clone）。它与候选成对走
/// 而不是塞进 [`Candidate`]，是因为下游要按它把同一部**作品**下的几个发行版归堆
/// （ADR-0010），而排序会打乱顺序——靠下标去另一张表里找它，改一次排序就错一次。
///
/// # Errors
/// 读 DAT 库失败时返回错误。
pub fn candidates(
    repo: &DatRepo,
    found: &[Evidence],
) -> Result<Vec<(Candidate, Option<String>)>, RepoError> {
    let mut out: Vec<(Candidate, Option<String>)> = Vec::new();
    for evidence in found {
        for hit in repo.lookup_serial(&evidence.id.key)? {
            // DAT 那一条写的就是这一串吗。写的是一条 CONTENT_ID 而只有里面那段对上时，
            // 折平之后不相等——那就是「含在里面」那一档。
            let exact = folding::normalize(&hit.shown).as_deref() == Some(evidence.id.key.as_str());
            let candidate = candidate_of(evidence, &hit, exact);
            // 同一条 DAT 条目会被好几个标识撞上（CONTENT_ID 一次、TitleID 再一次）。
            // 留**先出现的那条**：`found` 是按可信程度排好交进来的。
            if !out
                .iter()
                .any(|(seen, _)| seen.source == candidate.source && seen.game == candidate.game)
            {
                out.push((candidate, hit.cloneof.clone()));
            }
        }
    }
    Ok(out)
}

fn candidate_of(evidence: &Evidence, hit: &SerialHit, exact: bool) -> Candidate {
    let accepted = exact && evidence.from_content && evidence.nkit_clean;
    let mut text = format!(
        "{} 的《{}》里条目「{}」写着{} {}，与{}读出来的{} {} 对上",
        hit.source,
        hit.dat,
        hit.game,
        if exact { "" } else { "的" },
        hit.shown,
        if evidence.from_content {
            "从内容里"
        } else {
            "从名字上"
        },
        evidence.id.kind.label(),
        evidence.id.shown,
    );
    text.push_str(&format!("（{}）", evidence.id.from));
    // **标题与版本读出来了就写在依据里**——票据要的是「取得 TitleID、标题与版本」，
    // 而一条读出来却没人看得见的字段等于没读。它们不参与命中（命中靠编号），
    // 但人在裁决队列里一眼就能看出这条候选对不对。
    if let Some(title) = &evidence.id.title {
        text.push_str(&format!("；盘里的标题是「{title}」"));
    }
    if let Some(version) = &evidence.id.version {
        text.push_str(&format!("，版本 {version}"));
    }
    if !exact {
        text.push_str(
            "；**对上的只是里面那一段**——同一个 TitleID 下面还有更新与 DLC，\
             光凭它说不出是哪一条，不自动通过",
        );
    }
    if !evidence.from_content {
        text.push_str("；**这一条只看了名字没看内容**，目录只是强先验（ADR-0011），不自动通过");
    }
    if exact && evidence.from_content && !evidence.nkit_clean {
        text.push_str(
            "；这份镜像**没验过 NKit 或者验出来就是 NKit**——NKit 处理过的镜像 CRC32 \
             可能与好转储相同（Dolphin），序列号对得上也不代表它是一份好转储，不自动通过",
        );
    }
    Candidate {
        member_key: evidence.member.clone(),
        inner: evidence.inner.clone(),
        confidence: if accepted {
            Confidence::High
        } else {
            Confidence::Medium
        },
        accepted,
        source: hit.source.clone(),
        dat: hit.dat.clone(),
        platform: hit.platform.clone(),
        game: hit.game.clone(),
        // 序列号撞的是**条目**不是文件记录，所以这一列写读出来的那串编号——
        // 编一个文件名顶上去，事后复核的人会以为真有那么一条 `<rom>`。
        rom: evidence.id.shown.clone(),
        hashed_as: Convention::AsIs,
        dat_convention: Convention::AsIs,
        evidence: text,
        chinese: hit.chinese,
        serial: Some(evidence.id.shown.clone()),
        release_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 标识(key: &str, kind: IdKind, from_content: bool) -> Evidence {
        标识如此(key, kind, from_content, true)
    }

    fn 标识如此(key: &str, kind: IdKind, from_content: bool, nkit_clean: bool) -> Evidence {
        Evidence {
            member: "PSV/游戏".to_string(),
            inner: String::new(),
            id: DiscId {
                key: key.to_string(),
                shown: key.to_string(),
                kind,
                from: "测试".to_string(),
                title: None,
                version: None,
            },
            from_content,
            nkit_clean,
        }
    }

    fn 命中(shown: &str) -> SerialHit {
        SerialHit {
            source: "No-Intro".to_string(),
            dat: "Unofficial - Sony - PlayStation Vita (VPK)".to_string(),
            platform: "PSV".to_string(),
            game: "Akiba Strip 2 (Korea)".to_string(),
            cloneof: None,
            shown: shown.to_string(),
            chinese: None,
        }
    }

    #[test]
    fn 目录名里的_titleid_认得出来() {
        // 真库里点名的那两个形态。
        assert_eq!(
            title_id_in_name("AIME00001(wan华镜 v3.1)")
                .expect("认得出")
                .shown,
            "AIME00001"
        );
        assert_eq!(
            title_id_in_name("A11 新罗罗的炼金工房[PCSG00245][日版][v1.03][NONPDRM]")
                .expect("认得出")
                .shown,
            "PCSG00245"
        );
        assert_eq!(
            title_id_in_name("PCSG00718(恋爱复仇战)")
                .expect("认得出")
                .key,
            "PCSG00718"
        );
    }

    #[test]
    fn 不是那个形状的名字不许硬切一段出来() {
        assert!(title_id_in_name("超级马里奥").is_none());
        assert!(title_id_in_name("XPCSG002451").is_none(), "前后要有边界");
        assert!(title_id_in_name("ABC1234").is_none(), "位数不对");
    }

    #[test]
    fn 精确命中的序列号候选自动通过() {
        let candidate = candidate_of(
            &标识("PCSG00159", IdKind::TitleId, true),
            &命中("PCSG-00159"),
            true,
        );
        assert_eq!(candidate.confidence, Confidence::High);
        assert!(candidate.accepted, "票据：序列号命中等同于精确哈希命中");
        assert!(candidate.evidence.contains("对上"));
        assert_eq!(candidate.serial.as_deref(), Some("PCSG00159"));
    }

    #[test]
    fn 只对上_content_id_里那一段的不自动通过() {
        // 同一个 TitleID 下面躺着本体、更新和一堆 DLC，光凭它说不出是哪一条。
        let candidate = candidate_of(
            &标识("PCSG00245", IdKind::TitleId, true),
            &命中("JP0103-PCSG00245_00-SPECIALFREE00001"),
            false,
        );
        assert_eq!(candidate.confidence, Confidence::Medium);
        assert!(!candidate.accepted);
        assert!(candidate.evidence.contains("里面那一段"));
    }

    #[test]
    fn 名字上看来的那一条永远不自动通过() {
        let candidate = candidate_of(
            &标识("PCSG00159", IdKind::TitleId, false),
            &命中("PCSG-00159"),
            true,
        );
        assert!(!candidate.accepted, "目录只是强先验（ADR-0011）");
        assert!(candidate.evidence.contains("只看了名字"));
    }

    #[test]
    fn nkit_验不了的序列号命中也不自动通过() {
        // NKit 前置于**任何**命中，序列号这一层不例外：编号是对的，但那不是一份好转储。
        let candidate = candidate_of(
            &标识如此("GALE01", IdKind::DiscId, true, false),
            &命中("GALE01"),
            true,
        );
        assert!(!candidate.accepted);
        assert!(candidate.evidence.contains("NKit"));
    }
}
