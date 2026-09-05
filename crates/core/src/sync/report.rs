//! 把[操作计划](super::Plan)排一遍版。
//!
//! ## 这里**没有**一个「差量预览」类型
//!
//! 别的几层都是「从中立库折出一份报告结构」（`SelectionReport`、`HealthReport`……），
//! 这一层刻意不是。ADR-0016 要的预览就是 [`plan`](super::plan) 的返回值本身，
//! 另立一个报告类型等于把同一份账算两遍——而两遍算法迟早会漂开，那时用户看到的
//! 预览与同步真正会做的事就对不上了，正是这条硬要求要防的事。
//!
//! 于是这个文件里只有 [`Plan::render_text`]：一个 `&Plan -> String` 的排版函数。
//! `--json` 那一份直接就是 `Plan` 自己。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::report::{heading, human_bytes, human_duration, pad, thousands};

use super::execute::{Done, Outcome, Placement};
use super::{Act, FileKind, Plan, Step, SurpriseKind, Tally};
use crate::capability::RejectReason;

/// 每一类操作在报告里举几个例子。
const EXAMPLES: usize = 10;

/// 意外变化列几条。
const SURPRISE_EXAMPLES: usize = 20;

impl Plan {
    /// 打给人看的**差量预览**。
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "子库 {} · 差量预览", self.sublibrary);
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(out, "目标            {}", self.target);
        let _ = writeln!(out, "前端格式        {}", self.format);
        let _ = writeln!(
            out,
            "容量上限        {}",
            self.capacity
                .map_or_else(|| "不设限".to_string(), human_bytes)
        );
        let _ = writeln!(out, "能力档案        {}", self.capability);

        heading(&mut out, "差量");
        line(&mut out, "新增", self.adds, None);
        line(&mut out, "更新", self.updates, Some(self.updates_before));
        line(&mut out, "删除", self.deletes, None);
        line(&mut out, "保持", self.keeps, None);
        let _ = writeln!(out, "{}{}", pad("净变化", 12), signed_bytes(self.net_bytes));
        let _ = writeln!(
            out,
            "{}{}（{} 个文件 / {} 个变体）",
            pad("子库共", 12),
            pad(&human_bytes(self.desired_bytes), 12),
            thousands(self.desired.files),
            thousands(self.desired.variants),
        );
        if self.touched() == 0 {
            let _ = writeln!(out, "**一个文件都不用动**：目标已经与选择集对齐。");
        }
        if self.unreadable_sources > 0 {
            let _ = writeln!(
                out,
                "⚠️ 主库侧有 {} 个成员元数据读不到，它们照样要搬，但**容量按 0 计**\n\
                 （ADR-0021）——上面这几个数因此是**下界**。",
                thousands(self.unreadable_sources),
            );
        }
        if !self.empty_variants.is_empty() {
            let _ = writeln!(
                out,
                "⚠️ 有 {} 个变体选中了、却一个文件成员都没有：",
                thousands(self.empty_variants.len() as u64),
            );
            for key in self.empty_variants.iter().take(EXAMPLES) {
                let _ = writeln!(out, "  {key}");
            }
        }
        // 按类别拆开：ROM / 媒体 / 元数据。眼下只产得出 ROM（票 20 补另外两类）。
        let by_kind = kinds(&self.steps);
        if by_kind.len() > 1 {
            heading(&mut out, "按类别");
            let _ = writeln!(out, "{}{}类别", pad("文件", 10), pad("容量", 12));
            for kind in FileKind::all() {
                let Some(tally) = by_kind.get(&kind) else {
                    continue;
                };
                let _ = writeln!(
                    out,
                    "{}{}{}",
                    pad(&thousands(tally.files), 10),
                    pad(&human_bytes(tally.bytes), 12),
                    kind.label(),
                );
            }
        }

        for act in [Act::Delete, Act::Update, Act::Add] {
            let mut steps: Vec<&Step> = self.steps.iter().filter(|s| s.act == act).collect();
            if steps.is_empty() {
                continue;
            }
            // 最大的排前面：要砍要等，先看得见大头。
            steps.sort_by(|a, b| {
                b.bytes
                    .max(b.was)
                    .cmp(&a.bytes.max(a.was))
                    .then_with(|| a.path.cmp(&b.path))
            });
            heading(&mut out, act.label());
            for step in steps.iter().take(EXAMPLES) {
                let _ = writeln!(
                    out,
                    "  {}{}{}",
                    pad(&human_bytes(step.bytes.max(step.was)), 12),
                    if step.restore { "（补回）" } else { "" },
                    step.path,
                );
            }
            if steps.len() > EXAMPLES {
                let _ = writeln!(
                    out,
                    "  …… 另有 {} 个。`--json` 出完整的一份。",
                    thousands((steps.len() - EXAMPLES) as u64),
                );
            }
        }

        heading(&mut out, "要转格式的");
        if self.converts.files == 0 {
            let _ = writeln!(
                out,
                "一个都不用转：目标吃得下这一趟要搬的每一份（或者这份档案不作声称）。"
            );
        } else {
            let _ = writeln!(
                out,
                "{}{}（{} 个变体）",
                pad("要转", 12),
                pad(&format!("{} 个", thousands(self.converts.files)), 12),
                thousands(self.converts.variants),
            );
            let _ = writeln!(
                out,
                "{}{}",
                pad("要读", 12),
                human_bytes(self.convert_source_bytes),
            );
            let _ = writeln!(
                out,
                "{}{}",
                pad("转出来", 12),
                human_bytes(self.converts.bytes),
            );
            // **粗估两个字必须印出来。** 它是「字节数乘一个常量」，源盘、目标介质、
            // 是解压还是重压，随便哪一样都能把它带偏一倍。印一个看着精确的错数，
            // 比印一个明说是粗估的数糟得多。
            let _ = writeln!(
                out,
                "{}{}（**粗估**：按实测吞吐乘出来的，真机上差一倍很正常）",
                pad("大概要", 12),
                human_duration(self.convert_ms),
            );
            if self.convert_estimated > 0 {
                let _ = writeln!(
                    out,
                    "其中 {} 个的**产物大小也是估的**（重打包成 zip 那一路，压完才知道多大），\n\
                     上面「转出来」与容量那一栏因此是**上界**。",
                    thousands(self.convert_estimated),
                );
            }
            let mut rows: Vec<&Step> = self
                .steps
                .iter()
                .filter(|step| step.act != Act::Delete && step.convert.is_some())
                .collect();
            rows.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
            for step in rows.iter().take(EXAMPLES) {
                let Some(conversion) = &step.convert else {
                    continue;
                };
                let _ = writeln!(
                    out,
                    "  {}{} → {}",
                    pad(&human_bytes(step.bytes), 12),
                    step.source,
                    step.path,
                );
                let _ = writeln!(out, "  {}转成{}", pad("", 12), conversion.recipe.label(),);
            }
            if rows.len() > EXAMPLES {
                let _ = writeln!(
                    out,
                    "  …… 另有 {} 个。`--json` 出完整的一份。",
                    thousands((rows.len() - EXAMPLES) as u64),
                );
            }
            let _ = writeln!(
                out,
                "**转换只产生新文件**：主库里那几份原始形态一个字节都不会动（ADR-0004）。"
            );
        }

        if !self.rejected.is_empty() {
            heading(&mut out, "放不进目标存储");
            let _ = writeln!(
                out,
                "{} 个、{}。**这一趟一个都不传**——传必然失败，而失败会在卡上留下\n\
                 半份文件、在清单里留下一条谎。这几个也**不会被删掉**：万一目标上已经\n\
                 有一份，那是这条声明自己可能就错了，不是它该被删的理由。",
                thousands(self.rejected.len() as u64),
                human_bytes(self.rejected.iter().map(|file| file.bytes).sum()),
            );
            for reason in RejectReason::all() {
                let rows: Vec<_> = self
                    .rejected
                    .iter()
                    .filter(|file| file.reason == reason)
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "{}  {} 个",
                    pad(reason.label(), 18),
                    thousands(rows.len() as u64),
                );
                for file in rows.iter().take(EXAMPLES) {
                    let _ = writeln!(out, "  {}", file.path);
                    let _ = writeln!(out, "    {}", file.detail);
                }
                if rows.len() > EXAMPLES {
                    let _ = writeln!(
                        out,
                        "  …… 另有 {} 个。",
                        thousands((rows.len() - EXAMPLES) as u64)
                    );
                }
            }
            // **上界不能当准数报。** 重打包成 zip 那一路填的是未压缩总量，压完说不定
            // 就装得下了——不说这句，用户会照着一条其实不成立的结论去换一张卡。
            let 估的 = self.rejected.iter().filter(|file| file.estimated).count();
            if 估的 > 0 {
                let _ = writeln!(
                    out,
                    "其中 {} 个的大小是**上界**（重打包的产物压完才知道多大），\n\
                     这几条有可能是误判——真压完说不定就装得下了。",
                    thousands(估的 as u64),
                );
            }
            let _ = writeln!(
                out,
                "换一张 exFAT 的卡，或者 `romcat sublibrary except {} --exclude <变体的键>`\n\
                 把它们从选择集里去掉。",
                self.sublibrary,
            );
        }

        if !self.unsupported.is_empty() {
            heading(&mut out, "到了目标上打不开");
            let _ = writeln!(
                out,
                "{} 个、{}。目标吃不下这个形态，而**这一版转不了它**。\n\
                 它们**照样搬过去**——不搬是静默丢掉你亲手挑中的东西，比白占地方糟得多。\n\
                 但说清楚：搬过去也开不了，得你自己拿外部工具转一遍。",
                thousands(self.unsupported.len() as u64),
                human_bytes(self.unsupported.iter().map(|file| file.bytes).sum()),
            );
            let mut rows: Vec<_> = self.unsupported.iter().collect();
            rows.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
            for file in rows.iter().take(EXAMPLES) {
                let _ = writeln!(out, "  {}{}", pad(&human_bytes(file.bytes), 12), file.path,);
                let _ = writeln!(
                    out,
                    "  {}{}想要 {}",
                    pad("", 12),
                    file.platform
                        .as_deref()
                        .map_or_else(String::new, |name| format!("[{name}] ")),
                    file.want,
                );
                let _ = writeln!(out, "  {}{}", pad("", 12), file.why);
            }
            if rows.len() > EXAMPLES {
                let _ = writeln!(
                    out,
                    "  …… 另有 {} 个。`--json` 出完整的一份。",
                    thousands((rows.len() - EXAMPLES) as u64),
                );
            }
            let _ = writeln!(
                out,
                "这一栏来自**能力档案**「{}」。矩阵错了比不转换更糟（ADR-0017）——\n\
                 觉得这几条判错了，`romcat capability {}` 看它的出处与核实日期。",
                self.capability, self.capability,
            );
        }

        heading(&mut out, "目标上对不上的");
        if self.surprises.is_empty() {
            let _ = writeln!(
                out,
                "{}",
                if self.withheld > 0 {
                    "没有新的。"
                } else {
                    "没有。清单记的每一条在目标上都还是原样。"
                },
            );
        } else {
            for kind in SurpriseKind::all() {
                let rows: Vec<_> = self
                    .surprises
                    .iter()
                    .filter(|surprise| surprise.kind == kind)
                    .collect();
                if rows.is_empty() {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "{}  {} 个",
                    pad(kind.label(), 10),
                    thousands(rows.len() as u64)
                );
                for surprise in rows.iter().take(SURPRISE_EXAMPLES) {
                    let _ = writeln!(
                        out,
                        "  {}{}{}",
                        if surprise.still_wanted {
                            "[还要] "
                        } else {
                            "[不要了] "
                        },
                        surprise.path,
                        // 落点与卡上那份只差大小写时，**两条都得印**：只印一条，
                        // 用户要么在卡上找不到那个名字，要么不知道是谁要挤进来。
                        surprise
                            .landing
                            .as_ref()
                            .map_or_else(String::new, |landing| format!("  ← 本来要落 {landing}")),
                    );
                }
                if rows.len() > SURPRISE_EXAMPLES {
                    let _ = writeln!(
                        out,
                        "  …… 另有 {} 个。",
                        thousands((rows.len() - SURPRISE_EXAMPLES) as u64)
                    );
                }
            }
            if self.surprises.iter().any(|s| s.landing.is_some()) {
                let _ = writeln!(
                    out,
                    "带「← 本来要落」那几条是**只差大小写**（或只差 NFC/NFD）撞上的：\n\
                     卡是 exFAT / FAT32，Windows 与 macOS 默认的 APFS 也一样，这两条路径\n\
                     在目标上是**同一个文件**，落上去就顶掉了卡上那份。改掉两边任一个\n\
                     名字，或者把卡上那份挪走。",
                );
            }
            let _ = writeln!(
                out,
                "这些**本次一律不动**。清单说有、实际没了的**不会静默补回**——那可能是你\n\
                 在掌机上有意删的（ADR-0015）。想让它别再回来，记一条例外：\n\
                 `romcat sublibrary except {} --exclude <变体的键>`；\n\
                 想补回来，这次加上 `--restore`。",
                self.sublibrary,
            );
        }
        if self.unlistable_dirs > 0 {
            let _ = writeln!(
                out,
                "⚠️ 目标上有 {} 个目录列不开，那几枝底下的东西全部说不清——说不清的一律不碰。",
                thousands(self.unlistable_dirs),
            );
        }
        if self.withheld > 0 {
            let _ = writeln!(
                out,
                "另有 {} 个是**你删过、工具记着不补**的：选择集还要它们，但它们不会自己\n\
                 长回来。这次加 `--restore` 才补。",
                thousands(self.withheld),
            );
        }

        heading(&mut out, "清单之外");
        if self.strangers == 0 {
            let _ = writeln!(out, "目标上没有清单之外的文件。");
        } else {
            let _ = writeln!(
                out,
                "{} 个文件、{}。**工具连碰都不碰**（ADR-0015）：手动拷进去的存档、\n\
                 金手指、截图落在清单之外，连成为一条计划步骤的路径都没有。",
                thousands(self.strangers),
                human_bytes(self.stranger_bytes),
            );
            if self.stranger_unreadable > 0 {
                let _ = writeln!(
                    out,
                    "其中 {} 个元数据读不到，容量按 0 计——上面那个数是下界。",
                    thousands(self.stranger_unreadable),
                );
            }
        }

        heading(&mut out, "装得下吗");
        // 比的是**目标现占加净变化**，不是子库该有多大：卡上的地方是共用的，
        // 手动拷进去的存档、落点被占传不上去的、被改过因而不删的，全都还占着位置。
        let _ = writeln!(
            out,
            "{}{}（其中清单之外 {}）",
            pad("目标现占", 12),
            pad(&human_bytes(self.actual_bytes), 12),
            human_bytes(self.stranger_bytes),
        );
        let _ = writeln!(
            out,
            "{}{}",
            pad("同步之后", 12),
            human_bytes(self.after_bytes),
        );
        match (self.capacity, self.over_capacity) {
            (None, _) => {
                let _ = writeln!(
                    out,
                    "没设容量上限。`romcat sublibrary set {} --capacity 512GB` 设一个\n\
                     就能在这里看到余量。",
                    self.sublibrary,
                );
            }
            (Some(limit), None) => {
                let _ = writeln!(
                    out,
                    "装得下：{} / {}，还剩 {}。",
                    human_bytes(self.after_bytes),
                    human_bytes(limit),
                    human_bytes(limit.saturating_sub(self.after_bytes)),
                );
            }
            (Some(limit), Some(over)) => {
                let _ = writeln!(
                    out,
                    "**装不下**：{} / {}，超出 {}。",
                    human_bytes(self.after_bytes),
                    human_bytes(limit),
                    human_bytes(over),
                );
                let _ = writeln!(
                    out,
                    "**不会自动截断**（ADR-0016）——同一套规则在两张不同容量的卡上会选出\n\
                     完全不同的东西，而你无从得知它砍掉了什么。砍谁由你定，最大的几个变体是：",
                );
                let _ = writeln!(out, "  {}{}变体", pad("腾出", 12), pad("累计", 12));
                for trim in &self.trim_suggestions {
                    let _ = writeln!(
                        out,
                        "  {}{}{}",
                        pad(&human_bytes(trim.bytes), 12),
                        pad(&human_bytes(trim.cumulative), 12),
                        trim.variant,
                    );
                }
                if let Some(last) = self.trim_suggestions.last()
                    && last.cumulative < over
                {
                    let _ = writeln!(
                        out,
                        "这几个**全砍掉也只腾出 {}，还差 {}**——`--json` 出完整的一份，\n\
                         或者改规则少选一些。",
                        human_bytes(last.cumulative),
                        human_bytes(over - last.cumulative),
                    );
                }
                let _ = writeln!(
                    out,
                    "排除一个：`romcat sublibrary except {} --exclude <变体的键>`",
                    self.sublibrary,
                );
                if self.stranger_bytes > 0 {
                    let _ = writeln!(
                        out,
                        "另一条路是自己清掉清单之外那 {}——**工具不会替你动它们**。",
                        human_bytes(self.stranger_bytes),
                    );
                }
            }
        }
        out
    }
}

