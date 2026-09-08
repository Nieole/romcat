//! **导入与导出的两份报告**。
//!
//! 与体检、命中率、刮削、标题那几份同一个形状——从中立库折出来（ADR-0001），
//! 不重跑一遍，也不碰主库一个字节。
//!
//! ## 导入报告要说出口的三件事
//!
//! 1. **档位是实测出来的。** 每份文件都跑了一趟往返，逐字节比过。过了才写「无损往返」，
//!    没过就写降档后的那一档并指出第一处分岔在第几行。
//! 2. **留下了什么。** 注释几条、未知键几个、`x-` 扩展键几个——这几个数就是
//!    「一次往返没有蒸发心血」的证据。**用户状态单独一行**：ES gamelist 把收藏、
//!    游玩次数、上次游玩长在同一个文件里，导出时省略它们就等于清零（ADR-0006），
//!    所以「搬了几处」必须说出口，而不是混在未知键里。
//! 3. **格式自己会吞掉的写法在哪几行。** `rating: 85` 这一行在 Pegasus 里本来就没生效，
//!    维护者多半不知道。说出来比替他改掉强。ES gamelist 那一侧最常见的是**补足**：
//!    这个格式的日期没有精度这一说，只知道年份的发行日期写进去也带着一个月一个日。
//!
//! ## 导出报告要说出口的三件事
//!
//! 1. **收敛到什么程度**：多少变体收成了多少条目，其中多少是作品级的。
//! 2. **谁被挡在外面**：附属内容、非游戏资产、补丁各几个（ADR-0013、ADR-0010）。
//! 3. **有没有人在外面动过**：检测到就**不静默覆盖**，把冲突逐条列出来（ADR-0001 修订段）。
//!
//! ## 结构性损失在两份报告里说的是同一句
//!
//! 导入那一节第 3 条数的是**这份文件里**撞上了几处，[`StructuralLoss`] 说的是**格式结构上**
//! 做不到什么——后者与这一趟有没有撞上无关，一条都没撞上也照样成立。两份报告都印它，
//! 而且共用 `write_structural_losses` 这一个渲染：导出前读到的那句与导入后读到的那句
//! **必须是同一句**，各写各的就会漂成两种说法。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::adapter::{Lossy, StructuralLoss};
use crate::report::{heading, human_bytes, pad, thousands};

/// 报告里最多列几个例子。
///
/// 一处定死：`converge` 攒例子、`transfer` 截有损点、这里印，三处要是各写一个数，
/// 报告里就会出现「列了 10 条」与「其实攒了 8 条」这种对不上。
pub(super) const EXAMPLES: usize = 10;

/// 把这个格式的**结构性损失**写进报告。
///
/// **导入与导出共用这一个。** 同一件事在两份报告里各写各的，用户就会读到两种说法，
/// 而这两句正是他决定要不要导出的依据（票 02 的验收点名）。
///
/// 清单是空的就**一个字都不写**：空的意思是「还没查过」，不是「这个格式什么都不丢」
/// （[`Adapter::structural_losses`](crate::adapter::Adapter::structural_losses)）。
/// 印一句「无结构性损失」是一句没人核过的保证。
fn write_structural_losses(out: &mut String, losses: &[StructuralLoss]) {
    if losses.is_empty() {
        return;
    }
    heading(out, "这个格式的结构性损失");
    let _ = writeln!(
        out,
        "**与这一趟撞没撞上无关**：说的是格式本身做不到什么，不是这一趟丢了几条。\n\
         带**底本**的往返照旧一个字节都不差——变形只落在**新生成**的内容上。"
    );
    for loss in losses {
        let _ = writeln!(out, "  {}", loss.line());
    }
}

