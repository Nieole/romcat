//! **标识**：从一份内容自己的字节里读出来的那一串编号。
//!
//! 它有两个产地，产物是同一个形状：
//!
//! - [光盘那一层](super::disc)读出来的光盘序列号、TitleID、CONTENT_ID 与 GC / Wii 的
//!   光盘 ID（票 09）。
//! - [卡带那一层](super::cart)读出来的**游戏码**——GBA 的 `AGB-UTTD`、NDS 的 gamecode、
//!   MD 头 `0x180` 的序列号、N64 归一化之后 `0x3B` 的 Game Code（票 10）。
//!
//! 两边共用一个类型不是图省事，是因为下游**只有一条路**：[序列号那一层](super::serial)
//! 拿它去撞 DAT 的序列号索引。No-Intro 的卡带集把 4 字符游戏码写在 `<rom serial>` 上，
//! 与 Vita 那一份 CONTENT_ID 落在同一张表里——查询方没有理由知道这一串是从盘里读的
//! 还是从卡带头里读的。
//!
//! **它们不是同一档可信度，那件事记在 [`IdKind`] 上**：一条光盘序列号说的是「这张盘
//! 就是那次发行」，而一条卡带游戏码说的是「这张卡**基于**那次发行」——汉化补丁不改
//! 游戏码，所以同一个码底下躺着原版和一堆汉化版。差别落在
//! [`IdKind::release_level_only`] 上，序列号那一层据此决定敢不敢自动通过。

/// 一条读出来的**标识**是哪一种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IdKind {
    /// 光盘序列号：PS1 / PS2 的 `SLPS-02170`、PSP 的 `ULJM-05800`。
    Serial,
    /// 数字世代的 TitleID：PSV / PS3 的 `PCSG00245`。
    TitleId,
    /// CONTENT_ID：`JP0103-PCSG00245_00-APP…`，比 TitleID 更细一档。
    ContentId,
    /// GC / Wii 的光盘 ID（`GALE01`）。
    DiscId,
    /// **卡带游戏码**：GBA 的 `BR6J`、NDS 的 `C32J`、MD 的 `G-5521-00`、
    /// N64 的 `NMQE`、SFC 扩展头里的 `A83J`。
    GameCode,
}

impl IdKind {
    /// 依据里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Serial => "光盘序列号",
            Self::TitleId => "TitleID",
            Self::ContentId => "CONTENT_ID",
            Self::DiscId => "光盘 ID",
            Self::GameCode => "卡带游戏码",
        }
    }

    /// 存进中立库用的短码。**不拿 [`label`](Self::label) 当键**：那是展示词，
    /// 改一次报告用词就会让整张缓存静默作废。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Serial => "serial",
            Self::TitleId => "title-id",
            Self::ContentId => "content-id",
            Self::DiscId => "disc-id",
            Self::GameCode => "game-code",
        }
    }

    /// 从短码读回来。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::all().into_iter().find(|it| it.code() == code)
    }

    /// 全部五种，顺序固定。
    #[must_use]
    pub fn all() -> [Self; 5] {
        [
            Self::Serial,
            Self::TitleId,
            Self::ContentId,
            Self::DiscId,
            Self::GameCode,
        ]
    }

    /// 这一种标识**最多只说得到发行版这一层**吗。
    ///
    /// **卡带游戏码是**（ADR-0008）：汉化补丁通常不改卡带头，所以同一个游戏码底下
    /// 躺着原版转储和一堆汉化版。撞上它意味着「这是哪个游戏」有了答案，而
    /// 「这是谁汉化的第几版」没有——那归**裁决**（票 08）。因此它**永远不自动通过**，
    /// 哪怕折平之后与 DAT 写的那一串完全相等。
    ///
    /// 光盘那几种**不是**：一张 PS1 盘的 `SLPS-02170` 写在盘里，而汉化过的盘
    /// 与原版盘是两份不同的转储、各自有自己的哈希——序列号对上就是同一次发行
    /// （票 09 定的「序列号命中等同于精确哈希命中」）。
    #[must_use]
    pub fn release_level_only(self) -> bool {
        matches!(self, Self::GameCode)
    }
}

/// 一条从内容里读出来的标识。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Ident {
    /// 折平后的键，撞 DAT 走它（`dat::serial::normalize`）。
    pub key: String,
    /// 原样那一串，写进依据。
    pub shown: String,
    /// 哪一种标识。
    #[serde(with = "kind_serde")]
    pub kind: IdKind,
    /// 从哪儿读出来的，写进依据。
    pub from: String,
    /// 顺带读到的标题。
    pub title: Option<String>,
    /// 顺带读到的版本。
    pub version: Option<String>,
}

mod kind_serde {
    use super::IdKind;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub(super) fn serialize<S: Serializer>(kind: &IdKind, out: S) -> Result<S::Ok, S::Error> {
        kind.code().serialize(out)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<IdKind, D::Error> {
        let code = String::deserialize(input)?;
        IdKind::from_code(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("认不出的标识种类：{code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 标识种类的短码能来回折() {
        // 短码是中立库里那张缓存的键，读不回来就等于整张缓存作废。
        for kind in IdKind::all() {
            assert_eq!(IdKind::from_code(kind.code()), Some(kind));
        }
        assert_eq!(IdKind::from_code("光盘 ID"), None, "展示词不是键");
    }

    #[test]
    fn 只有卡带游戏码停在发行版这一层() {
        // 汉化补丁不改卡带头，同一个游戏码底下躺着原版和一堆汉化版（ADR-0008）。
        assert!(IdKind::GameCode.release_level_only());
        for kind in [
            IdKind::Serial,
            IdKind::TitleId,
            IdKind::ContentId,
            IdKind::DiscId,
        ] {
            assert!(!kind.release_level_only(), "{}", kind.label());
        }
    }
}