impl Outcome {
    /// 打给人看的**同步结果**。
    ///
    /// 它与[差量预览](Plan::render_text)是两份报告，因为回答的是两个不同的问题：
    /// 预览说「会做什么」，这一份说「**做成了什么**」。两者对不上正是要看得见的东西
    /// ——写不进去的那几个、被中断截住的那几个，都落在这个差里。
    #[must_use]
    pub fn render_text(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "子库 {} · 同步结果", self.sublibrary);
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(out, "目标            {}", self.target);

        heading(&mut out, "做成了什么");
        done_line(&mut out, "新增", self.added);
        done_line(&mut out, "更新", self.updated);
        done_line(&mut out, "删除", self.deleted);
        if self.touched() == 0 && self.failures.is_empty() {
            let _ = writeln!(out, "**一个文件都没动**：目标本来就与选择集对齐。");
        }

        if self.converted.files > 0 {
            heading(&mut out, "转了什么");
            let _ = writeln!(
                out,
                "{}{}",
                pad("转了", 12),
                format_args!(
                    "{} 份，产物共 {}",
                    thousands(self.converted.files),
                    human_bytes(self.converted.bytes),
                ),
            );
            let _ = writeln!(
                out,
                "{}{}",
                pad("用时", 12),
                human_duration(self.convert_ms)
            );
            if self.convert_cached > 0 {
                let _ = writeln!(
                    out,
                    "其中 {} 份是**从转换缓存取的**，没有真转——多台设备要同一种格式时\n\
                     转一次用多次。",
                    thousands(self.convert_cached),
                );
            }
            let _ = writeln!(out, "主库那几份原始形态**一个字节都没动**（ADR-0004）。");
        }

