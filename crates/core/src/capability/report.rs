//! 把[能力档案](super::Profile)排一遍版。
//!
//! 这一层存在的**唯一**理由是让矩阵可被复核。ADR-0017：矩阵错误比不转换更糟——用户会
//! 以为工具已经处理妥当，直到在掌机上打不开才发现。于是这里印的不是「支持哪些格式」
//! 这一行结论，而是**结论加它的出处**：每条声明是从哪个源码文件、哪份官方文档来的，
//! 哪天核实的，以及**它是不是已经陈旧**。

use std::fmt::Write as _;

use crate::report::{heading, human_bytes, pad};

use super::{Accepts, Claim, Filesystem, Profile, Roster, STALE_DAYS};

impl Claim {
    /// 「核实日期 + 陈不陈旧」那一格。
    #[must_use]
    pub fn stamp(&self, today: &str) -> String {
        match self.age_days(today) {
            None => format!("{}（日期读不懂，当**陈旧**算）", self.verified),
            Some(days) if days > STALE_DAYS => {
                format!("{}（{days} 天前，**陈旧**）", self.verified)
            }
            Some(days) => format!("{}（{days} 天前）", self.verified),
        }
    }
}

impl Filesystem {
    /// 目标存储那几条约束排一遍版。
    #[must_use]
    pub fn render_text(&self, today: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{}{}", pad("文件系统", 16), self.name);
        if !self.note.is_empty() {
            let _ = writeln!(out, "{}{}", pad("", 16), self.note);
        }
        let _ = writeln!(
            out,
            "{}{}",
            pad("单文件上限", 16),
            self.max_file_bytes
                .map_or_else(|| "不设限".to_string(), human_bytes),
        );
        let _ = writeln!(
            out,
            "{}{}",
            pad("文件名上限", 16),
            self.max_name_chars
                .map_or_else(|| "不设限".to_string(), |n| format!("{n} 个字符")),
        );
        let _ = writeln!(
            out,
            "{}{}",
            pad("完整路径上限", 16),
            self.max_path_chars
                .map_or_else(|| "不设限".to_string(), |n| format!("{n} 个字符")),
        );
        let _ = writeln!(
            out,
            "{}{}",
            pad("禁用字符", 16),
            if self.forbidden.is_empty() {
                "（不检查）".to_string()
            } else {
                self.forbidden.iter().collect::<String>()
            },
        );
        let _ = writeln!(
            out,
            "{}{}",
            pad("保留名", 16),
            if self.reserved_stems.is_empty() {
                "（不检查）".to_string()
            } else {
                format!("{} 个（CON / NUL / COM1…）", self.reserved_stems.len())
            },
        );
        let _ = writeln!(out, "{}{}", pad("核实日期", 16), self.claim.stamp(today));
        let _ = writeln!(out, "{}{}", pad("来源", 16), self.claim.cite);
        out
    }
}

impl Profile {
    /// 一份档案的全文，**含每条声明的出处**。
    #[must_use]
    pub fn render_text(&self, today: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "能力档案 {} ", self.name);
        let _ = writeln!(out, "{}", "═".repeat(24));
        if !self.note.is_empty() {
            let _ = writeln!(out, "{}", self.note);
        }

        heading(&mut out, "目标存储");
        out.push_str(&self.filesystem.render_text(today));

        heading(&mut out, "前端与核心吃什么");
        let _ = writeln!(out, "矩阵            {}", self.matrix.name);
        if !self.matrix.note.is_empty() {
            let _ = writeln!(out, "                {}", self.matrix.note);
        }
        if self.matrix.entries.is_empty() {
            let _ = writeln!(
                out,
                "（一条都没有——这份档案对任何平台都**不作声称**：不转换、不检查。）"
            );
        }
        for entry in &self.matrix.entries {
            let _ = writeln!(out);
            let _ = writeln!(out, "平台  {}", entry.platforms.join("、"));
            match &entry.accepts {
                Accepts::Anything => {
                    let _ = writeln!(out, "吃    **不作声称**——没查过。工具于是既不转也不报。");
                }
                Accepts::Only(set) => {
                    let list: Vec<&str> = set.iter().map(String::as_str).collect();
                    let _ = writeln!(out, "吃    {}", list.join(" "));
                }
            }
            let _ = writeln!(
                out,
                "转成  {}",
                entry
                    .convert_to
                    .map_or("（转不了就如实报出来）", super::Recipe::label),
            );
            let _ = writeln!(out, "核实  {}", entry.claim.stamp(today));
            let _ = writeln!(out, "来源  {}", entry.claim.cite);
        }

        let stale = self.stale_claims(today);
        if stale > 0 {
            let _ = writeln!(
                out,
                "\n⚠️ 这份档案里有 {stale} 条声明已经**超过 {STALE_DAYS} 天没核实**。\n\
                 模拟器一年发好几版，而矩阵错误比不转换更糟（ADR-0017）——去源码里\n\
                 核一遍，改 `capability.toml` 并把核实日期改成今天。"
            );
        }
        out
    }
}

impl Roster {
    /// 一份名册的目录：有哪些档案、各自对着什么设备、有没有陈旧的声明。
    #[must_use]
    pub fn render_text(&self, today: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "能力档案");
        let _ = writeln!(out, "{}", "═".repeat(24));
        let _ = writeln!(
            out,
            "{}{}{}说明",
            pad("名字", 20),
            pad("矩阵", 14),
            pad("文件系统", 12),
        );
        for profile in self.profiles() {
            let stale = profile.stale_claims(today);
            let _ = writeln!(
                out,
                "{}{}{}{}{}",
                pad(&profile.name, 20),
                pad(&profile.matrix.name, 14),
                pad(&profile.filesystem.name, 12),
                if stale > 0 { "⚠️ " } else { "" },
                profile.note,
            );
        }
        let _ = writeln!(
            out,
            "\n挑一份给子库用：`romcat sublibrary set <子库> --capability <名字>`\n\
             看一份的全文与出处：`romcat capability <名字>`\n\
             改一份：`romcat capability --dump-builtin capability.toml`，改完放进工作目录自动生效。"
        );
        let stale: usize = self
            .profiles()
            .iter()
            .map(|profile| profile.stale_claims(today))
            .sum();
        if stale > 0 {
            let _ = writeln!(
                out,
                "\n⚠️ 有档案带着**超过 {STALE_DAYS} 天没核实**的声明（上面打 ⚠️ 的那几行）。"
            );
        }
        out
    }
}
