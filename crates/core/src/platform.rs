//! **平台清单与成型规则**：声明式配置，加一个平台不改一行 Rust。
//!
//! 三件事写在配置里而不是代码里（`docs/platforms.md`：平台清单是数据不是代码）：
//!
//! 1. **哪些顶层目录算平台**。这同时是**范围边界**——ADR-0011 的修订段定下：未映射的
//!    顶层目录整体不进识别管线，但**库体检照报**。真库顶层 73 个条目里混着平台目录、
//!    按作品分的目录、与 ROM 无关的目录和散落的裸归档，工具只管已按平台分好的那部分。
//! 2. **每个平台用哪几条成型规则**把散落的文件聚成**变体**。
//! 3. **哪些扩展名只可能属于某一个平台**。目录说 GBA、文件是 NDS 这类冲突靠它认出来
//!    （ADR-0011：目录是强先验而非权威）。
//!
//! 内置一份 [`Manifest::builtin`]，`--manifest <文件>` 可以整份换掉。
//!
//! ## 规则的应用顺序由成型器固定
//!
//! 平台条目里的 `成型` 列表只说「用哪几条」，**不说先后**——先后是
//! [`crate::shape`] 定死的：目录树 → 同名成组 → 多碟同族。理由是这三步之间有真实的
//! 依赖：目录树一旦认下一棵子树，里面的文件就是**内部资源**，不该再被同名成组捡一遍；
//! 而多碟同族合并的是前两步的产物。把顺序交给配置只会让人写出跑不通的组合。

pub mod pattern;

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::path::{self, fold};
use pattern::{Pattern, PatternError};

/// 内置的那一份清单。
const BUILTIN: &str = include_str!("platform/platforms.toml");

/// 本程序认得的配置版本。
pub const MANIFEST_VERSION: u32 = 1;

/// 一条成型规则的**方式**。
///
/// 方式决定成型器怎么用这条规则；规则名只是平台那边引用它的把手。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ShapeKind {
    /// 同目录、同名的一组文件是一个变体：`.cue` 加它引用的 `.bin`。
    #[serde(rename = "同名成组")]
    SameStem,
    /// 一整棵目录树是一个变体：`PS3_GAME` 所在目录、PSV 的转储目录。
    #[serde(rename = "目录树")]
    DirectoryTree,
    /// 剥掉碟片标记之后同名的几个变体合成一个：`(Disc 1)` / `(Disc 2)`。
    #[serde(rename = "多碟同族")]
    DiscFamily,
}

impl ShapeKind {
    /// 报告里用的名字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::SameStem => "同名成组",
            Self::DirectoryTree => "目录树",
            Self::DiscFamily => "多碟同族",
        }
    }
}

/// 目录树规则里，变体根落在锚目录本身还是它的上一级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum TreeRoot {
    /// 锚目录自己就是变体根。Wii U 的 loadiine 转储是这样。
    #[default]
    #[serde(rename = "锚目录")]
    Anchor,
    /// 装着锚目录的那个目录才是变体根。
    ///
    /// PS3 与 PSV 都要这一条：`PS3_GAME` 边上还有 `PS3_UPDATE`，
    /// PSV 的 `app/<TitleID>` 与 `patch/<TitleID>` 是同一个变体的两半。
    #[serde(rename = "上级")]
    Parent,
}

impl TreeRoot {
    /// 配置里写的那个记号。指纹与展示都用它，省得同一个字符串在两处各写一遍。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Anchor => "锚目录",
            Self::Parent => "上级",
        }
    }
}

/// 一条成型规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// 规则名，平台那边引用它。
    pub name: String,
    /// 方式。
    pub kind: ShapeKind,
    /// 同名成组：哪些扩展名参与成组。
    pub group_extensions: Vec<String>,
    /// 同名成组：主文件按这个顺序挑，越靠前越优先。
    pub main_priority: Vec<String>,
    /// 同名成组：多轨光盘的轨道标记，剥掉之后才和表单同名。
    pub track_markers: Vec<Pattern>,
    /// 目录树：目录名长这样就是锚。
    pub anchor_dirs: Vec<Pattern>,
    /// 目录树：目录底下有这个相对路径的文件就是锚。
    pub anchor_files: Vec<String>,
    /// 目录树：从锚往上走时可以穿过的分区目录名。
    pub partition_dirs: Vec<String>,
    /// 目录树：这些目录下面的东西是**附属内容**而不是普通的内部资源。
    pub extra_content_dirs: Vec<String>,
    /// 目录树：变体根落在哪。
    pub tree_root: TreeRoot,
    /// 多碟同族：碟片标记。
    pub disc_markers: Vec<Pattern>,
}