        if let Some(placement) = self.placement {
            heading(&mut out, "媒体怎么放的");
            let _ = writeln!(out, "探测结果      {}（媒体池 → 目标）", placement.label());
            // **这两个数只算媒体。** ROM 与元数据一律复制，把它们算进来会让「复制了
            // 几份」虚高，然后这一栏指着一条走对了的链接路径说它没生效。
            let _ = writeln!(
                out,
                "实际          媒体 {} 份：硬链接 {}、复制 {}",
                thousands(self.linked + self.copied),
                thousands(self.linked),
                thousands(self.copied),
            );
            match placement {
                Placement::Link => {
                    let _ = writeln!(
                        out,
                        "同卷且支持硬链接，铺过去的媒体**不额外占空间**（ADR-0009）。",
                    );
                }
                Placement::Copy => {
                    let _ = writeln!(
                        out,
                        "目标文件系统不支持硬链接（SD 卡的 exFAT / FAT32 就是这样），\n\
                         自动降级为复制——这是必需路径，不是退让。",
                    );
                }
            }
        }

        heading(&mut out, "清单");
        let _ = writeln!(
            out,
            "{}{} 条（同步完之后目标的真实状态）",
            pad("现在记着", 12),
            thousands(self.manifest.files.len() as u64),
        );
        if self.withheld > 0 {
            let _ = writeln!(
                out,
                "其中 {} 条标着「你在目标上删过、工具记着不补」——**它们不会自己长回来**。\n\
                 想补回来下次加 `--restore`；想让它们从此不再被念叨，记一条例外：\n\
                 `romcat sublibrary except {} --exclude <变体的键>`",
                thousands(self.withheld),
                self.sublibrary,
            );
        }
        if self.dropped > 0 {
            let _ = writeln!(
                out,
                "另有 {} 条从清单里丢掉了：不要它们了，目标上也确实没有。",
                thousands(self.dropped),
            );
        }

