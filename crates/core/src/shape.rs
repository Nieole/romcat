//! **成型**：把中立库里散落的条目按平台声明的规则聚成**变体**。
//!
//! ## 为什么成型跑在中立库上而不是遍历途中
//!
//! 一条成型规则要看的是**一整棵子树**：PSV 的 `app/<TitleID>` 与 `patch/<TitleID>` 是
//! 同一个变体的两半，`游戏.cue` 与 `游戏 (Track 02).bin` 是同一个变体的两块。而遍历是
//! 多线程、乱序、可中断的（`crate::scan`），一个工作线程手上只有一层目录。把成型塞进
//! 遍历里，就得让线程之间互相等对方的结果——那正是票 01 用队列避开的东西。
//!
//! 跑在中立库上还白拿两件事：**它是键的纯函数**，因此完全测得动，不必碰磁盘；
//! **改一条规则不必重扫 8.6 TiB**，`romcat shape` 重跑一遍就是了。
//!
//! ## 三步，顺序固定
//!
//! 1. **目录树**——一整棵子树是一个变体。PSV 的转储、`PS3_GAME` 所在目录、Wii U 的
//!    loadiine 目录。里面的文件是**内部资源**，不各自成条目。
//! 2. **同名成组**——同目录、同名的一组文件是一个变体。`.cue` 加它引用的 `.bin`。
//! 3. **多碟同族**——剥掉碟片标记之后同名的几个变体合成一个。
//!
//! 顺序是有依赖的，因此**不交给配置**（见 [`crate::platform`]）：目录树认下的子树里，
//! 文件已经是内部资源，不该再被同名成组捡一遍；而多碟同族合并的是前两步的产物。
//!
//! 三步都没认领的**内容**文件走兜底：一个文件一个变体。媒体、文档、系统垃圾不成变体
//! ——它们在库体检里另有归属，硬塞进变体只会让「库里有多少个可玩的东西」这个数说谎。
//!
//! ## 范围边界
//!
//! 未映射到平台的顶层目录**整体不成型**（ADR-0011 修订段）：真库顶层 73 个条目里
//! 混着 `存档`、`插件`、`杂志`、散落的裸归档，用户明确决定那部分手动处理。
//! 它们照样进**库体检**——报告的覆盖面本来就大于识别的覆盖面。

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::catalog::{Catalog, CatalogError};
use crate::classify::{self, Category};
use crate::container::volume;
use crate::path::{self, fold, platform_dir_of_key};
use crate::platform::{Manifest, Platform, Rule, ShapeKind, TreeRoot};

/// 成型器看到的一条中立库记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// 中立库的键（相对根、NFC）。
    pub key: String,
    /// 是不是目录。
    pub is_dir: bool,
    /// 字节数；**`None` 是「元数据读不到」而不是「0 字节」**（ADR-0021）。
    ///
    /// 库里另有 4,317 个真正的空文件，两者混起来两个数都会说谎。用 `Option` 逼
    /// 每个用到它的地方显式挑一边——变体的容量因此是个**下界**，报告里说明白。
    pub len: Option<u64>,
}

/// 一个成员在变体里的身份。
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Role {
    /// **主文件**：代表这个变体、也用来交给模拟器启动的那一个。
    ///
    /// 目录树成型出来的变体，它的主文件是**那个目录本身**——CONTEXT 里主文件的例子
    /// 写的正是「`PS3_GAME` 所在目录」。
    Main,
    /// 附属文件：同一个变体里除主文件之外的那些。
    Companion,
    /// **内部资源**：目录树转储里的音频、贴图、封包数据。它们没有独立身份，
    /// 识别管线不为它们产生候选。
    Internal,
    /// **附属内容**：真正的追加内容，归属某个作品但不能独立运行（ADR-0013）。
    /// 入库但不导出为前端条目。
    ExtraContent,
}

impl Role {
    /// 存进中立库的短码，也是报告里用的名字。
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Main => "主文件",
            Self::Companion => "附属文件",
            Self::Internal => "内部资源",
            Self::ExtraContent => "附属内容",
        }
    }

    /// 从短码读回来；认不出时是 `None`。
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Some(match code {
            "主文件" => Self::Main,
            "附属文件" => Self::Companion,
            "内部资源" => Self::Internal,
            "附属内容" => Self::ExtraContent,
            _ => return None,
        })
    }

    /// 报告里固定的排列顺序。
    #[must_use]
    pub fn all() -> [Self; 4] {
        [
            Self::Main,
            Self::Companion,
            Self::Internal,
            Self::ExtraContent,
        ]
    }
}

/// 一个成型出来的**变体**。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variant {
    /// 变体的键：主文件的键，或者整棵目录树的根。
    pub key: String,
    /// 所属平台；**可空**——认不出平台的内容照常入库（`docs/platforms.md`）。
    pub platform: Option<String>,
    /// 哪条成型规则成的型。人工纠正出来的是 [`MANUAL_RULE`]。
    pub rule: String,
    /// 主文件的键。
    pub main_key: String,
    /// 是不是人工纠正出来的。
    pub manual: bool,
    /// 文件成员数（不含目录）。
    pub files: u64,
    /// 文件成员的字节合计。**这是个下界**：元数据读不到的成员按 0 计入（ADR-0021）。
    pub bytes: u64,
    /// 成员里有几个元数据读不到，因此 `bytes` 少算了它们。
    pub unreadable_files: u64,
    /// 全部成员，按键排序。
    pub members: Vec<(String, Role)>,
}

/// 人工纠正出来的变体，`rule` 记这个。
pub const MANUAL_RULE: &str = "人工纠正";

/// 兜底：一个文件一个变体。
pub const SINGLE_FILE_RULE: &str = "一文件一变体";

/// **分卷压缩**：一组分卷是一个变体，主文件是入口卷。
pub const SPLIT_VOLUME_RULE: &str = "分卷压缩";

/// 成型的产物。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// 成型出来的变体，按键排序。
    pub variants: Vec<Variant>,
    /// 在范围内、却没有成为任何变体的文件数（媒体、元数据、文档、系统垃圾）。
    pub unshaped_files: u64,
    /// 同上的字节数。
    pub unshaped_bytes: u64,
}

/// 一条键在范围里的位置。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope<'m> {
    /// 落在某个平台目录下。
    Platform(&'m Platform),
    /// 落在**明确排除**的目录下，附排除理由。
    Excluded(&'m str),
    /// 落在一个还没映射到平台的顶层目录下。
    Unmapped,
    /// 直接躺在库根下，连顶层目录都没有。它没有平台目录可依据，也就没有平台。
    RootLevel,
}

impl<'m> Scope<'m> {
    /// 这条键进不进识别管线。
    #[must_use]
    pub fn in_scope(&self) -> bool {
        matches!(self, Self::Platform(_))
    }

    /// 认出来的平台；未映射、明确排除、库根下的散文件都没有平台。
    #[must_use]
    pub fn platform(&self) -> Option<&'m Platform> {
        match self {
            Self::Platform(platform) => Some(platform),
            _ => None,
        }
    }
}

/// 一条键落在范围的哪一格。
///
/// 平台由**键的第一级目录名**给出（ADR-0011），大小写不敏感、比较前过 NFC。
#[must_use]
pub fn scope_of<'m>(manifest: &'m Manifest, key: &str) -> Scope<'m> {
    // 键的第一段是**根名**，平台目录在它后面一段（`path::library_key`）。
    let Some((head, rest)) = path::relative_of_key(key).split_once('/') else {
        return Scope::RootLevel;
    };
    if head.is_empty() || rest.is_empty() {
        return Scope::RootLevel;
    }
    if let Some(platform) = manifest.platform_for_dir(head) {
        return Scope::Platform(platform);
    }
    if let Some(reason) = manifest.exclusion_reason(head) {
        return Scope::Excluded(reason);
    }
    Scope::Unmapped
}

