//! **数据源清单**：五个源从哪儿取、长什么样、哪份 DAT 归哪个平台。
//!
//! 与平台清单同一个道理（`docs/platforms.md`：**平台清单是数据不是代码**）——加一个源、
//! 改一条映射都不该动 Rust。选 TOML 而不是 JSON 也是同一个理由：这份表里每一条都得写清
//! 「为什么这么定」，而 JSON 没有注释。
//!
//! ## 映射：第一条匹配的说了算
//!
//! 一份 DAT 归哪个平台，靠一串**按顺序**试的模式（复用成型规则那套
//! [`Pattern`]，不引正则引擎）。**从上往下第一条匹配的胜出**，于是
//! 「先写窄的、再写宽的」就是全部规则：
//!
//! ```text
//! "Nintendo - Nintendo Entertainment System (Headerless)"  → FC，去头
//! "Nintendo - Nintendo Entertainment System (Head*"        → FC，含头
//! ```
//!
//! **没有任何映射命中的 DAT 整份不入库**。这不是遗漏而是范围边界，与
//! ADR-0011 修订段对未映射目录的处置同源：工具只管说得清归属的那部分，
//! 其余照报数不照收。

use std::collections::BTreeSet;
use std::path::Path;

use serde::Deserialize;

use super::Convention;
use crate::platform::Manifest;
use crate::platform::pattern::{Pattern, PatternError};

/// 内置的那一份。
const BUILTIN: &str = include_str!("sources.toml");

/// 本程序认得的清单版本。
pub const REGISTRY_VERSION: u32 = 1;

/// 一份 DAT 长什么样，决定用哪个解析器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Shape {
    /// Logiqx XML。Redump、TOSEC、No-Intro 都是这一种。
    #[serde(rename = "Logiqx")]
    Logiqx,
    /// MAME 的 software list XML。
    #[serde(rename = "MAME software list")]
    SoftwareList,
    /// BizHawk 的 gamedb 纯文本。
    #[serde(rename = "BizHawk gamedb")]
    GameDb,
}

impl Shape {
    /// 报告里写的那个词。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Logiqx => "Logiqx",
            Self::SoftwareList => "MAME software list",
            Self::GameDb => "BizHawk gamedb",
        }
    }
}

/// 一个源怎么列举、怎么取。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// GitHub 发行资产里的一个**整包**。
    ///
    /// 增量的粒度就是这个包——镜像只发整包，发不了单份 DAT。指纹取资产的
    /// `updated_at` 加大小。
    ReleaseBundle {
        /// `主/仓库`。
        repo: String,
        /// 资产名，如 `no-intro.zip`。
        asset: String,
    },
    /// Redump 的**现行**站点，一个系统一份 DAT。
    ///
    /// 指纹取响应头 `Content-Disposition` 里的附件名——里面带条目数与生成时刻，
    /// 而 Redump 既不给 `ETag` 也不给 `Last-Modified`。
    RedumpSite {
        /// 站点根，如 `https://redump.info`。
        site: String,
    },
    /// GitHub 仓库里的一批文件。
    ///
    /// 列举走 `git/trees?recursive=1`——**`/contents` 在 1000 条处硬截断且不报错**，
    /// 调研中因此误判 TOSEC 缺少多个平台集。指纹取每个 blob 的 sha，于是「哪几份变了」
    /// 是仓库自己算好的，天然增量。
    RepoFiles {
        /// `主/仓库`。
        repo: String,
        /// 分支或提交。
        reference: String,
        /// 只看这棵子树（如 `hash`、`Assets/gamedb`）。
        ///
        /// 有它就不必递归列举整个仓库——MAME 有五万多个文件，递归列举一次又大又慢，
        /// 还可能撞上 trees 接口自己的上限。
        subtree: Option<String>,
    },
}

impl Origin {
    /// 这个源取回来的是一个**整包**吗。
    ///
    /// 整包那一档的映射作用在包**里面**的每份 DAT 上，包本身没有平台——排计划时
    /// 与逐份取的源走的是两条路。
    #[must_use]
    pub fn is_bundle(&self) -> bool {
        matches!(self, Self::ReleaseBundle { .. })
    }