        if !self.failures.is_empty() {
            heading(&mut out, "没做成的");
            let _ = writeln!(
                out,
                "{} 个。**它们没有进清单**——清单只记真的放上去了的那些。",
                thousands(self.failures.len() as u64),
            );
            for failure in self.failures.iter().take(EXAMPLES) {
                let _ = writeln!(
                    out,
                    "  {}{}——{}",
                    pad(failure.act.label(), 6),
                    failure.path,
                    failure.why,
                );
            }
            if self.failures.len() > EXAMPLES {
                let _ = writeln!(
                    out,
                    "  …… 另有 {} 个。",
                    thousands((self.failures.len() - EXAMPLES) as u64),
                );
            }
        }
        if self.gave_up {
            let _ = writeln!(
                out,
                "\n⚠️ **连着 {} 个写不进去，停下来了。** 多半是卡满了、卡被拔了，\n\
                 或者目标变成只读。清单记的是到停下来为止的真实状态，**接着跑就是**。",
                thousands(super::execute::GIVE_UP_AFTER),
            );
        }
        if self.interrupted {
            let _ = writeln!(
                out,
                "\n⚠️ **这一趟被中断了。** 写到一半的那份已经清掉，落点上是完整的旧文件；\n\
                 清单记的是到中断为止目标上的真实状态。**再跑一次就从剩下的接着来。**",
            );
        }
        out
    }
}