fn parent_of(key: &str) -> Option<&str> {
    key.rsplit_once('/').map(|(head, _)| head)
}

fn last_component(key: &str) -> &str {
    key.rsplit('/').next().unwrap_or(key)
}

/// 对着一份中立库重新成型一遍：读条目、算变体、整批换掉。
///
/// **不碰磁盘。** 改一条成型规则不必重扫 8.6 TiB，跑一次这个就够了。
/// 已有的作品与发行版链接会被保住（[`Catalog::replace_variants`]）。
///
/// `scan` 是这次成型对着的遍历代号，报告据此说得出「成型是不是比中立库旧」。
///
/// # Errors
/// 读写中立库失败时返回错误。
pub fn reshape(
    catalog: &mut Catalog,
    manifest: &Manifest,
    scan: i64,
) -> Result<Plan, CatalogError> {
    let entries = catalog.shape_entries()?;
    let overrides = catalog.shaping_overrides()?;
    let plan = plan(&entries, manifest, &overrides);
    catalog.replace_variants(&plan.variants, scan, manifest)?;
    Ok(plan)
}

/// 成型：从中立库的条目算出变体。
///
/// `overrides` 是**人工纠正**：键 → 它该归到哪个变体。它优先于一切规则，
/// 且不随重新成型消失——规则的缺陷不该永久污染库（CONTEXT 的 **成型规则** 词条）。
#[must_use]
pub fn plan(entries: &[Entry], manifest: &Manifest, overrides: &BTreeMap<String, String>) -> Plan {
    let overrides = &with_targets(overrides);
    let index = Index::build(entries);
    let tree_roots = find_tree_roots(entries, manifest, &index);
    // **分卷的多个分卷合起来才是一个容器**（CONTEXT 的「透明容器」条），落到成型上
    // 就是一组分卷一个**变体**：主文件是入口卷，其余是附属文件。先整批算出来，
    // 因为判据要看**整个目录**——单看一个 `X.zip` 断不出它是不是某一组的末段。
    let volumes = find_volume_groups(entries, manifest);

    // 键 → (变体的键, 身份)。一个文件只属于一个变体。
    let mut assigned: BTreeMap<&str, (String, Role)> = BTreeMap::new();
    let mut rule_of_variant: BTreeMap<String, String> = BTreeMap::new();
    let mut manual_variants: BTreeSet<String> = BTreeSet::new();
    let mut plan = Plan::default();
    for (root, rule) in &tree_roots {
        rule_of_variant.insert(root.clone(), rule.clone());
    }

    // 同名成组要按 (目录, 剥掉轨道标记的主干) 分桶，桶里至少两个文件才算一组。
    let mut stem_groups: BTreeMap<(String, String), Vec<&Entry>> = BTreeMap::new();

    for entry in entries {
        // **人工纠正优先于一切规则**，目录也算——一个变体的主文件可以是整个目录
        // （CONTEXT 的「主文件」词条：`PS3_GAME` 所在目录）。这一步放在目录分支之前，
        // 否则拿目录当合并目标时那条纠正会被静默丢掉。
        if let Some(target) = overrides.get(&entry.key) {
            let role = if *target == entry.key {
                Role::Main
            } else {
                Role::Companion
            };
            assigned.insert(&entry.key, (target.clone(), role));
            manual_variants.insert(target.clone());
            continue;
        }
        if entry.is_dir {
            // 目录树的根本身进成员表，身份是主文件——目录树变体的主文件就是那个目录。
            if tree_roots.contains_key(&entry.key) {
                assigned.insert(&entry.key, (entry.key.clone(), Role::Main));
            }
            continue;
        }
        let Some(platform) = scope_of(manifest, &entry.key).platform() else {
            continue;
        };

        // 落在某棵目录树里？
        if let Some(root) = enclosing_root(&entry.key, &tree_roots) {
            let rule = manifest
                .rule_of(platform, ShapeKind::DirectoryTree)
                .expect("这棵树是这个平台的规则认下的");
            let role = if is_extra_content(&entry.key, root, rule) {
                Role::ExtraContent
            } else {
                Role::Internal
            };
            assigned.insert(&entry.key, (root.to_string(), role));
            continue;
        }

        // 一组分卷？**排在目录树之后**：目录树转储里的 `root.pfs.000/001/002` 是
        // **内部资源**，它们已经归了那棵树，不该再被当成一组分卷拎出来。
        if let Some((main, role)) = volumes.get(entry.key.as_str()) {
            assigned.insert(&entry.key, (main.clone(), *role));
            rule_of_variant.insert(main.clone(), SPLIT_VOLUME_RULE.to_string());
            continue;
        }

        if let Some(rule) = manifest.rule_of(platform, ShapeKind::SameStem) {
            let name = path::file_name_of_key(&entry.key);
            if let Some(extension) = path::extension_lower(std::path::Path::new(name))
                && rule.group_extensions.contains(&extension)
            {
                let dir = parent_of(&entry.key).unwrap_or("").to_string();
                let stem = strip_markers(base_stem(name), &rule.track_markers);
                stem_groups
                    .entry((dir, fold(&stem)))
                    .or_default()
                    .push(entry);
                continue;
            }
        }

        // 兜底：内容文件自成一个变体，其余不成变体。
        if is_content(&entry.key) {
            assigned.insert(&entry.key, (entry.key.clone(), Role::Main));
            rule_of_variant.insert(entry.key.clone(), SINGLE_FILE_RULE.to_string());
        } else {
            plan.unshaped_files += 1;
            plan.unshaped_bytes += entry.len.unwrap_or(0);
        }
    }

    // 人工纠正认下的变体一律记成人工纠正，**不管规则那边是不是也认领过它**。
    // 放在循环之外是为了与条目的先后无关：循环里写的话，「先看到被并进来的那个文件」
    // 与「先看到主文件」会给出两种结果。
    for key in &manual_variants {
        rule_of_variant.insert(key.clone(), MANUAL_RULE.to_string());
    }

    // 同名成组：桶里挑主文件，其余是附属文件。
    for ((_, _), members) in stem_groups {
        let Some(main) = pick_main(&members, manifest) else {
            continue;
        };
        let rule_name = members
            .first()
            .and_then(|entry| main_rule_name(manifest, &entry.key))
            .unwrap_or_else(|| SINGLE_FILE_RULE.to_string());
        for entry in &members {
            let role = if entry.key == main {
                Role::Main
            } else {
                Role::Companion
            };
            assigned.insert(&entry.key, (main.clone(), role));
        }
        rule_of_variant.insert(main, rule_name);
    }

    let mut variants = collect(
        entries,
        &assigned,
        &rule_of_variant,
        &manual_variants,
        manifest,
    );
    merge_disc_families(&mut variants, manifest);
    variants.sort_by(|a, b| a.key.cmp(&b.key));
    plan.variants = variants;
    plan
}

/// 把「被指为合并目标的那个键」也补成一条指向自己的纠正。
///
/// 少了这一步，`纠正: 甲 → 乙` 会把甲从原来的变体里拉走、却把乙留在**它自己原来那个
/// 变体**里，于是磁盘上多出一个只有甲一个成员、主文件却写着乙的变体——一条谁也代表不了
/// 的记录。补上之后，「甲和乙是一个变体」这句话在两种写法下是同一个意思。
fn with_targets(overrides: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = overrides.clone();
    for target in overrides.values() {
        out.entry(target.clone()).or_insert_with(|| target.clone());
    }
    out
}