/// 一个平台。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Platform {
    /// 规范名，报告与中立库里用它。
    pub name: String,
    /// 认哪些顶层目录名（已折成小写并规范化成 NFC）。
    pub dirs: Vec<String>,
    /// 只可能属于这个平台的扩展名（已折成小写）。
    pub extensions: Vec<String>,
    /// 用哪几条成型规则，按名字引用。
    pub rules: Vec<String>,
}

/// 一条「明确排除」的目录。
///
/// 与「还没映射」不是一回事：这些是**看过、决定不做**的。报告把两者分开说，
/// 否则每次读报告都会重新怀疑一遍是不是漏了。
#[derive(Debug, Clone)]
pub struct OutOfScopeDir {
    /// 目录名（小写、NFC）。
    pub dir: String,
    /// 为什么排除。
    pub reason: String,
}

/// 平台清单与成型规则。
#[derive(Debug, Clone)]
pub struct Manifest {
    platforms: Vec<Platform>,
    rules: Vec<Rule>,
    out_of_scope: Vec<OutOfScopeDir>,
    /// 目录名 → 平台下标。
    by_dir: BTreeMap<String, usize>,
    /// 扩展名 → 平台下标。
    by_extension: BTreeMap<String, usize>,
    /// 目录名 → 排除说明下标。
    excluded: BTreeMap<String, usize>,
}

/// 清单读不进来。
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    /// 文件打不开。
    #[error("平台清单读不出来：{path}（{source}）")]
    Io {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// TOML 解析失败。
    #[error("平台清单 {path} 解析失败：{source}")]
    Parse {
        /// 出问题的文件。
        path: String,
        /// 底层错误。
        source: Box<toml::de::Error>,
    },
    /// 版本对不上。
    #[error("平台清单 {path} 的版本是 {found}，本程序认得的是 {expected}")]
    Version {
        /// 出问题的文件。
        path: String,
        /// 文件里的版本。
        found: u32,
        /// 本程序的版本。
        expected: u32,
    },
    /// 内容自相矛盾。
    #[error("平台清单 {path} 有问题：{message}")]
    Invalid {
        /// 出问题的文件。
        path: String,
        /// 说明。
        message: String,
    },
    /// 名字模式编不出来。
    #[error("平台清单 {path} 里 {rule} 的模式有问题：{source}")]
    Pattern {
        /// 出问题的文件。
        path: String,
        /// 出问题的规则。
        rule: String,
        /// 底层错误。
        source: PatternError,
    },
}

// ── TOML 里的形状 ───────────────────────────────────────────────────────────
//
// 与上面公开的形状分开：配置里的字段是中文名、可省略，而程序里用的是校验过、
// 索引建好的形态。两者混成一个类型的话，「这个 `Vec` 校验过没有」会变成读代码才知道的事。

#[derive(Debug, Deserialize)]
struct RawManifest {
    #[serde(rename = "版本")]
    version: u32,
    #[serde(rename = "平台", default)]
    platforms: Vec<RawPlatform>,
    #[serde(rename = "成型规则", default)]
    rules: Vec<RawRule>,
    #[serde(rename = "范围外", default)]
    out_of_scope: Vec<RawOutOfScope>,
}