/// 一份导进来的文件的账。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportedFile {
    /// 那份文件在哪。
    pub path: String,
    /// 多少字节。
    pub bytes: u64,
    /// 多少行。
    pub lines: u64,
    /// 里面有几个合集段。
    pub collections: u64,
    /// 几个游戏段。
    pub games: u64,
    /// 几条注释。
    pub comments: u64,
    /// 几个中立模型不认的键。
    pub unknown_keys: u64,
    /// 几个 `x-` 扩展键。
    pub extension_keys: u64,
    /// 几处**用户状态**从快照原样搬了过去（ADR-0006）。
    pub user_state: u64,
    /// 往返逐字节相同吗。
    pub roundtrip: bool,
    /// 实测下来是哪一档。
    pub tier: String,
    /// 没过的话，第一处分岔在哪。
    pub difference: Option<String>,
    /// 这份文件里格式自己会吞掉的写法，**按吃法由重到轻**各取几条。
    pub lossy: Vec<String>,
    /// 按吃法各几条：`(吃法, 几条)`。
    pub lossy_by_kind: Vec<(String, u64)>,
    /// 一共几条。
    pub lossy_total: u64,
    /// 里面的条目有几条对回了库里的变体。
    pub resolved: u64,
    /// 几条对不回去。
    pub unresolved: u64,
}

/// 一次导入的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    /// 中立库在哪。
    pub catalog: String,
    /// 哪个格式。
    pub format: String,
    /// 适配器声称的上限。
    pub ceiling: String,
    /// **实测**下来是哪一档——全部文件里最低的那一档。
    pub tier: String,
    /// 这个格式的**结构性损失**。
    ///
    /// **与这一趟导了什么无关**：它是格式自己的边界，逐条都是 `&'static`。
    /// 导入这一侧也印，是因为「导出前就知道会丢什么」得从第一次接触这个格式起就说得出，
    /// 而不是等到导出那一趟才冒出来。
    pub structural_losses: Vec<StructuralLoss>,
    /// 逐份文件。
    pub files: Vec<ImportedFile>,
    /// 一共落了多少条字段值进中立库。
    pub values: u64,
    /// 对不回库里变体的那些 `file:` 路径，前几条。
    pub unresolved_examples: Vec<String>,
}

impl ImportReport {
    /// 打给人看的那一份。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "导入 {} 元数据", self.format);
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(out, "中立库          {}", self.catalog);
        let _ = writeln!(
            out,
            "能力档位        **{}**（声称的上限是 {}）",
            self.tier, self.ceiling
        );
        let _ = writeln!(
            out,
            "档位是**实测**出来的：每份文件都读进来又写回去，逐字节比过。\n\
             没过就自动降一档，并指出第一处分岔在第几行。"
        );
        write_structural_losses(&mut out, &self.structural_losses);

        heading(&mut out, "逐份文件");
        let _ = writeln!(
            out,
            "{}{}{}{}文件",
            pad("往返", 6),
            pad("段", 6),
            pad("注释", 6),
            pad("未知键", 8),
        );
        for file in &self.files {
            let _ = writeln!(
                out,
                "{}{}{}{}{}",
                pad(
                    if file.roundtrip {
                        "逐字节同"
                    } else {
                        "有差异"
                    },
                    6
                ),
                pad(&thousands(file.collections + file.games), 6),
                pad(&thousands(file.comments), 6),
                pad(&format!("{}+{}", file.unknown_keys, file.extension_keys), 8),
                file.path,
            );
            if let Some(difference) = &file.difference {
                let _ = writeln!(out, "  ⚠️ {difference}");
            }
        }

        // **用户状态单独说一句。** 它长在 ES gamelist 里，导出时省略等于把维护者的
        // 收藏与游玩记录清零（ADR-0006）。这个数就是「搬运真的发生了」的证据。
        let user_state: u64 = self.files.iter().map(|file| file.user_state).sum();
        if user_state > 0 {
            let _ = writeln!(
                out,
                "用户状态        {} 处（收藏、游玩次数、游玩时长、通关状态、上次游玩）\n\
                 工具**既不生成也不覆盖**，只从快照原样搬运——省略它们等于清零。",
                thousands(user_state)
            );
        }

        let lossy: u64 = self.files.iter().map(|file| file.lossy_total).sum();
        if lossy > 0 {
            heading(&mut out, "格式自己会吞掉的写法");
            let mut by_kind: BTreeMap<&str, u64> = BTreeMap::new();
            for file in &self.files {
                for (kind, count) in &file.lossy_by_kind {
                    *by_kind.entry(kind.as_str()).or_insert(0) += count;
                }
            }
            let _ = writeln!(
                out,
                "一共 {} 处。**这不是我们的损失，是格式的**——分三种吃法，轻重差得很远：",
                thousands(lossy)
            );
            for kind in Lossy::all() {
                let _ = writeln!(
                    out,
                    "{}{}",
                    pad(kind.label(), 12),
                    thousands(by_kind.get(kind.label()).copied().unwrap_or(0))
                );
            }
            let _ = writeln!(
                out,
                "**整条丢弃**那一档最要命：那几行在前端里根本不存在，而写的人多半不知道。"
            );
            for file in &self.files {
                for note in &file.lossy {
                    let _ = writeln!(out, "  {} {note}", file.path);
                }
            }
        }