/// 中立库条目的几个索引，成型途中反复要查。
struct Index {
    /// 折成小写的文件键，锚文件按它查——真实转储里 `sce_sys` 有大小写两种写法。
    lower_files: HashSet<String>,
}

impl Index {
    fn build(entries: &[Entry]) -> Self {
        let mut lower_files = HashSet::with_capacity(entries.len());
        for entry in entries {
            if !entry.is_dir {
                lower_files.insert(fold(&entry.key));
            }
        }
        Self { lower_files }
    }

    fn has_file(&self, key: &str) -> bool {
        self.lower_files.contains(&fold(key))
    }
}

/// 一个锚，与它算出来的变体根。
struct Anchor<'e> {
    key: &'e str,
    root: String,
    rule: &'e str,
    partition_dirs: &'e [String],
}

/// 找出全部目录树变体的根。
///
/// 三步：认锚、**挡住合集目录**、外层的赢。中间那一步是这段代码里最要紧的，
/// 理由见 [`refuse_collection_dirs`]。
fn find_tree_roots(
    entries: &[Entry],
    manifest: &Manifest,
    index: &Index,
) -> BTreeMap<String, String> {
    let mut anchors: Vec<Anchor<'_>> = Vec::new();
    for entry in entries {
        if !entry.is_dir {
            continue;
        }
        let Some(platform) = scope_of(manifest, &entry.key).platform() else {
            continue;
        };
        let Some(rule) = manifest.rule_of(platform, ShapeKind::DirectoryTree) else {
            continue;
        };
        if !is_anchor(&entry.key, rule, index) {
            continue;
        }
        // `climb` 拿它与 `parent_of` 出来的那一截比，比的是**完整的键**，
        // 所以要带根名的那一份。
        let Some(dir) = platform_dir_of_key(&entry.key) else {
            continue;
        };
        anchors.push(Anchor {
            key: &entry.key,
            root: climb(&entry.key, rule, dir),
            rule: &rule.name,
            partition_dirs: &rule.partition_dirs,
        });
    }

    refuse_collection_dirs(&mut anchors);

    let mut roots: BTreeMap<String, String> = BTreeMap::new();
    for anchor in &anchors {
        roots
            .entry(anchor.root.clone())
            .or_insert_with(|| anchor.rule.to_string());
    }
    // 一个根落在另一个根之下就丢掉：**外层的赢**。
    // PSV 的 `addcont/<TitleID>/<追加内容 id>` 也长得像 TitleID 目录，会自己认出一个根来；
    // 不去掉的话，一份转储会裂成本体加几份附属内容好几个变体。
    let all: Vec<String> = roots.keys().cloned().collect();
    for root in &all {
        if all
            .iter()
            .any(|other| other != root && root.starts_with(&format!("{other}/")))
        {
            roots.remove(root);
        }
    }
    roots
}

/// 上溯一级如果落到了一个**合集目录**上，就退回锚本身。
///
/// 这是整条成型链路上最危险的一步，值得把话说透。真库里 PSV 的形状是
/// `PSV/PSVENJP/<游戏名>[TitleID]/app/<TitleID>/…`——`PSVENJP` 底下坐着 221 份转储。
/// 一旦有哪一份的锚落在 `<游戏名>` 那一层（比如它直接放着 `eboot.bin`，或者目录名
/// 本身就带 TitleID），`变体根 = 上级` 会把它上溯到 `PSVENJP`；而「外层的赢」紧接着
/// 把同级 220 份**已经正确成型**的转储全部吸进去——**221 份游戏塌成 1 个变体**。
/// 这比不成型还糟：不成型至少数得清有多少东西，塌了之后连数都没了。
///
/// 判据：一个候选根 `R` 底下若坐着**另一个锚**，而那个锚
///
/// - 算出来的根不是 `R`（它自成一份转储），且
/// - 不在 `R` 紧下面的**分区目录**里（`app` / `patch` / `addcont` 里的东西本来就该被 `R` 吞掉）
///
/// 那 `R` 就是个装了不止一份转储的合集目录，这个锚退回自己那一层。
fn refuse_collection_dirs(anchors: &mut [Anchor<'_>]) {
    let roots: Vec<(String, String)> = anchors
        .iter()
        .map(|anchor| (anchor.key.to_string(), anchor.root.clone()))
        .collect();
    for anchor in anchors.iter_mut() {
        if anchor.root == anchor.key {
            continue;
        }
        let 挡住 = roots.iter().any(|(other_key, other_root)| {
            if other_key == anchor.key || *other_root == anchor.root {
                return false;
            }
            let Some(relative) = other_key
                .strip_prefix(anchor.root.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
            else {
                return false;
            };
            let head = relative.split('/').next().unwrap_or("");
            !anchor.partition_dirs.contains(&fold(head))
        });
        if 挡住 {
            anchor.root = anchor.key.to_string();
        }
    }
}

fn is_anchor(dir_key: &str, rule: &Rule, index: &Index) -> bool {
    let name = last_component(dir_key);
    if rule.anchor_dirs.iter().any(|p| p.matches(name)) {
        return true;
    }
    rule.anchor_files
        .iter()
        .any(|relative| index.has_file(&format!("{dir_key}/{relative}")))
}

/// 从锚目录走到变体根。
///
/// 两步：先穿过**分区目录**（PSV 的 `app` / `patch` / `addcont` 各装着一个同名的
/// TitleID 目录，它们是同一个变体的几半），再按规则决定要不要再上一级。
///
/// **绝不上到平台目录本身。** 少了这道闸，一个直接躺在 `PSV/` 下的 TitleID 目录会
/// 把整个平台吞成一个变体——十几万个文件归到一条记录上，比不成型还糟。
fn climb(anchor: &str, rule: &Rule, platform_dir: &str) -> String {
    let mut root = anchor;
    while let Some(parent) = parent_of(root) {
        if parent == platform_dir || parent.is_empty() {
            break;
        }
        if !rule.partition_dirs.contains(&fold(last_component(parent))) {
            break;
        }
        root = parent;
    }
    if rule.tree_root == TreeRoot::Parent
        && let Some(parent) = parent_of(root)
        && parent != platform_dir
        && !parent.is_empty()
    {
        root = parent;
    }
    root.to_string()
}

/// 这条键落在哪棵目录树里。根之间已经没有嵌套，因此至多命中一个。
fn enclosing_root<'r>(key: &str, roots: &'r BTreeMap<String, String>) -> Option<&'r str> {
    let mut cursor = key;
    while let Some(parent) = parent_of(cursor) {
        if let Some((root, _)) = roots.get_key_value(parent) {
            return Some(root.as_str());
        }
        cursor = parent;
    }
    None
}

/// 变体根之下第一级是不是**附属内容目录**。
fn is_extra_content(key: &str, root: &str, rule: &Rule) -> bool {
    let Some(relative) = key.strip_prefix(root).and_then(|r| r.strip_prefix('/')) else {
        return false;
    };
    let head = relative.split('/').next().unwrap_or("");
    rule.extra_content_dirs.contains(&fold(head))
}

/// 文件名去掉扩展名的那一截。
fn base_stem(name: &str) -> &str {
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => name,
    }
}