    /// 「从哪儿取」一句话。**报告里必须打出真实地址**——两条取数纪律说的都是
    /// 「别连那个站」，而用户唯一能核对这件事的地方就是这一行。
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::ReleaseBundle { repo, asset } => {
                format!("GitHub 发行资产 https://github.com/{repo} 的 {asset}")
            }
            Self::RedumpSite { site } => format!("{site}/downloads/ 与 {site}/datfile/<短码>"),
            Self::RepoFiles {
                repo,
                reference,
                subtree,
            } => {
                let 子树 = subtree
                    .as_deref()
                    .map_or_else(String::new, |subtree| format!(" 的 {subtree}/"));
                format!(
                    "GitHub 仓库 https://github.com/{repo}@{reference}{子树}（走 git/trees，不走 /contents）"
                )
            }
        }
    }
}

/// 一个数据源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// 源名，映射那边引用它。
    pub name: String,
    /// 怎么列举、怎么取。
    pub origin: Origin,
    /// DAT 长什么样。
    pub shape: Shape,
    /// 默认哈希口径。单份 DAT 可以在映射里覆盖。
    pub convention: Convention,
    /// 为什么是这么取的。会打给用户看。
    pub note: String,
}

/// 一条 DAT → 平台的映射。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    /// 属于哪个源。
    pub source: String,
    /// 拿什么去匹配：No-Intro 是 DAT 名，Redump 是系统短码，其余是仓库内路径。
    pub pattern: Pattern,
    /// 归哪个平台。`None` 表示**明确不要**。
    pub platform: Option<String>,
    /// 覆盖源的默认哈希口径。
    pub convention: Option<Convention>,
    /// 为什么这么定，或者为什么不要。
    pub note: String,
}

/// 一条 DAT 落在哪。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mapped<'a> {
    /// 归哪个平台。
    pub platform: &'a str,
    /// 用哪套哈希口径。
    pub convention: Convention,
}

/// 查一条 DAT 的下场。
///
/// **不入库的两种要分得开**：「明确不要」带着理由，「没映射」什么也没说。报告里把两者
/// 合成一个数，「TOSEC 怎么少了这么多」就得每次重新查一遍。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup<'a> {
    /// 归某个平台。
    Mapped(Mapped<'a>),
    /// 命中了一条「明确不要」，附理由。
    Excluded(&'a str),
    /// 一条映射都没命中。这是范围边界，不是遗漏。
    Unmapped,
}

/// 数据源清单本身写坏了。
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    /// 文件读不出来。
    #[error("数据源清单 {path} 读不出来：{source}")]
    Io {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// TOML 语法或结构不对。
    #[error("数据源清单 {path} 读不动：{source}")]
    Toml {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: toml::de::Error,
    },
    /// 版本对不上。
    #[error("数据源清单 {path} 的版本是 {found}，本程序认得的是 {REGISTRY_VERSION}")]
    Version {
        /// 出问题的文件。
        path: String,
        /// 文件里写的版本。
        found: u32,
    },
    /// 内容自相矛盾。
    #[error("数据源清单 {path} 里 {detail}")]
    Invalid {
        /// 出问题的文件。
        path: String,
        /// 哪里不对。
        detail: String,
    },
    /// 模式写坏了。
    #[error("数据源清单 {path} 里的模式有问题：{source}")]
    Pattern {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: PatternError,
    },
}

/// 一份数据源清单。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registry {
    sources: Vec<Source>,
    mappings: Vec<Mapping>,
}