#[derive(Debug, Deserialize)]
struct RawPlatform {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "目录", default)]
    dirs: Vec<String>,
    #[serde(rename = "扩展名", default)]
    extensions: Vec<String>,
    #[serde(rename = "成型", default)]
    rules: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawRule {
    #[serde(rename = "名")]
    name: String,
    #[serde(rename = "方式")]
    kind: ShapeKind,
    #[serde(rename = "组内扩展名", default)]
    group_extensions: Vec<String>,
    #[serde(rename = "主文件优先", default)]
    main_priority: Vec<String>,
    #[serde(rename = "轨道标记", default)]
    track_markers: Vec<String>,
    #[serde(rename = "锚目录", default)]
    anchor_dirs: Vec<String>,
    #[serde(rename = "锚文件", default)]
    anchor_files: Vec<String>,
    #[serde(rename = "分区目录", default)]
    partition_dirs: Vec<String>,
    #[serde(rename = "附属内容目录", default)]
    extra_content_dirs: Vec<String>,
    #[serde(rename = "变体根", default)]
    tree_root: TreeRoot,
    #[serde(rename = "碟片标记", default)]
    disc_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct RawOutOfScope {
    #[serde(rename = "目录")]
    dirs: Vec<String>,
    #[serde(rename = "理由")]
    reason: String,
}

// 目录名与扩展名一律过 `path::fold`（小写 + NFC）之后才比较，理由见那个函数。

impl Manifest {
    /// 内置的那一份。
    ///
    /// # Panics
    /// 内置清单编不出来说明这次构建本身是坏的，直接 panic 好过让工具带着半份清单跑。
    #[must_use]
    pub fn builtin() -> Self {
        Self::parse(BUILTIN, "（内置）").expect("内置平台清单必须是好的")
    }

    /// 内置清单的原文。
    ///
    /// 「新增一个平台不需要改代码」这句话要落地，用户得先拿到一份能照着改的底稿——
    /// `romcat platforms --dump-builtin` 写的就是它。
    #[must_use]
    pub fn builtin_text() -> &'static str {
        BUILTIN
    }

    /// 从一份 TOML 文本读出清单。`path` 只用于报错。
    ///
    /// # Errors
    /// 解析失败、版本对不上、引用了不存在的规则、模式编不出来时返回错误。
    pub fn parse(text: &str, path: &str) -> Result<Self, ManifestError> {
        let raw: RawManifest = toml::from_str(text).map_err(|source| ManifestError::Parse {
            path: path.to_string(),
            source: Box::new(source),
        })?;
        if raw.version != MANIFEST_VERSION {
            return Err(ManifestError::Version {
                path: path.to_string(),
                found: raw.version,
                expected: MANIFEST_VERSION,
            });
        }

        let invalid = |message: String| ManifestError::Invalid {
            path: path.to_string(),
            message,
        };

        let mut rules = Vec::with_capacity(raw.rules.len());
        for raw_rule in raw.rules {
            let compile = |sources: Vec<String>| -> Result<Vec<Pattern>, ManifestError> {
                sources
                    .iter()
                    .map(|source| {
                        Pattern::new(source).map_err(|source| ManifestError::Pattern {
                            path: path.to_string(),
                            rule: raw_rule.name.clone(),
                            source,
                        })
                    })
                    .collect()
            };
            if rules.iter().any(|r: &Rule| r.name == raw_rule.name) {
                return Err(invalid(format!("成型规则「{}」出现了两次", raw_rule.name)));
            }
            let rule = Rule {
                name: raw_rule.name.clone(),
                kind: raw_rule.kind,
                group_extensions: raw_rule.group_extensions.iter().map(|e| fold(e)).collect(),
                main_priority: raw_rule.main_priority.iter().map(|e| fold(e)).collect(),
                track_markers: compile(raw_rule.track_markers.clone())?,
                anchor_dirs: compile(raw_rule.anchor_dirs.clone())?,
                anchor_files: raw_rule.anchor_files.iter().map(|f| fold(f)).collect(),
                partition_dirs: raw_rule.partition_dirs.iter().map(|d| fold(d)).collect(),
                extra_content_dirs: raw_rule
                    .extra_content_dirs
                    .iter()
                    .map(|d| fold(d))
                    .collect(),
                tree_root: raw_rule.tree_root,
                disc_markers: compile(raw_rule.disc_markers.clone())?,
            };
            // 一条规则宣称自己是目录树却一个锚都没有，会静悄悄地什么都不成型。
            if rule.kind == ShapeKind::DirectoryTree
                && rule.anchor_dirs.is_empty()
                && rule.anchor_files.is_empty()
            {
                return Err(invalid(format!(
                    "成型规则「{}」是目录树，却既没有锚目录也没有锚文件",
                    raw_rule.name
                )));
            }
            if rule.kind == ShapeKind::DiscFamily && rule.disc_markers.is_empty() {
                return Err(invalid(format!(
                    "成型规则「{}」是多碟同族，却一个碟片标记都没有",
                    raw_rule.name
                )));
            }
            rules.push(rule);
        }

        let mut platforms = Vec::with_capacity(raw.platforms.len());
        let mut by_dir = BTreeMap::new();
        let mut by_extension = BTreeMap::new();
        for raw_platform in raw.platforms {
            let index = platforms.len();
            if platforms
                .iter()
                .any(|p: &Platform| p.name == raw_platform.name)
            {
                return Err(invalid(format!("平台「{}」出现了两次", raw_platform.name)));
            }
            for name in &raw_platform.rules {
                if !rules.iter().any(|rule| rule.name == *name) {
                    return Err(invalid(format!(
                        "平台「{}」引用了不存在的成型规则「{name}」",
                        raw_platform.name
                    )));
                }
            }
            let dirs: Vec<String> = raw_platform.dirs.iter().map(|d| fold(d)).collect();
            for dir in &dirs {
                // 一个目录归两个平台，扫描时只能二选一，而选哪个取决于清单里的顺序——
                // 那是读者看不出来的。
                if let Some(previous) = by_dir.insert(dir.clone(), index) {
                    return Err(invalid(format!(
                        "目录「{dir}」同时归「{}」与「{}」",
                        platforms[previous].name, raw_platform.name
                    )));
                }
            }
            let extensions: Vec<String> = raw_platform.extensions.iter().map(|e| fold(e)).collect();
            for extension in &extensions {
                if let Some(previous) = by_extension.insert(extension.clone(), index) {
                    return Err(invalid(format!(
                        "扩展名「{extension}」同时归「{}」与「{}」——\
                         「只可能属于这个平台」这句话就不成立了，两边都该去掉",
                        platforms[previous].name, raw_platform.name
                    )));
                }
            }
            platforms.push(Platform {
                name: raw_platform.name,
                dirs,
                extensions,
                rules: raw_platform.rules,
            });
        }

        let mut out_of_scope = Vec::new();
        let mut excluded = BTreeMap::new();
        for raw_entry in raw.out_of_scope {
            for dir in raw_entry.dirs {
                let dir = fold(&dir);
                if by_dir.contains_key(&dir) {
                    return Err(invalid(format!("目录「{dir}」既归了平台又写在范围外")));
                }
                excluded.insert(dir.clone(), out_of_scope.len());
                out_of_scope.push(OutOfScopeDir {
                    dir,
                    reason: raw_entry.reason.clone(),
                });
            }
        }

        Ok(Self {
            platforms,
            rules,
            out_of_scope,
            by_dir,
            by_extension,
            excluded,
        })
    }

    /// 从磁盘上一份 TOML 文件读出清单。
    ///
    /// # Errors
    /// 文件读不出来，或内容有问题时返回错误。
    pub fn load(file: &Path) -> Result<Self, ManifestError> {
        let display = path::display(file);
        let text = std::fs::read_to_string(file).map_err(|source| ManifestError::Io {
            path: display.clone(),
            source,
        })?;
        Self::parse(&text, &display)
    }

    /// 全部平台，按清单里的顺序。
    #[must_use]
    pub fn platforms(&self) -> &[Platform] {
        &self.platforms
    }

    /// 全部成型规则，按清单里的顺序。
    #[must_use]
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// 全部明确排除的目录。
    #[must_use]
    pub fn out_of_scope(&self) -> &[OutOfScopeDir] {
        &self.out_of_scope
    }

    /// 这个顶层目录名归哪个平台；没映射时是 `None`。
    #[must_use]
    pub fn platform_for_dir(&self, dir: &str) -> Option<&Platform> {
        self.by_dir
            .get(&fold(dir))
            .map(|index| &self.platforms[*index])
    }

    /// 这个扩展名只可能属于哪个平台；说不准时是 `None`。
    ///
    /// 说不准的一律给 `None`——`.iso` / `.bin` / `.zip` 跨平台，硬给一个答案会让
    /// 「目录与内容冲突」这份清单里全是噪音。
    #[must_use]
    pub fn platform_for_extension(&self, extension: &str) -> Option<&Platform> {
        self.by_extension
            .get(&fold(extension))
            .map(|index| &self.platforms[*index])
    }

    /// 这个顶层目录是不是**明确排除**的；是的话给出理由。
    #[must_use]
    pub fn exclusion_reason(&self, dir: &str) -> Option<&str> {
        self.excluded
            .get(&fold(dir))
            .map(|index| self.out_of_scope[*index].reason.as_str())
    }

    /// 这份清单的指纹。
    ///
    /// 中立库记下**成型时用的是哪一份清单**，报告据此说得出「这份变体表是用另一份清单
    /// 成的型」。少了它会出现一种静默的错位：用清单 A 成型、用清单 B 出报告，
    /// 平台那张表按 B 分组、变体数按 A 算，两边对不上而报告什么都不说。
    ///
    /// 只要「不同的清单大概率不同指纹」，不需要密码学强度，用的是 FNV-1a。
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |text: &str| {
            for byte in text.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
            hash ^= 0xff;
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        };
        for platform in &self.platforms {
            eat(&platform.name);
            for dir in &platform.dirs {
                eat(dir);
            }
            for extension in &platform.extensions {
                eat(extension);
            }
            for rule in &platform.rules {
                eat(rule);
            }
        }
        for rule in &self.rules {
            eat(&rule.name);
            eat(rule.kind.label());
            for pattern in rule
                .anchor_dirs
                .iter()
                .chain(&rule.disc_markers)
                .chain(&rule.track_markers)
            {
                eat(pattern.source());
            }
            for text in rule
                .anchor_files
                .iter()
                .chain(&rule.partition_dirs)
                .chain(&rule.extra_content_dirs)
                .chain(&rule.group_extensions)
                .chain(&rule.main_priority)
            {
                eat(text);
            }
            eat(rule.tree_root.label());
        }
        for dir in &self.out_of_scope {
            eat(&dir.dir);
        }
        hash
    }

    /// 按名字取一条成型规则。
    #[must_use]
    pub fn rule(&self, name: &str) -> Option<&Rule> {
        self.rules.iter().find(|rule| rule.name == name)
    }

    /// 一个平台按方式取它的成型规则。
    ///
    /// 平台没声明这一方式时是 `None`——那就走兜底：一个文件一个变体。
    #[must_use]
    pub fn rule_of(&self, platform: &Platform, kind: ShapeKind) -> Option<&Rule> {
        platform
            .rules
            .iter()
            .filter_map(|name| self.rule(name))
            .find(|rule| rule.kind == kind)
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 内置清单编得出来且平台不重不漏() {
        let manifest = Manifest::builtin();
        assert!(manifest.platforms().len() >= 26, "议定的平台清单是 26 项");
        assert_eq!(
            manifest.platform_for_dir("FC").map(|p| p.name.as_str()),
            Some("FC")
        );
        assert_eq!(
            manifest.platform_for_dir("psv").map(|p| p.name.as_str()),
            Some("PSV"),
            "目录名大小写不敏感"
        );
        assert_eq!(
            manifest.platform_for_dir("杂志"),
            None,
            "没映射的目录不归任何平台"
        );
    }

    #[test]
    fn 明确排除与还没映射分得开() {
        let manifest = Manifest::builtin();
        assert!(
            manifest.exclusion_reason("pc").is_some(),
            "pc 是看过之后决定不做的"
        );
        assert_eq!(manifest.exclusion_reason("杂志"), None, "这个只是还没映射");
        assert_eq!(manifest.platform_for_dir("pc"), None, "排除的目录不归平台");
    }

    #[test]
    fn 扩展名只在说得准时才给平台() {
        let manifest = Manifest::builtin();
        assert_eq!(
            manifest
                .platform_for_extension("nds")
                .map(|p| p.name.as_str()),
            Some("NDS")
        );
        for 说不准 in ["iso", "bin", "cue", "zip", "7z", "chd", "gb", "gbc"] {
            assert_eq!(
                manifest.platform_for_extension(说不准),
                None,
                "{说不准} 跨平台，硬给答案只会让冲突清单全是噪音"
            );
        }
    }

    #[test]
    fn 平台按方式取到自己的成型规则() {
        let manifest = Manifest::builtin();
        let psv = manifest.platform_for_dir("PSV").expect("有 PSV");
        let rule = manifest
            .rule_of(psv, ShapeKind::DirectoryTree)
            .expect("PSV 有目录树规则");
        assert_eq!(rule.tree_root, TreeRoot::Parent);
        assert!(rule.partition_dirs.iter().any(|d| d == "app"));
        assert!(rule.extra_content_dirs.iter().any(|d| d == "addcont"));

        let fc = manifest.platform_for_dir("FC").expect("有 FC");
        assert!(
            manifest.rule_of(fc, ShapeKind::DirectoryTree).is_none(),
            "FC 没有目录树规则，走一个文件一个变体的兜底"
        );
    }

    #[test]
    fn 改了清单指纹就变() {
        let 内置 = Manifest::builtin();
        let 加了一个 = Manifest::parse(
            &format!(
                "{}\n[[\"平台\"]]\n\"名\" = \"假想机\"\n\"目录\" = [\"fictional\"]\n",
                Manifest::builtin_text()
            ),
            "（测试）",
        )
        .expect("编得出来");
        assert_ne!(内置.fingerprint(), 加了一个.fingerprint());
        assert_eq!(
            内置.fingerprint(),
            Manifest::builtin().fingerprint(),
            "同一份清单指纹稳定"
        );
    }

    #[test]
    fn 加一个平台只要改配置() {
        // 这条测试就是「新增一个平台不需要改代码」那条验收的可执行形态。
        let text = r#"
"版本" = 1
[["成型规则"]]
"名" = "假想目录树"
"方式" = "目录树"
"锚目录" = ["GAME@#"]
"变体根" = "上级"
[["平台"]]
"名" = "假想机"
"目录" = ["fictional"]
"扩展名" = ["fic"]
"成型" = ["假想目录树"]
"#;
        let manifest = Manifest::parse(text, "（测试）").expect("编得出来");
        let platform = manifest.platform_for_dir("Fictional").expect("认得出来");
        assert_eq!(platform.name, "假想机");
        assert_eq!(
            manifest
                .platform_for_extension("FIC")
                .map(|p| p.name.as_str()),
            Some("假想机")
        );
        assert!(
            manifest
                .rule_of(platform, ShapeKind::DirectoryTree)
                .is_some()
        );
    }

    fn 编不出来(text: &str) -> String {
        Manifest::parse(text, "（测试）")
            .expect_err("该报错")
            .to_string()
    }

    #[test]
    fn 版本对不上直接报错() {
        let message = 编不出来("版本 = 99");
        assert!(message.contains("版本"), "{message}");
    }

    #[test]
    fn 一个目录归两个平台要报错而不是随缘取一个() {
        let message = 编不出来(
            r#"
"版本" = 1
[["平台"]]
"名" = "甲"
"目录" = ["x"]
[["平台"]]
"名" = "乙"
"目录" = ["X"]
"#,
        );
        assert!(message.contains("同时归"), "{message}");
    }

    #[test]
    fn 一个扩展名归两个平台要报错() {
        let message = 编不出来(
            r#"
"版本" = 1
[["平台"]]
"名" = "甲"
"目录" = ["a"]
"扩展名" = ["rom"]
[["平台"]]
"名" = "乙"
"目录" = ["b"]
"扩展名" = ["rom"]
"#,
        );
        assert!(message.contains("扩展名"), "{message}");
    }

    #[test]
    fn 引用不存在的成型规则要报错() {
        let message = 编不出来(
            r#"
"版本" = 1
[["平台"]]
"名" = "甲"
"目录" = ["a"]
"成型" = ["没这条"]
"#,
        );
        assert!(message.contains("不存在"), "{message}");
    }

    #[test]
    fn 没有锚的目录树规则要报错() {
        let message = 编不出来(
            r#"
"版本" = 1
[["成型规则"]]
"名" = "空的"
"方式" = "目录树"
"#,
        );
        assert!(message.contains("锚"), "{message}");
    }

    #[test]
    fn 既归平台又写在范围外要报错() {
        let message = 编不出来(
            r#"
"版本" = 1
[["平台"]]
"名" = "甲"
"目录" = ["pc"]
[["范围外"]]
"目录" = ["pc"]
"理由" = "说不清"
"#,
        );
        assert!(message.contains("范围外"), "{message}");
    }
}