/// 把结尾的标记一层层剥掉。
fn strip_markers(text: &str, markers: &[crate::platform::pattern::Pattern]) -> String {
    let mut current = text;
    while let Some(shorter) = markers
        .iter()
        .find_map(|marker| marker.strip_suffix(current))
    {
        let trimmed = shorter.trim_end_matches([' ', '\t', '-', '_', '.', '、']);
        // 剥到空、或者一个字符都没少，就停手——不然会一直转下去。
        if trimmed.is_empty() || trimmed.len() == current.len() {
            break;
        }
        current = trimmed;
    }
    current.to_string()
}

/// 一组同名文件里，哪一个当主文件。
fn pick_main(members: &[&Entry], manifest: &Manifest) -> Option<String> {
    let rank = |key: &str| -> usize {
        let Some(rule) = main_rule(manifest, key) else {
            return usize::MAX;
        };
        let name = path::file_name_of_key(key);
        let Some(extension) = path::extension_lower(std::path::Path::new(name)) else {
            return usize::MAX;
        };
        rule.main_priority
            .iter()
            .position(|candidate| *candidate == extension)
            .unwrap_or(usize::MAX)
    };
    members
        .iter()
        .min_by(|a, b| {
            rank(&a.key)
                .cmp(&rank(&b.key))
                // 同一档里按键排序，成型结果才与条目的先后无关。
                .then_with(|| a.key.cmp(&b.key))
        })
        .map(|entry| entry.key.clone())
}

fn main_rule<'m>(manifest: &'m Manifest, key: &str) -> Option<&'m Rule> {
    let platform = scope_of(manifest, key).platform()?;
    manifest.rule_of(platform, ShapeKind::SameStem)
}

fn main_rule_name(manifest: &Manifest, key: &str) -> Option<String> {
    main_rule(manifest, key).map(|rule| rule.name.clone())
}

/// 找出全部**分卷**组：键 → (这一组的变体键, 这一条在组里的身份)。
///
/// 分组这件事本身**不在这里做**——它住在
/// [`container::volume`](crate::container::volume)，归类、成型与穿透看的是同一份判据。
/// 这里只做成型这一侧的两件事：**按目录切开**（分卷不会散落在两个目录，而两个目录里
/// 同名的两组是两组东西），以及把组里的成员折成变体的成员表。
///
/// 挑主文件时认的是「哪一段是入口」，不是段号最小：**ZIP split 的入口是最后那个
/// `.zip`**（APPNOTE §8.3.4 说末段用 `.zip` 扩展名正是为了让中央目录一次读到）。
/// 认反了，主文件会指向一段连中央目录都没有的碎片。
fn find_volume_groups(entries: &[Entry], manifest: &Manifest) -> BTreeMap<String, (String, Role)> {
    let mut by_dir: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for entry in entries {
        if entry.is_dir || scope_of(manifest, &entry.key).platform().is_none() {
            continue;
        }
        let Some(dir) = parent_of(&entry.key) else {
            continue;
        };
        by_dir
            .entry(dir)
            .or_default()
            .push(path::file_name_of_key(&entry.key));
    }

    let mut out: BTreeMap<String, (String, Role)> = BTreeMap::new();
    for (dir, names) in by_dir {
        for group in volume::group_volumes(names.iter().copied()) {
            // 一段入口都没有（**缺入口卷**）时退而取段号最小的那一段，好过留一组
            // 谁也代表不了的碎片——识别那一侧会照实说这一组缺入口卷。
            let Some(main) = group.entry.or_else(|| group.members.first().cloned()) else {
                continue;
            };
            let main_key = format!("{dir}/{main}");
            for name in &group.members {
                let role = if *name == main {
                    Role::Main
                } else {
                    Role::Companion
                };
                out.insert(format!("{dir}/{name}"), (main_key.clone(), role));
            }
        }
    }
    out
}

/// 这个文件是不是**内容**：透明容器、压缩镜像、裸文件三类主线之一。
///
/// 媒体、元数据、文档、模拟器本体、系统垃圾都不是——它们在库体检里另有归属，
/// 硬塞进变体只会让「库里有多少个可玩的东西」这个数说谎。
fn is_content(key: &str) -> bool {
    let name = std::path::Path::new(path::file_name_of_key(key));
    let classification = classify::classify(name);
    classification.suspect.is_none()
        && matches!(
            classification.category,
            Category::TransparentContainer | Category::CompressedImage | Category::BareFile
        )
}

fn collect(
    entries: &[Entry],
    assigned: &BTreeMap<&str, (String, Role)>,
    rule_of_variant: &BTreeMap<String, String>,
    manual_variants: &BTreeSet<String>,
    manifest: &Manifest,
) -> Vec<Variant> {
    let mut by_variant: BTreeMap<&str, Variant> = BTreeMap::new();
    for entry in entries {
        let Some((variant_key, role)) = assigned.get(entry.key.as_str()) else {
            continue;
        };
        let variant = by_variant
            .entry(variant_key.as_str())
            .or_insert_with(|| Variant {
                key: variant_key.clone(),
                platform: scope_of(manifest, variant_key)
                    .platform()
                    .map(|platform| platform.name.clone()),
                rule: rule_of_variant
                    .get(variant_key)
                    .cloned()
                    .unwrap_or_else(|| SINGLE_FILE_RULE.to_string()),
                main_key: variant_key.clone(),
                manual: manual_variants.contains(variant_key),
                files: 0,
                bytes: 0,
                unreadable_files: 0,
                members: Vec::new(),
            });
        if !entry.is_dir {
            variant.files += 1;
            // 大小未知的按 0 计入容量——**变体的容量因此是个下界**，报告里说明白
            // （ADR-0021：不凭空编数字，但也不把「未知」说成「零」）。
            variant.bytes += entry.len.unwrap_or(0);
            variant.unreadable_files += u64::from(entry.len.is_none());
        }
        if *role == Role::Main {
            variant.main_key.clone_from(&entry.key);
        }
        variant.members.push((entry.key.clone(), *role));
    }
    for variant in by_variant.values_mut() {
        variant.members.sort();
        // 一个变体必须有主文件。人工纠正指向一个已经不在库里的键时，成员表里会一个
        // 主文件都没有——那时把排在最前的成员提上来，好过留一条谁也代表不了的记录。
        if !variant.members.iter().any(|(_, role)| *role == Role::Main)
            && let Some((key, role)) = variant.members.first_mut()
        {
            *role = Role::Main;
            variant.main_key.clone_from(key);
        }
    }
    by_variant.into_values().collect()
}