        heading(&mut out, "落进中立库的");
        let _ = writeln!(
            out,
            "原文已**逐字节**存进中立库，而且这一份**永远不被导出顶掉**：\n\
             导出会把盘上那份改写成合并后的样子，库里这份原件是唯一还原得回去的东西。"
        );
        let resolved: u64 = self.files.iter().map(|file| file.resolved).sum();
        let unresolved: u64 = self.files.iter().map(|file| file.unresolved).sum();
        let _ = writeln!(
            out,
            "条目对上库里的变体  {} 条；对不上 {} 条\n字段值              {} 条（源写作「{}」）",
            thousands(resolved),
            thousands(unresolved),
            thousands(self.values),
            self.format,
        );
        if !self.unresolved_examples.is_empty() {
            let _ = writeln!(
                out,
                "对不上的例子（**它们的原文一字不差地留在快照里**，只是还挂不到变体上）："
            );
            for example in &self.unresolved_examples {
                let _ = writeln!(out, "  {example}");
            }
        }
        out
    }
}

/// 一份写出去的文件的账。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExportedFile {
    /// 落点。
    pub path: String,
    /// 里面那个合集叫什么。
    pub collection: String,
    /// 几个条目。
    pub entries: u64,
    /// 多少字节。
    pub bytes: u64,
    /// 真的写盘了吗（`--dry-run` 是没有）。
    pub written: bool,
    /// 以哪份快照为基线；没有基线就是 `None`。
    pub baseline: Option<String>,
    /// 基线里有、这次库里没有、**原样留下来**的段有几个。
    pub kept_verbatim: u64,
    /// 写出来那份自己再读一遍还能逐字节写回去吗。
    pub roundtrip: bool,
}

/// 一处外部改动。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Conflict {
    /// 哪份文件。
    pub path: String,
    /// 怎么回事。
    pub why: String,
}

/// 一次导出的报告。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExportReport {
    /// 中立库在哪。
    pub catalog: String,
    /// 哪个格式。
    pub format: String,
    /// 实测档位。
    pub tier: String,
    /// 这个格式的**结构性损失**。与导入报告里那一份是同一份、印出来是同一句。
    pub structural_losses: Vec<StructuralLoss>,
    /// 写到哪儿。
    pub out: String,
    /// 只排计划、不写盘吗。
    pub dry_run: bool,
    /// 逐份文件。
    pub files: Vec<ExportedFile>,
    /// 库里一共几个变体。
    pub variants: u64,
    /// 收敛成几个条目。
    pub entries: u64,
    /// 其中作品级收敛出来的。
    pub work_entries: u64,
    /// 其中一个变体一个条目的。
    pub loose_entries: u64,
    /// 真的写进条目的变体。
    pub exported_variants: u64,
    /// 多于一个变体、收敛真的起了作用的条目。
    pub converged_entries: u64,
    /// 被挡下的变体：`(理由, 几个)`。
    pub excluded: Vec<(String, u64)>,
    /// 被挡下的例子：`(理由, 变体的键)`。
    pub excluded_examples: Vec<(String, String)>,
    /// 库里的**附属内容**成员有几个——一个都不导出。
    pub extra_content_members: u64,
    /// 首选变体各凭什么当上的：`(理由, 几个条目)`。
    pub preferred: Vec<(String, u64)>,
    /// 检测到外部改动、因此**没有覆盖**的文件。
    pub conflicts: Vec<Conflict>,
    /// 检测到外部改动、但用户给了 `--force` 于是**照写了**的文件。
    ///
    /// 它与 `conflicts` 分成两列，是因为两者对用户的意思相反：一个是「你要的事没做，
    /// 去处理一下」，一个是「你要的事做了，代价是这个」。合成一列，`--force` 那一趟
    /// 要么被当成失败、要么就什么也不说——而**丢掉一次手改是必须说出口的**。
    pub forced: Vec<Conflict>,
}