#[derive(Debug, Deserialize)]
struct RawRegistry {
    #[serde(rename = "版本")]
    version: u32,
    #[serde(rename = "数据源", default)]
    sources: Vec<RawSource>,
    #[serde(rename = "映射", default)]
    mappings: Vec<RawMapping>,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "取法")]
    origin: String,
    #[serde(rename = "格式")]
    shape: Shape,
    #[serde(rename = "口径")]
    convention: String,
    #[serde(rename = "说明", default)]
    note: String,
    #[serde(rename = "仓库", default)]
    repo: Option<String>,
    #[serde(rename = "资产", default)]
    asset: Option<String>,
    #[serde(rename = "引用", default)]
    reference: Option<String>,
    #[serde(rename = "子树", default)]
    subtree: Option<String>,
    #[serde(rename = "站点", default)]
    site: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMapping {
    #[serde(rename = "源")]
    source: String,
    #[serde(rename = "名")]
    pattern: String,
    #[serde(rename = "平台", default)]
    platform: Option<String>,
    #[serde(rename = "跳过", default)]
    skip: Option<String>,
    #[serde(rename = "口径", default)]
    convention: Option<String>,
    #[serde(rename = "说明", default)]
    note: String,
}

impl Registry {
    /// 内置的那一份。
    ///
    /// # Panics
    /// 内置清单写坏了就是编译期该发现的错，这里直接炸。测试里有一条钉着它读得动。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse(BUILTIN, "内置数据源清单").expect("内置数据源清单必须读得动")
    }

    /// 内置那一份的原文，用来导出底稿。
    #[must_use]
    pub fn builtin_text() -> &'static str {
        BUILTIN
    }

    /// 从文件读一份。
    ///
    /// # Errors
    /// 读不出来、语法不对、版本对不上或内容自相矛盾时返回错误。
    pub fn load(file: &Path) -> Result<Self, RegistryError> {
        let path = crate::path::display(file);
        let text = std::fs::read_to_string(file).map_err(|source| RegistryError::Io {
            path: path.clone(),
            source,
        })?;
        Self::parse(&text, &path)
    }

    /// 从 TOML 文本读一份。
    ///
    /// # Errors
    /// 语法不对、版本对不上或内容自相矛盾时返回错误。
    pub fn parse(text: &str, path: &str) -> Result<Self, RegistryError> {
        let raw: RawRegistry = toml::from_str(text).map_err(|source| RegistryError::Toml {
            path: path.to_string(),
            source,
        })?;
        if raw.version != REGISTRY_VERSION {
            return Err(RegistryError::Version {
                path: path.to_string(),
                found: raw.version,
            });
        }
        let invalid = |detail: String| RegistryError::Invalid {
            path: path.to_string(),
            detail,
        };

        let mut sources = Vec::with_capacity(raw.sources.len());
        for source in raw.sources {
            let convention = Convention::from_label(&source.convention).ok_or_else(|| {
                invalid(format!(
                    "源 {} 的口径 {} 不认得",
                    source.name, source.convention
                ))
            })?;
            let missing = |field: &str| {
                invalid(format!(
                    "源 {} 的取法是 {}，却没给「{field}」",
                    source.name, source.origin
                ))
            };
            let origin = match source.origin.as_str() {
                "GitHub 发行资产" => {
                    let repo = source.repo.clone().ok_or_else(|| missing("仓库"))?;
                    let asset = source.asset.clone().ok_or_else(|| missing("资产"))?;
                    // 「Redump 不走那个镜像」在 URL 上看不出来（同一个主机、同一个
                    // 发行），判据是**资产名**。在读清单这一步就拦下，免得一份改坏的
                    // 清单要等到跑完才发现取的是 60 个系统的旧数据。
                    super::guard::check_asset(&repo, &asset)
                        .map_err(|refusal| invalid(format!("{refusal}")))?;
                    Origin::ReleaseBundle { repo, asset }
                }
                "Redump 站点" => Origin::RedumpSite {
                    site: source.site.clone().ok_or_else(|| missing("站点"))?,
                },
                "GitHub 仓库文件" => Origin::RepoFiles {
                    repo: source.repo.clone().ok_or_else(|| missing("仓库"))?,
                    reference: source.reference.clone().ok_or_else(|| missing("引用"))?,
                    subtree: source.subtree.clone(),
                },
                other => return Err(invalid(format!("源 {} 的取法 {other} 不认得", source.name))),
            };
            sources.push(Source {
                name: source.name,
                origin,
                shape: source.shape,
                convention,
                note: source.note,
            });
        }

        let names: BTreeSet<&str> = sources.iter().map(|source| source.name.as_str()).collect();
        let mut mappings = Vec::with_capacity(raw.mappings.len());
        for mapping in raw.mappings {
            if !names.contains(mapping.source.as_str()) {
                return Err(invalid(format!("映射引用了不存在的源 {}", mapping.source)));
            }
            // 「归哪个平台」与「明确不要」必须二选一：两个都给说不清要不要，
            // 两个都不给的话这条映射什么也没说。
            let platform = match (&mapping.platform, &mapping.skip) {
                (Some(platform), None) => Some(platform.clone()),
                (None, Some(_)) => None,
                _ => {
                    return Err(invalid(format!(
                        "映射 {} 要么给「平台」要么给「跳过」，正好一个",
                        mapping.pattern
                    )));
                }
            };
            let convention = match &mapping.convention {
                Some(label) => Some(Convention::from_label(label).ok_or_else(|| {
                    invalid(format!("映射 {} 的口径 {label} 不认得", mapping.pattern))
                })?),
                None => None,
            };
            let note = mapping.skip.clone().unwrap_or(mapping.note);
            mappings.push(Mapping {
                source: mapping.source,
                pattern: Pattern::new(&mapping.pattern).map_err(|source| {
                    RegistryError::Pattern {
                        path: path.to_string(),
                        source,
                    }
                })?,
                platform,
                convention,
                note,
            });
        }

        Ok(Self { sources, mappings })
    }

    /// 全部数据源。
    #[must_use]
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    /// 全部映射。
    #[must_use]
    pub fn mappings(&self) -> &[Mapping] {
        &self.mappings
    }

    /// 按名字找一个源。
    #[must_use]
    pub fn source(&self, name: &str) -> Option<&Source> {
        self.sources.iter().find(|source| source.name == name)
    }

    /// 这份 DAT 的下场。**从上往下第一条匹配的胜出。**
    ///
    /// 「第一条匹配的胜出」这条规则只在这里实现一次——排计划、入库、报告都问它，
    /// 各写一遍的话三处迟早各走各的。
    #[must_use]
    pub fn lookup<'a>(&'a self, source: &str, name: &str) -> Lookup<'a> {
        let Some(mapping) = self
            .mappings
            .iter()
            .find(|mapping| mapping.source == source && mapping.pattern.matches(name))
        else {
            return Lookup::Unmapped;
        };
        let Some(platform) = mapping.platform.as_deref() else {
            return Lookup::Excluded(&mapping.note);
        };
        let default = self
            .source(source)
            .map_or(Convention::AsIs, |source| source.convention);
        Lookup::Mapped(Mapped {
            platform,
            convention: mapping.convention.unwrap_or(default),
        })
    }

    /// 这份 DAT 归哪个平台、用哪套口径；不入库时是 `None`。
    ///
    /// [`Self::lookup`] 的简写，给只关心「要不要」的调用方用。
    #[must_use]
    pub fn map<'a>(&'a self, source: &str, name: &str) -> Option<Mapped<'a>> {
        match self.lookup(source, name) {
            Lookup::Mapped(mapped) => Some(mapped),
            Lookup::Excluded(_) | Lookup::Unmapped => None,
        }
    }

    /// 映射里提到、但平台清单里没有的平台名。
    ///
    /// 打错一个字的后果是那份 DAT 静悄悄地归到一个谁也查不到的平台上，报告里多出一行
    /// 没人认得的名字。开工前查一遍，当场说出来。
    #[must_use]
    pub fn unknown_platforms(&self, manifest: &Manifest) -> Vec<&str> {
        let known: BTreeSet<&str> = manifest
            .platforms()
            .iter()
            .map(|platform| platform.name.as_str())
            .collect();
        let mut unknown: Vec<&str> = self
            .mappings
            .iter()
            .filter_map(|mapping| mapping.platform.as_deref())
            .filter(|name| !known.contains(name))
            .collect();
        unknown.sort_unstable();
        unknown.dedup();
        unknown
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 内置清单读得动且五个源都在() {
        let registry = Registry::builtin();
        let names: Vec<&str> = registry
            .sources()
            .iter()
            .map(|source| source.name.as_str())
            .collect();
        assert_eq!(names, ["No-Intro", "Redump", "TOSEC", "MAME", "GoodNES"]);
    }

    #[test]
    fn 内置清单里的平台名都在平台清单里() {
        // 打错一个字，那份 DAT 就归到一个谁也查不到的平台上。
        let registry = Registry::builtin();
        assert_eq!(
            registry.unknown_platforms(&Manifest::builtin()),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn no_intro_的两套_nes_口径分得开() {
        // 这是这张票最要紧的一条：同一个平台上两份 DAT，哈希口径相反。
        let registry = Registry::builtin();
        let headerless = registry
            .map(
                "No-Intro",
                "Nintendo - Nintendo Entertainment System (Headerless)",
            )
            .expect("该有映射");
        assert_eq!(headerless.platform, "FC");
        assert_eq!(headerless.convention, Convention::Headerless);

        let headered = registry
            .map(
                "No-Intro",
                "Nintendo - Nintendo Entertainment System (Headered)",
            )
            .expect("该有映射");
        assert_eq!(headered.platform, "FC");
        assert_eq!(headered.convention, Convention::AsIs);
    }

    #[test]
    fn tosec_一律按含头() {
        // 只算去头哈希就会丢掉 TOSEC 里全部汉化条目（ADR-0002 修订段）。
        let registry = Registry::builtin();
        let mapped = registry
            .map("TOSEC", "TOSEC/SNK Neo-Geo Pocket Color - Games.dat")
            .expect("该有映射");
        assert_eq!(mapped.platform, "NGPC");
        assert_eq!(mapped.convention, Convention::AsIs);
    }

    #[test]
    fn 扫描件那一族明确不要() {
        let registry = Registry::builtin();
        assert_eq!(
            registry.map(
                "TOSEC",
                "TOSEC-PIX/Nintendo Famicom & Entertainment System - Books.dat"
            ),
            None
        );
    }

    #[test]
    fn 没有映射命中就不入库() {
        let registry = Registry::builtin();
        assert_eq!(registry.map("No-Intro", "Commodore - Amiga"), None);
        assert_eq!(
            registry.map("No-Intro", "Source Code - Nintendo - Game Boy Color"),
            None
        );
    }

    #[test]
    fn 平台与跳过必须正好给一个() {
        let text = "\"版本\" = 1\n\
[[\"数据源\"]]\n\"名\" = \"X\"\n\"取法\" = \"Redump 站点\"\n\"站点\" = \"https://redump.info\"\n\"格式\" = \"Logiqx\"\n\"口径\" = \"含头\"\n\
[[\"映射\"]]\n\"源\" = \"X\"\n\"名\" = \"PSX\"\n";
        let error = Registry::parse(text, "测试").expect_err("两个都没给");
        assert!(format!("{error}").contains("正好一个"));
    }

    #[test]
    fn 清单把_redump_指到那个镜像上时读清单就失败() {
        let text = "\"版本\" = 1\n\
[[\"数据源\"]]\n\"名\" = \"Redump\"\n\"取法\" = \"GitHub 发行资产\"\n\
\"仓库\" = \"hugo19941994/auto-datfile-generator\"\n\"资产\" = \"redump.zip\"\n\
\"格式\" = \"Logiqx\"\n\"口径\" = \"含头\"\n";
        let error = Registry::parse(text, "测试").expect_err("该拒");
        assert!(format!("{error}").contains("redump.org"), "{error}");
    }

    #[test]
    fn 版本对不上要说清楚() {
        let error = Registry::parse("\"版本\" = 99\n", "测试").expect_err("版本不对");
        assert!(matches!(error, RegistryError::Version { found: 99, .. }));
    }
}