/// 剥掉每一级路径末尾的碟片标记之后，两条键相同的变体是同一个东西。
fn merge_disc_families(variants: &mut Vec<Variant>, manifest: &Manifest) {
    // 桶里除了下标，还记着「这一份带没带碟片标记」与该用哪条规则的名字。
    let mut families: BTreeMap<(String, String), (Vec<usize>, bool, String)> = BTreeMap::new();
    for (index, variant) in variants.iter().enumerate() {
        let Some(platform) = scope_of(manifest, &variant.key).platform() else {
            continue;
        };
        let Some(rule) = manifest.rule_of(platform, ShapeKind::DiscFamily) else {
            continue;
        };
        if variant.manual {
            continue;
        }
        let family = strip_disc_markers(&variant.key, rule);
        let bucket = families
            .entry((platform.name.clone(), family.clone()))
            .or_insert_with(|| (Vec::new(), false, rule.name.clone()));
        bucket.0.push(index);
        // 没带标记的那一份也参加分组：真实的转储常常是 `游戏.chd` 加
        // `游戏 (Disc 2).chd`——碟 1 不写标记。但**整桶都没标记就不算同族**，
        // 那时桶里本来也只可能有一份（键是唯一的）。
        bucket.1 |= family != variant.key;
    }

    let mut absorbed: BTreeSet<usize> = BTreeSet::new();
    let mut merged: Vec<Variant> = Vec::new();
    for (_, (group, has_marker, rule_name)) in families {
        if group.len() < 2 || !has_marker {
            continue;
        }
        // 键最小的那一份当代表：碟 1 排在碟 2 前面，结果与条目的先后无关。
        let mut keeper = variants[group[0]].clone();
        keeper.rule = rule_name;
        for index in &group {
            absorbed.insert(*index);
            if *index == group[0] {
                continue;
            }
            let other = &variants[*index];
            keeper.files += other.files;
            keeper.bytes += other.bytes;
            keeper.unreadable_files += other.unreadable_files;
            for (key, role) in &other.members {
                // 别的碟的主文件降成附属文件：一个变体只有一个主文件。
                let role = if *role == Role::Main {
                    Role::Companion
                } else {
                    *role
                };
                keeper.members.push((key.clone(), role));
            }
        }
        keeper.members.sort();
        merged.push(keeper);
    }
    if absorbed.is_empty() {
        return;
    }
    let mut kept: Vec<Variant> = variants
        .drain(..)
        .enumerate()
        .filter(|(index, _)| !absorbed.contains(index))
        .map(|(_, variant)| variant)
        .collect();
    kept.append(&mut merged);
    *variants = kept;
}