impl ExportReport {
    /// 打给人看的那一份。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "导出 {} 元数据", self.format);
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(out, "中立库          {}", self.catalog);
        let _ = writeln!(out, "落点            {}", self.out);
        let _ = writeln!(out, "能力档位        **{}**（实测）", self.tier);
        if self.dry_run {
            let _ = writeln!(out, "**只排了计划，一个字节都没写盘。**");
        }
        write_structural_losses(&mut out, &self.structural_losses);

        heading(&mut out, "收敛");
        let _ = writeln!(
            out,
            "变体            {} 个，其中 {} 个进了条目\n\
             条目            {} 个：作品级 {} 个、还没认出作品的 {} 个\n\
             真的合并了的    {} 个条目装着不止一个变体",
            thousands(self.variants),
            thousands(self.exported_variants),
            thousands(self.entries),
            thousands(self.work_entries),
            thousands(self.loose_entries),
            thousands(self.converged_entries),
        );
        let _ = writeln!(
            out,
            "收敛是按**作品 × 平台**：一个合集一份文件。合集混在一份文件里就说不清归属\n\
             ——Pegasus 会把一个 game 加进该文件中此前定义过的**所有**合集，\n\
             ES 家族则是一个平台目录一份 gamelist。"
        );

        heading(&mut out, "首选变体");
        let _ = writeln!(out, "汉化 > 官中 > 日版 > 其他，裁决压过全部（ADR-0012）。");
        for (why, count) in &self.preferred {
            let _ = writeln!(out, "{}{}", pad(why, 10), thousands(*count));
        }

        heading(&mut out, "入库但不导出");
        let _ = writeln!(out, "判据是**能否独立运行**，不是目录名（ADR-0013）。");
        for (why, count) in &self.excluded {
            let _ = writeln!(out, "{}{} 个变体", pad(why, 12), thousands(*count));
        }
        let _ = writeln!(
            out,
            "{}{} 个成员（它们是变体的一部分，本来就成不了条目）",
            pad("附属内容", 12),
            thousands(self.extra_content_members)
        );
        for (why, key) in &self.excluded_examples {
            let _ = writeln!(out, "  {why}：{key}");
        }

        heading(&mut out, "逐份文件");
        let _ = writeln!(
            out,
            "{}{}{}文件",
            pad("条目", 8),
            pad("体积", 10),
            pad("基线", 6),
        );
        for file in &self.files {
            let _ = writeln!(
                out,
                "{}{}{}{}",
                pad(&thousands(file.entries), 8),
                pad(&human_bytes(file.bytes), 10),
                pad(
                    if file.baseline.is_some() {
                        "有"
                    } else {
                        "无"
                    },
                    6
                ),
                file.path,
            );
            if file.kept_verbatim > 0 {
                let _ = writeln!(
                    out,
                    "  基线里有 {} 段库里没有，**原样留着**——工具不认得的东西不因此消失。",
                    thousands(file.kept_verbatim)
                );
            }
            if !file.roundtrip {
                let _ = writeln!(out, "  ⚠️ 这份写出来之后自己读回去对不上，档位已降。");
            }
        }