/// 做成了什么那一栏的一行。
fn done_line(out: &mut String, name: &str, done: Done) {
    let _ = writeln!(
        out,
        "{}{}{}",
        pad(name, 12),
        pad(&format!("{} 个文件", thousands(done.files)), 26),
        human_bytes(done.bytes),
    );
}

/// 差量那一栏的一行。
fn line(out: &mut String, name: &str, tally: Tally, before: Option<u64>) {
    let _ = writeln!(
        out,
        "{}{}{}{}",
        pad(name, 12),
        pad(
            &format!(
                "{} 个文件 / {} 个变体",
                thousands(tally.files),
                thousands(tally.variants)
            ),
            26
        ),
        pad(&human_bytes(tally.bytes), 12),
        match before {
            Some(before) if tally.files > 0 => format!("（原先 {}）", human_bytes(before)),
            _ => String::new(),
        },
    );
}

/// 带符号的容量，给净变化用。
fn signed_bytes(net: i64) -> String {
    let sign = if net < 0 { "-" } else { "+" };
    format!("{sign}{}", human_bytes(net.unsigned_abs()))
}

/// 把步骤按类别折成账。
fn kinds(steps: &[Step]) -> BTreeMap<FileKind, Tally> {
    let mut out: BTreeMap<FileKind, Tally> = BTreeMap::new();
    for step in steps {
        let tally = out.entry(step.kind).or_default();
        tally.files += 1;
        tally.bytes += step.bytes.max(step.was);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sublibrary::Sublibrary;
    use crate::sync::{Desired, DesiredFile, Manifest, Options, TargetState, plan};

    fn 子库() -> Sublibrary {
        Sublibrary {
            name: "掌机".to_string(),
            target: "/Volumes/SDCARD/Games".to_string(),
            target_raw: Some("/Volumes/SDCARD/Games".to_string()),
            format: "Pegasus".to_string(),
            capacity: Some(4096),
            capability: None,
        }
    }

    #[test]
    fn 预览印得出新增删除与净变化() {
        let desired = Desired {
            files: vec![DesiredFile {
                path: "GB/一.zip".to_string(),
                kind: FileKind::Rom,
                bytes: 1024,
                unreadable: false,
                source: "GB/一.zip".to_string(),
                source_stamp: crate::sync::Stamp {
                    bytes: 1024,
                    mtime_ns: Some(1),
                },
                variant: "GB/一.zip".to_string(),
                convert: None,
            }],
            ..Desired::default()
        };
        let text = plan(
            &子库(),
            &desired,
            &Manifest::empty(),
            &TargetState::default(),
            Options::default(),
        )
        .render_text();
        assert!(text.contains("差量预览"), "{text}");
        assert!(text.contains("新增"), "{text}");
        assert!(text.contains("净变化"), "{text}");
        assert!(text.contains("+1.00 KiB"), "{text}");
        assert!(text.contains("装得下"), "{text}");
    }

    #[test]
    fn 什么都不用动时明说() {
        let text = plan(
            &子库(),
            &Desired::default(),
            &Manifest::empty(),
            &TargetState::default(),
            Options::default(),
        )
        .render_text();
        assert!(text.contains("一个文件都不用动"), "{text}");
    }

    #[test]
    fn 落点只差大小写被占时_两条路径都印出来() {
        // 只印卡上那个名字，用户不知道是谁要挤进来；只印落点，用户按那个名字在卡上
        // 什么都找不到。两条都印，再加一句为什么它们是同一个文件。
        let desired = Desired {
            files: vec![DesiredFile {
                path: "GB/tetris.zip".to_string(),
                kind: FileKind::Rom,
                bytes: 1024,
                unreadable: false,
                source: "库/GB/tetris.zip".to_string(),
                source_stamp: crate::sync::Stamp {
                    bytes: 1024,
                    mtime_ns: Some(1),
                },
                variant: "库/GB/tetris.zip".to_string(),
                convert: None,
            }],
            ..Desired::default()
        };
        let actual = TargetState {
            files: vec![crate::sync::TargetFile {
                path: "GB/Tetris.zip".to_string(),
                stamp: Some(crate::sync::Stamp {
                    bytes: 999,
                    mtime_ns: Some(2),
                }),
            }],
            unlistable_dirs: 0,
        };
        let text = plan(
            &子库(),
            &desired,
            &Manifest::empty(),
            &actual,
            Options::default(),
        )
        .render_text();
        assert!(text.contains("落点被占"), "{text}");
        assert!(text.contains("GB/Tetris.zip"), "{text}");
        assert!(text.contains("← 本来要落 GB/tetris.zip"), "{text}");
        assert!(text.contains("只差大小写"), "{text}");
        // 它照样算清单之外——报告不能说出「目标上没有清单之外的文件」。
        assert!(!text.contains("目标上没有清单之外的文件"), "{text}");
    }
}