fn strip_disc_markers(key: &str, rule: &Rule) -> String {
    let parts: Vec<&str> = key.split('/').collect();
    let last = parts.len().saturating_sub(1);
    parts
        .iter()
        .enumerate()
        .map(|(index, part)| {
            // 最后一级留着扩展名：`游戏 (Disc 1).chd` 的标记在扩展名**前面**，
            // 不先摘掉扩展名的话「只认结尾」这条规则永远匹配不上。
            if index == last
                && let Some((stem, extension)) = part.rsplit_once('.')
                && !stem.is_empty()
            {
                return format!("{}.{extension}", strip_markers(stem, &rule.disc_markers));
            }
            strip_markers(part, &rule.disc_markers)
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn 清单() -> Manifest {
        Manifest::builtin()
    }

    fn 目录(key: &str) -> Entry {
        Entry {
            key: key.to_string(),
            is_dir: true,
            len: None,
        }
    }

    fn 文件(key: &str, len: u64) -> Entry {
        Entry {
            key: key.to_string(),
            is_dir: false,
            len: Some(len),
        }
    }

    /// 测试里那个根叫什么。**键的第一段是根名**（`path::library_key`），
    /// 所以这几条测试跑的是真库上那条路：`库/FC/…` 而不是 `FC/…`。
    const 根: &str = "库";

    /// 给一条相对根的路径接上根名。
    fn 键(相对: &str) -> String {
        path::join_root(根, 相对)
    }

    /// 补齐一条键路上的每一级目录，省得每条测试都手写一串。
    ///
    /// 传进来的是**相对根**的路径，这里替它接上根名——于是每条测试写的还是
    /// `FC/甲.zip` 这种一眼看得懂的东西，跑的却是带根名的真键。
    fn 建条目(files: &[(&str, u64)]) -> Vec<Entry> {
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        let mut entries: Vec<Entry> = Vec::new();
        entries.push(目录(根));
        for (key, len) in files {
            let key = 键(key);
            let mut cursor = key.as_str();
            while let Some(parent) = parent_of(cursor) {
                dirs.insert(parent.to_string());
                cursor = parent;
            }
            entries.push(文件(&key, *len));
        }
        for dir in dirs {
            if dir != 根 {
                entries.push(目录(&dir));
            }
        }
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        entries
    }

    /// 成型跑完之后把根名剥掉再交出来。
    ///
    /// **这几条测试考的是成型，不是键的形状**：带根名跑一遍（那是真路），
    /// 断言里写不带根名的键（那才读得懂）。
    fn 剥根名(plan: &mut Plan) {
        let 剥 = |key: &mut String| {
            *key = path::relative_of_key(key).to_string();
        };
        for variant in &mut plan.variants {
            剥(&mut variant.key);
            剥(&mut variant.main_key);
            for (key, _) in &mut variant.members {
                剥(key);
            }
        }
    }

    fn 成型(files: &[(&str, u64)]) -> Plan {
        let mut plan = plan(&建条目(files), &清单(), &BTreeMap::new());
        剥根名(&mut plan);
        plan
    }

    fn 变体键(plan: &Plan) -> Vec<&str> {
        plan.variants.iter().map(|v| v.key.as_str()).collect()
    }

    fn 取<'p>(plan: &'p Plan, key: &str) -> &'p Variant {
        plan.variants
            .iter()
            .find(|v| v.key == key)
            .unwrap_or_else(|| panic!("有变体 {key}，实际是 {:?}", 变体键(plan)))
    }

    #[test]
    fn 一组分卷是一个变体而不是一堆孤立碎片() {
        // CONTEXT 的「透明容器」条：**分卷压缩的多个分卷合起来才是一个容器**。
        let plan = 成型(&[
            ("ps2/大作/大作.part1.rar", 4_000_000),
            ("ps2/大作/大作.part2.rar", 4_000_000),
            ("ps2/大作/大作.part3.rar", 1_000_000),
        ]);
        assert_eq!(变体键(&plan), ["ps2/大作/大作.part1.rar"]);
        let variant = 取(&plan, "ps2/大作/大作.part1.rar");
        assert_eq!(variant.rule, SPLIT_VOLUME_RULE);
        assert_eq!(
            variant.main_key, "ps2/大作/大作.part1.rar",
            "主文件是入口卷"
        );
        assert_eq!(variant.files, 3);
        assert_eq!(variant.bytes, 9_000_000);
    }

    #[test]
    fn zip_官方_split_的主文件是最后那个_zip() {
        // 最容易搞反的一条：APPNOTE §8.3.4 说末段用 `.zip` 扩展名，正是为了让中央目录
        // 一次读到。主文件指向 `.z01` 的话，那一段连中央目录都没有。
        // 真库里 WIIU 那 4 组 `XenobladeX-…-WUP.z01…z04 + .zip` 就是这一种。
        let plan = 成型(&[
            ("WIIU/异度/游戏.z01", 4_000_000),
            ("WIIU/异度/游戏.z02", 4_000_000),
            ("WIIU/异度/游戏.zip", 3_000_000),
        ]);
        assert_eq!(变体键(&plan), ["WIIU/异度/游戏.zip"]);
        let variant = 取(&plan, "WIIU/异度/游戏.zip");
        assert_eq!(variant.rule, SPLIT_VOLUME_RULE);
        assert_eq!(variant.files, 3);
    }

    #[test]
    fn 字节切分的一组也是一个变体() {
        let plan = 成型(&[("SFC/合集/资料.7z.001", 500), ("SFC/合集/资料.7z.002", 400)]);
        assert_eq!(变体键(&plan), ["SFC/合集/资料.7z.001"]);
        assert_eq!(取(&plan, "SFC/合集/资料.7z.001").files, 2);
    }

    #[test]
    fn 同目录里两组分卷不会串到一起() {
        let plan = 成型(&[
            ("ps2/甲.part1.rar", 100),
            ("ps2/甲.part2.rar", 100),
            ("ps2/乙.part1.rar", 100),
            ("ps2/乙.part2.rar", 100),
        ]);
        assert_eq!(变体键(&plan), ["ps2/乙.part1.rar", "ps2/甲.part1.rar"]);
    }

    #[test]
    fn 一个普通的容器不会被当成一组分卷() {
        // 库里 34,808 个 zip 与 1,333 个 rar 全都长着「入口候选」的样子。
        let plan = 成型(&[("FC/甲.zip", 100), ("FC/乙.zip", 100), ("FC/丙.rar", 100)]);
        assert_eq!(变体键(&plan), ["FC/丙.rar", "FC/乙.zip", "FC/甲.zip"]);
        for key in ["FC/甲.zip", "FC/丙.rar"] {
            assert_eq!(取(&plan, key).rule, SINGLE_FILE_RULE);
        }
    }

    #[test]
    fn 缺入口卷时那一组也不散成碎片() {
        // 真库里那个 `废都物语_资料合辑_220928.7z.006`：`.001` 到 `.005` 都不在库里。
        // 段号最小的那一段当主文件——好过留一组谁也代表不了的碎片；识别那一侧会照实
        // 说这一组缺入口卷。
        let plan = 成型(&[("SFC/合集/资料.7z.005", 500), ("SFC/合集/资料.7z.006", 400)]);
        assert_eq!(变体键(&plan), ["SFC/合集/资料.7z.005"]);
        assert_eq!(取(&plan, "SFC/合集/资料.7z.005").rule, SPLIT_VOLUME_RULE);
    }

    #[test]
    fn cue_与它的_bin_是一个变体() {
        let plan = 成型(&[("ps/某游戏/游戏.cue", 100), ("ps/某游戏/游戏.bin", 700_000)]);
        assert_eq!(变体键(&plan), ["ps/某游戏/游戏.cue"]);
        let variant = 取(&plan, "ps/某游戏/游戏.cue");
        assert_eq!(variant.main_key, "ps/某游戏/游戏.cue");
        assert_eq!(variant.files, 2);
        assert_eq!(variant.bytes, 700_100);
        assert_eq!(variant.platform.as_deref(), Some("PS1"));
    }

    #[test]
    fn 多轨光盘的每一条轨道都归到表单名下() {
        let plan = 成型(&[
            ("ps/某游戏/游戏.cue", 100),
            ("ps/某游戏/游戏 (Track 01).bin", 500),
            ("ps/某游戏/游戏 (Track 02).bin", 600),
        ]);
        assert_eq!(变体键(&plan), ["ps/某游戏/游戏.cue"]);
        assert_eq!(取(&plan, "ps/某游戏/游戏.cue").files, 3);
    }

    #[test]
    fn 压缩镜像旁边的表单不许把它降成附属() {
        // `.chd` 是压缩镜像，模拟器直接读它（ADR-0014）；旁边那份 `.cue` 只是转换留下的。
        let plan = 成型(&[("ps/某游戏/游戏.chd", 400_000), ("ps/某游戏/游戏.cue", 100)]);
        let variant = 取(&plan, "ps/某游戏/游戏.chd");
        assert_eq!(variant.main_key, "ps/某游戏/游戏.chd");
        assert_eq!(
            variant
                .members
                .iter()
                .find(|(key, _)| key.ends_with(".cue"))
                .map(|(_, role)| *role),
            Some(Role::Companion)
        );
    }

    #[test]
    fn 多碟同族在同一个目录里合成一个变体() {
        let plan = 成型(&[
            ("ps/某游戏/游戏 (Disc 1).cue", 100),
            ("ps/某游戏/游戏 (Disc 1).bin", 700),
            ("ps/某游戏/游戏 (Disc 2).cue", 100),
            ("ps/某游戏/游戏 (Disc 2).bin", 800),
        ]);
        assert_eq!(变体键(&plan), ["ps/某游戏/游戏 (Disc 1).cue"]);
        let variant = 取(&plan, "ps/某游戏/游戏 (Disc 1).cue");
        assert_eq!(variant.files, 4, "两张碟四个文件，一个变体");
        assert_eq!(variant.bytes, 1700);
        assert_eq!(
            variant
                .members
                .iter()
                .filter(|(_, role)| *role == Role::Main)
                .count(),
            1,
            "一个变体只有一个主文件"
        );
    }

    #[test]
    fn 多碟同族分在几个目录里也合得起来() {
        // 真库里就是这样：`…]Disc A/` 与 `…]Disc B/` 是两个兄弟目录。
        let plan = 成型(&[
            ("ps/龙骑士传说[简]Disc A/Legend (Disc 1).chd", 500),
            ("ps/龙骑士传说[简]Disc B/Legend (Disc 2).chd", 600),
        ]);
        assert_eq!(plan.variants.len(), 1, "{:?}", 变体键(&plan));
        assert_eq!(plan.variants[0].files, 2);
    }

    #[test]
    fn 名字里碰巧带碟字的两个游戏不会被并到一起() {
        let plan = 成型(&[("ps/光碟大战.chd", 100), ("ps/光碟传奇.chd", 100)]);
        assert_eq!(plan.variants.len(), 2);
    }

    #[test]
    fn ps3_的一整个目录是一个变体() {
        let plan = 成型(&[
            ("ps3/某游戏/PS3_GAME/USRDIR/EBOOT.BIN", 4000),
            ("ps3/某游戏/PS3_GAME/ICON0.PNG", 100),
            ("ps3/某游戏/PS3_UPDATE/PS3UPDAT.PUP", 200),
            ("ps3/某游戏/PARAM.SFO", 50),
        ]);
        assert_eq!(变体键(&plan), ["ps3/某游戏"]);
        let variant = 取(&plan, "ps3/某游戏");
        assert_eq!(variant.main_key, "ps3/某游戏", "主文件是 PS3_GAME 所在目录");
        assert_eq!(variant.files, 4);
        assert!(
            variant
                .members
                .iter()
                .filter(|(key, _)| key.ends_with(".PNG"))
                .all(|(_, role)| *role == Role::Internal),
            "目录里的东西是内部资源，不各自成条目"
        );
    }

    #[test]
    fn psv_的_nonpdrm_转储整棵树成一个变体() {
        let plan = 成型(&[
            (
                "PSV/PSVENJP/零之轨迹[PCSG00042]/app/PCSG00042/eboot.bin",
                900,
            ),
            (
                "PSV/PSVENJP/零之轨迹[PCSG00042]/app/PCSG00042/data.psarc",
                1_000_000,
            ),
            (
                "PSV/PSVENJP/零之轨迹[PCSG00042]/app/PCSG00042/sce_sys/param.sfo",
                700,
            ),
            (
                "PSV/PSVENJP/零之轨迹[PCSG00042]/app/PCSG00042/bgm/a.at9",
                4000,
            ),
            (
                "PSV/PSVENJP/零之轨迹[PCSG00042]/patch/PCSG00042/eboot.bin",
                900,
            ),
        ]);
        assert_eq!(变体键(&plan), ["PSV/PSVENJP/零之轨迹[PCSG00042]"]);
        let variant = 取(&plan, "PSV/PSVENJP/零之轨迹[PCSG00042]");
        assert_eq!(variant.files, 5, "app 与 patch 是同一个变体的两半");
        assert_eq!(variant.platform.as_deref(), Some("PSV"));
    }

    #[test]
    fn psv_的_maidump_转储也成一个变体() {
        let plan = 成型(&[
            ("PSV/PSVENJP/某游戏[PCSG00162]/PCSG00162/eboot.bin", 900),
            ("PSV/PSVENJP/某游戏[PCSG00162]/PCSG00162/data.psarc", 5000),
            (
                "PSV/PSVENJP/某游戏[PCSG00162]/PCSG00162_patch/eboot.bin",
                900,
            ),
        ]);
        assert_eq!(变体键(&plan), ["PSV/PSVENJP/某游戏[PCSG00162]"]);
        assert_eq!(取(&plan, "PSV/PSVENJP/某游戏[PCSG00162]").files, 3);
    }

    #[test]
    fn psv_把_titleid_写在目录名里的那种也认得() {
        let plan = 成型(&[("PSV/AIME00001(wan华镜 v3.1)/AIME00001.tar.zst", 300_000)]);
        assert_eq!(变体键(&plan), ["PSV/AIME00001(wan华镜 v3.1)"]);
    }

    #[test]
    fn psv_的_vpk_与透明容器各自是一个变体() {
        let plan = 成型(&[
            ("PSV/去月球/ToTheMoon.vpk", 200_000),
            ("PSV/虚之少女-PSV-V1.0版.7z", 900_000),
        ]);
        let mut keys = 变体键(&plan);
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["PSV/去月球/ToTheMoon.vpk", "PSV/虚之少女-PSV-V1.0版.7z"]
        );
    }

    #[test]
    fn psv_的_addcont_子树标成附属内容而不是普通内部资源() {
        let plan = 成型(&[
            ("PSV/PSVENJP/某游戏[PCSE00372]/app/PCSE00372/eboot.bin", 900),
            (
                "PSV/PSVENJP/某游戏[PCSE00372]/addcont/PCSE00372/TWDS20000000DLC2/x.psarc",
                4000,
            ),
        ]);
        assert_eq!(plan.variants.len(), 1, "{:?}", 变体键(&plan));
        let variant = &plan.variants[0];
        assert_eq!(
            variant
                .members
                .iter()
                .find(|(key, _)| key.contains("addcont"))
                .map(|(_, role)| *role),
            Some(Role::ExtraContent),
            "追加内容入库、算进变体，但不导出为前端条目（ADR-0013）"
        );
    }

    #[test]
    fn 合集目录不许把同级的几份转储吞成一个变体() {
        // 真库里 `PSV/PSVENJP` 底下坐着 221 份转储。只要有一份的锚落在游戏名那一层
        // （这里是目录名自带 TitleID 的那种），`变体根 = 上级` 就会上溯到 `PSVENJP`，
        // 「外层的赢」再把同级已经正确成型的那些全吸进去——**几百个游戏塌成一个变体**。
        let plan = 成型(&[
            ("PSV/PSVENJP/甲[PCSG00042]/app/PCSG00042/eboot.bin", 100),
            ("PSV/PSVENJP/乙[PCSG00043]/app/PCSG00043/eboot.bin", 100),
            ("PSV/PSVENJP/AIME00001(丙)/data.psarc", 100),
        ]);
        let mut keys = 变体键(&plan);
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "PSV/PSVENJP/AIME00001(丙)",
                "PSV/PSVENJP/乙[PCSG00043]",
                "PSV/PSVENJP/甲[PCSG00042]",
            ],
            "三份转储三个变体，`PSVENJP` 绝不许成为变体根"
        );
    }

    #[test]
    fn 同级的几份转储各归各的分区目录照样吞得下() {
        // 上一条那道守卫不许误伤正常形态：`app` / `patch` / `addcont` 里的东西
        // 本来就该被同一个变体吞掉。
        let plan = 成型(&[
            ("PSV/PSVENJP/甲[PCSG00042]/app/PCSG00042/eboot.bin", 100),
            ("PSV/PSVENJP/甲[PCSG00042]/patch/PCSG00042/eboot.bin", 100),
            (
                "PSV/PSVENJP/甲[PCSG00042]/addcont/PCSG00042/TWDS20000000DLC2/x.psarc",
                100,
            ),
            ("PSV/PSVENJP/乙[PCSG00162]/PCSG00162/eboot.bin", 100),
            ("PSV/PSVENJP/乙[PCSG00162]/PCSG00162_patch/eboot.bin", 100),
        ]);
        let mut keys = 变体键(&plan);
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["PSV/PSVENJP/乙[PCSG00162]", "PSV/PSVENJP/甲[PCSG00042]"]
        );
        assert_eq!(取(&plan, "PSV/PSVENJP/甲[PCSG00042]").files, 3);
        assert_eq!(取(&plan, "PSV/PSVENJP/乙[PCSG00162]").files, 2);
    }

    #[test]
    fn 碟一不带标记时照样与后面几张合成一个变体() {
        // 真实转储常常是「碟 1 不写标记，碟 2 才写」。
        let plan = 成型(&[
            ("ps/某游戏/游戏.chd", 500),
            ("ps/某游戏/游戏 (Disc 2).chd", 600),
        ]);
        assert_eq!(plan.variants.len(), 1, "{:?}", 变体键(&plan));
        assert_eq!(plan.variants[0].files, 2);
    }

    #[test]
    fn 一整桶都没有碟片标记时不算同族() {
        let plan = 成型(&[("ps/甲.chd", 100), ("ps/乙.chd", 100)]);
        assert_eq!(plan.variants.len(), 2);
    }

    #[test]
    fn 元数据读不到的成员不当成零字节() {
        // ADR-0021：库里另有 4,317 个真正的空文件，两者混起来两个数都会说谎。
        let entries = vec![
            目录(根),
            目录(&键("FC")),
            Entry {
                key: 键("FC/读不到.zip"),
                is_dir: false,
                len: None,
            },
            文件(&键("FC/空的.zip"), 0),
        ];
        let mut plan = plan(&entries, &清单(), &BTreeMap::new());
        剥根名(&mut plan);
        let 读不到 = 取(&plan, "FC/读不到.zip");
        assert_eq!(读不到.files, 1);
        assert_eq!(读不到.bytes, 0, "未知大小按 0 计入，容量是个下界");
        assert_eq!(读不到.unreadable_files, 1, "少算了几个要说得出来");
        assert_eq!(
            取(&plan, "FC/空的.zip").unreadable_files,
            0,
            "真的 0 字节不算读不到"
        );
    }

    #[test]
    fn 人工纠正也能拿目录当合并目标() {
        // 一个变体的主文件可以是整个目录（CONTEXT 的「主文件」词条）。
        let entries = 建条目(&[
            ("ps3/某游戏/PS3_GAME/EBOOT.BIN", 100),
            ("ps3/散落的补丁.zip", 50),
        ]);
        let mut overrides = BTreeMap::new();
        overrides.insert(键("ps3/某游戏"), 键("ps3/某游戏"));
        overrides.insert(键("ps3/散落的补丁.zip"), 键("ps3/某游戏"));
        let mut plan = plan(&entries, &清单(), &overrides);
        剥根名(&mut plan);
        let variant = 取(&plan, "ps3/某游戏");
        assert!(variant.manual);
        assert_eq!(variant.main_key, "ps3/某游戏");
        assert!(
            variant
                .members
                .iter()
                .any(|(key, _)| key == "ps3/散落的补丁.zip"),
            "{:?}",
            variant.members
        );
    }

    #[test]
    fn 人工纠正指向一个不在库里的键时不留下没有主文件的变体() {
        let entries = 建条目(&[("FC/甲.zip", 100), ("FC/乙.zip", 200)]);
        let mut overrides = BTreeMap::new();
        overrides.insert(键("FC/甲.zip"), 键("FC/早就删了.zip"));
        overrides.insert(键("FC/乙.zip"), 键("FC/早就删了.zip"));
        let mut plan = plan(&entries, &清单(), &overrides);
        剥根名(&mut plan);
        let variant = 取(&plan, "FC/早就删了.zip");
        assert_eq!(
            variant
                .members
                .iter()
                .filter(|(_, role)| *role == Role::Main)
                .count(),
            1,
            "排在最前的成员被提成主文件，不留一条谁也代表不了的记录"
        );
        assert_eq!(
            variant.main_key, "FC/乙.zip",
            "提上来的是排在最前的那个成员，与条目的先后无关"
        );
    }

    #[test]
    fn 人工纠正能造出平台为空的变体() {
        // 「变体的平台属性可空」那条验收的可执行形态：把两个范围之外的条目并成一个
        // 变体，它没有平台，照样入库（`docs/platforms.md`「其他掌机是兜底桶」）。
        let entries = 建条目(&[("杂志/甲.cbz", 100), ("杂志/乙.cbz", 200)]);
        let mut overrides = BTreeMap::new();
        overrides.insert(键("杂志/甲.cbz"), 键("杂志/甲.cbz"));
        overrides.insert(键("杂志/乙.cbz"), 键("杂志/甲.cbz"));
        let mut plan = plan(&entries, &清单(), &overrides);
        剥根名(&mut plan);
        let variant = 取(&plan, "杂志/甲.cbz");
        assert_eq!(variant.platform, None, "认不出平台不构成拒绝入库的理由");
        assert_eq!(variant.files, 2);
    }

    #[test]
    fn 变体根绝不上溯到平台目录本身() {
        // 一个直接躺在 `PSV/` 下的 TitleID 目录，若把整个平台吞成一个变体，
        // 十几万个文件会归到一条记录上——比不成型还糟。
        let plan = 成型(&[
            ("PSV/PCSG00042/eboot.bin", 900),
            ("PSV/别的游戏/x.vpk", 500),
        ]);
        let keys = 变体键(&plan);
        assert!(keys.contains(&"PSV/PCSG00042"), "{keys:?}");
        assert!(!keys.contains(&"PSV"), "{keys:?}");
        assert!(keys.contains(&"PSV/别的游戏/x.vpk"), "{keys:?}");
    }

    #[test]
    fn wii_u_的_loadiine_目录锚在自己身上() {
        let plan = 成型(&[
            ("WIIU/某游戏/code/app.xml", 100),
            ("WIIU/某游戏/content/data.bin", 90_000),
            ("WIIU/某游戏/meta/meta.xml", 100),
        ]);
        assert_eq!(变体键(&plan), ["WIIU/某游戏"]);
    }

    #[test]
    fn 未映射的顶层目录整体不成型() {
        let plan = 成型(&[
            ("杂志/攻略/第一期.cbz", 1000),
            ("模拟器/retroarch/core.dll", 2000),
            ("FC/超级马里奥.zip", 300),
        ]);
        assert_eq!(变体键(&plan), ["FC/超级马里奥.zip"]);
    }

    #[test]
    fn 明确排除的目录也不成型() {
        let plan = 成型(&[("pc/某游戏/game.iso", 9_000_000)]);
        assert!(plan.variants.is_empty());
    }

    #[test]
    fn 库根下的散文件不成型() {
        // 连顶层目录都没有，平台无从谈起；它照样进库体检（ADR-0011）。
        let plan = 成型(&[("散落的游戏.gba", 16)]);
        assert!(plan.variants.is_empty());
    }

    #[test]
    fn 媒体与文档不成变体() {
        let plan = 成型(&[
            ("FC/超级马里奥.zip", 300),
            ("FC/封面.png", 100),
            ("FC/说明.txt", 20),
            ("FC/.DS_Store", 8),
        ]);
        assert_eq!(变体键(&plan), ["FC/超级马里奥.zip"]);
        assert_eq!(plan.unshaped_files, 3);
        assert_eq!(plan.unshaped_bytes, 128);
    }

    #[test]
    fn 人工纠正优先于一切规则且能把散文件并成一个变体() {
        let entries = 建条目(&[("FC/甲.zip", 100), ("FC/乙.zip", 200), ("FC/丙.zip", 300)]);
        let mut overrides = BTreeMap::new();
        overrides.insert(键("FC/甲.zip"), 键("FC/甲.zip"));
        overrides.insert(键("FC/乙.zip"), 键("FC/甲.zip"));
        let mut plan = plan(&entries, &清单(), &overrides);
        剥根名(&mut plan);

        let mut keys = 变体键(&plan);
        keys.sort_unstable();
        assert_eq!(keys, ["FC/丙.zip", "FC/甲.zip"]);
        let 并起来的 = 取(&plan, "FC/甲.zip");
        assert_eq!(并起来的.files, 2);
        assert!(并起来的.manual);
        assert_eq!(并起来的.rule, MANUAL_RULE);
        assert_eq!(并起来的.main_key, "FC/甲.zip");
    }

    #[test]
    fn 人工纠正能把目录树规则拆错的东西拉回来() {
        // 规则会出错，人工纠正是一等公民功能（CONTEXT 的「成型规则」词条）。
        let entries = 建条目(&[
            ("ps3/某游戏/PS3_GAME/EBOOT.BIN", 100),
            ("ps3/另有隐情.iso", 500),
        ]);
        let mut overrides = BTreeMap::new();
        overrides.insert(键("ps3/另有隐情.iso"), 键("ps3/某游戏/PS3_GAME/EBOOT.BIN"));
        let mut plan = plan(&entries, &清单(), &overrides);
        剥根名(&mut plan);
        // 被纠正的文件从「一文件一变体」里被拉走，落到目录树那个变体的主文件名下。
        assert!(
            plan.variants.iter().all(|v| v.key != "ps3/另有隐情.iso"),
            "它不该再自成一个变体：{:?}",
            变体键(&plan)
        );
        let 目标 = 取(&plan, "ps3/某游戏/PS3_GAME/EBOOT.BIN");
        assert!(目标.manual);
        assert_eq!(
            目标.members,
            vec![
                ("ps3/另有隐情.iso".to_string(), Role::Companion),
                ("ps3/某游戏/PS3_GAME/EBOOT.BIN".to_string(), Role::Main),
            ]
        );
    }

    #[test]
    fn 成型与条目的先后无关() {
        let mut entries = 建条目(&[
            ("ps/某游戏/游戏.cue", 100),
            ("ps/某游戏/游戏.bin", 700),
            ("ps3/某游戏/PS3_GAME/EBOOT.BIN", 900),
        ]);
        let mut 正序 = plan(&entries, &清单(), &BTreeMap::new());
        剥根名(&mut 正序);
        entries.reverse();
        let mut 倒序 = plan(&entries, &清单(), &BTreeMap::new());
        剥根名(&mut 倒序);
        assert_eq!(正序, 倒序);
    }

    #[test]
    fn psv_那一堆内部资源收敛得下来() {
        // 这条测试是那句「PSV 若不成型会炸出十几万条垃圾条目」的可执行形态：
        // 一份转储 500 个文件，成型之后必须是 1 个变体。
        let mut files: Vec<(String, u64)> = Vec::new();
        for i in 0..500 {
            files.push((
                format!("PSV/PSVENJP/某游戏[PCSG00042]/app/PCSG00042/bgm/{i}.at9"),
                1000,
            ));
        }
        files.push((
            "PSV/PSVENJP/某游戏[PCSG00042]/app/PCSG00042/sce_sys/param.sfo".to_string(),
            700,
        ));
        let borrowed: Vec<(&str, u64)> = files.iter().map(|(k, l)| (k.as_str(), *l)).collect();
        let plan = 成型(&borrowed);
        assert_eq!(plan.variants.len(), 1);
        assert_eq!(plan.variants[0].files, 501);
    }
}