        if !self.forced.is_empty() {
            heading(&mut out, "⚠️ 外面有人动过，这几份照 `--force` 覆盖掉了");
            for conflict in &self.forced {
                let _ = writeln!(out, "  {}\n    {}", conflict.path, conflict.why);
            }
            let _ = writeln!(
                out,
                "那几次手改**已经没了**。「不静默覆盖」说的是不许悄悄发生，不是不许发生。"
            );
        }
        if self.conflicts.is_empty() {
            let _ = writeln!(
                out,
                "\n没有检测到外部改动。**导出不是覆盖写**：盘上那份与上次对齐的样子不一致时，\n\
                 这一趟会停下来问，不会把手改的成果静默吞掉（ADR-0001 修订段）。"
            );
        } else {
            heading(&mut out, "⚠️ 外面有人动过，这几份没写");
            for conflict in &self.conflicts {
                let _ = writeln!(out, "  {}\n    {}", conflict.path, conflict.why);
            }
            let _ = writeln!(
                out,
                "先把要留的内容 `romcat import` 进来，或者确认可以丢弃之后加 `--force`。"
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{Adapter, pegasus::Pegasus};

    /// 把**结构性损失**那一节从渲染出来的报告里抠出来。
    ///
    /// 一节由 [`heading`] 起头（空一行、标题、横线），到下一节那个空行为止。
    fn 那一节(text: &str) -> &str {
        let start = text.find("这个格式的结构性损失").expect("有这一节");
        let rest = &text[start..];
        let end = rest.find("\n\n").expect("后面还有别的节");
        &rest[..end]
    }

    #[test]
    fn 导入与导出对结构性损失说的是同一句() {
        // 用户读到的两句要是不一样，他就得自己猜哪一句算数。
        //
        // **它钉的是「两句一字不差」，不是「共用同一个函数」**——真有人在某一侧原样
        // 抄一份渲染，它照样绿。共用 `write_structural_losses` 是让这条一直绿的做法，
        // 不是这条测试证得出的事。
        let losses = Pegasus.structural_losses().to_vec();
        let 导入 = ImportReport {
            structural_losses: losses.clone(),
            ..ImportReport::default()
        }
        .render_text();
        let 导出 = ExportReport {
            structural_losses: losses,
            ..ExportReport::default()
        }
        .render_text();
        assert_eq!(那一节(&导入), 那一节(&导出), "两侧说的必须是同一句");
        assert!(那一节(&导入).contains("换行"), "{}", 那一节(&导入));
        assert!(那一节(&导入).contains("U+3000"), "{}", 那一节(&导入));
    }

    /// 一个**还没量过**的格式：它没有覆写 [`Adapter::structural_losses`]。
    ///
    /// 两个真适配器现在都量过了（Pegasus 五条、ES gamelist 八条），拿它们当被测对象
    /// 证不了「空清单一个字都不印」这条口径还在。所以留一个没量过的在这儿——它同时
    /// 钉住两样：**默认实现交出来的是空清单**，以及**渲染那一侧对空清单只字不提**。
    #[derive(Debug, Clone, Copy)]
    struct 还没量过的格式;

    impl Adapter for 还没量过的格式 {
        fn name(&self) -> &'static str {
            "还没量过的格式"
        }
        fn ceiling(&self) -> crate::adapter::Capability {
            crate::adapter::Capability::WriteOnly
        }
        fn file_name(&self) -> &'static str {
            "还没量过.txt"
        }
        fn read(
            &self,
            _bytes: &[u8],
        ) -> Result<crate::adapter::Parsed, crate::adapter::AdapterError> {
            unreachable!("这个格式只用来钉空清单那条口径")
        }
        fn write(
            &self,
            _doc: &crate::adapter::Document,
            _baseline: Option<&crate::adapter::Parsed>,
        ) -> Result<Vec<u8>, crate::adapter::AdapterError> {
            unreachable!("这个格式只用来钉空清单那条口径")
        }
        fn media_placement(
            &self,
            _rom_key: &str,
            _kind: crate::scrape::MediaKind,
            _hash: &str,
            _ext: &str,
        ) -> Option<crate::adapter::MediaPlacement> {
            None
        }
    }

    #[test]
    fn 没声明结构性损失的格式一个字都不印() {
        // 空清单的意思是**还没查过**，不是「这个格式什么都不丢」。印一句
        // 「无结构性损失」就是替一份没人核过的调研背书。
        assert!(
            还没量过的格式.structural_losses().is_empty(),
            "没覆写的适配器，默认交出来的就该是空清单"
        );
        let text = ImportReport {
            structural_losses: 还没量过的格式.structural_losses().to_vec(),
            ..ImportReport::default()
        }
        .render_text();
        assert!(!text.contains("结构性损失"), "空清单该一个字都不印：{text}");
        assert!(!text.contains("无结构性损失"), "更不许印这句假保证：{text}");
        // 导出那一侧是同一条口径——用户在导出**之前**读到的那份也不许印。
        let text = ExportReport {
            structural_losses: 还没量过的格式.structural_losses().to_vec(),
            ..ExportReport::default()
        }
        .render_text();
        assert!(!text.contains("结构性损失"), "导出那一侧一样：{text}");
    }
}
