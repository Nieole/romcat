//! 门禁里扫词表的那一条：**新写的代码撞上 `CONTEXT.md` 里 `_Gate_` 的词，当场报红**，
//! 并说清哪一行、撞了哪个词、该用哪个正名。
//!
//! ## 词表从哪来
//!
//! 从仓库根的 `CONTEXT.md` 当场读（[`gate_words`]），**这里一个词都不抄**。「哪些词严到可以
//! 报红」是领域判断，ADR-0024 说它只许有一处实现，而那一处是词表自己的 `_Gate_` 行
//! （词表「`_Gate_` 是词表对门禁的授权」一节）。`_Gate_` 只许是它那条 `_Avoid_` 的子集，
//! 越界的词读词表时当场报错——写规格时人手核过一遍，就撞出过一处放错条的。
//!
//! ## 扫什么，不扫什么
//!
//! - **只扫新写的代码**：相对 `main` 的 merge base 以来的全部改动，**含未提交的**（暂存了的、
//!   没暂存的、还没 `git add` 的新文件）。接票的人跑门禁时改动一半已提交一半没有，只看一半都会漏。
//!   存量一处都不报。
//! - **`crates/` 底下的 `.rs`**（规格定的口径），其中的**标识符与字符串字面量**。
//!   **不扫注释与文档注释**——那里是在谈论这个词，不是拿它起名。`.scratch/` 与词表自己
//!   都是 Markdown、都不在 `crates/` 底下，天然不扫。
//! - 汉字按子串撞，英文词不分大小写。子串撞之所以不误报，是 `_Gate_` 的入选条件替它兜着：
//!   进 `_Gate_` 的词不是任何正名的一截。一个长词里套着的短词只报长的那个。
//!
//! ## 在 `main` 上、在 CI 上各看得见什么
//!
//! - **站在 `main` 上时 merge base 就是 `HEAD`**，于是退化成「只看未提交的」。这是对的，
//!   不是失效了；`docs/agents/long-jobs.md`「读回执」一节写着这一句。
//! - **拿不到历史时如实跳过**（[`Scope::Skipped`]）：浅克隆、找不到 `main`、不是 git 仓库。
//!   跳过退 0，但印明「一行代码都没扫」——不许假装扫过了。
//! - GitHub Actions 那份配置给 `actions/checkout` 配了 `fetch-depth: 0`，PR 那一趟扫得到
//!   整条分支的改动。**推到 `main` 的那一趟检出的就是 `main`，一行都不扫**：在本地合进 `main`
//!   再推上去的改动，CI 上没有这道机器看着，看着它的只有合并之前在分支上跑的那趟门禁。
//!
//! ## 它拦不住什么
//!
//! 这道机器**一次也没能「重放」第四轮撞上词表的那三次事故**：
//!
//! - **`job_of`**（挂单 `Q424`）拦不住：`job` 不在 `_Gate_` 里——一条子串规则分不开它与
//!   `default_jobs`，后者指并发度，按词表豁免第 1 类不算违规。
//! - **「输出目录」**（挂单 `Q439`）拦不住：它写在票与规格里，而这一条不扫 `.scratch/`。
//! - **`Step`**（挂单 `Q415`）拦不住：它压根不在 `_Avoid_` 里，词表禁的是中文「步骤」。
//!
//! **它拦的是将来那些多字中文近义词。** 别以为它比这更强。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// 词表里一个 `_Gate_` 词，连同它该换成的那个正名。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateWord {
    /// 撞上就报红的那个词。
    pub word: String,
    /// 该用哪个正名——`_Gate_` 那一行所在词条的词头。
    pub term: String,
    /// 那一行 `_Gate_` 在 `CONTEXT.md` 里是第几行（从 1 数）。
    pub line: usize,
}

/// 词表本身读不通：`_Gate_` 越出了它那条 `_Avoid_`、写走了样、找不到它属于哪个词条，
/// 或者整份一个 `_Gate_` 词都读不出来。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlossaryError {
    /// 出事的那一行在 `CONTEXT.md` 里是第几行（从 1 数）；整份的毛病没有行号。
    pub line: Option<usize>,
    /// 哪儿不对，一句话。
    pub message: String,
}

impl fmt::Display for GlossaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "CONTEXT.md:{line}: {}", self.message),
            None => write!(f, "CONTEXT.md: {}", self.message),
        }
    }
}

/// 从词表正文里读出全部 `_Gate_` 词。
///
/// # Errors
///
/// 词表读不通时报 [`GlossaryError`]。
pub fn gate_words(glossary: &str) -> Result<Vec<GateWord>, GlossaryError> {
    let mut out = Vec::new();
    // 眼下读到哪个词条、它那条 `_Avoid_` 是什么。换词条或换小节时两样一起清掉——
    // 一行 `_Gate_` 只认**自己那个词条**里的 `_Avoid_`，不许借上一条的。
    let mut term: Option<&str> = None;
    let mut avoid: Option<Vec<&str>> = None;
    for (index, line) in glossary.lines().enumerate() {
        let number = index + 1;
        let fail = |message: &str| GlossaryError {
            line: Some(number),
            message: message.to_string(),
        };
        if line.starts_with('#') {
            term = None;
            avoid = None;
        } else if let Some(name) = term_header(line) {
            term = Some(name);
            avoid = None;
        } else if let Some(rest) = line.strip_prefix("_Avoid_:") {
            // 同一个词条里的第二行 `_Avoid_`，多半是上面那个词头写走了样、没被认出来——
            // 照读下去，后面那行 `_Gate_` 会挂到上一个正名底下，报红时给出错的正名。
            if avoid.is_some() {
                return Err(fail(
                    "同一个词条里出现了第二行 `_Avoid_`——多半是上面的词头没写成 `**正名**:`",
                ));
            }
            avoid = Some(split_words(rest));
        } else if let Some(rest) = line.strip_prefix("_Gate_:") {
            let (Some(term), Some(avoid)) = (term, avoid.as_ref()) else {
                return Err(fail("这一行 `_Gate_` 找不到它所在词条的词头与 `_Avoid_`"));
            };
            let words = split_words(rest);
            if words.is_empty() {
                return Err(fail("这一行 `_Gate_` 一个词都没有"));
            }
            for word in words {
                if !avoid.contains(&word) {
                    return Err(fail(&format!(
                        "`_Gate_` 里的「{word}」不在它那条 `_Avoid_` 里——\
                         `_Gate_` 只许是 `_Avoid_` 的子集"
                    )));
                }
                out.push(GateWord {
                    word: word.to_string(),
                    term: term.to_string(),
                    line: number,
                });
            }
        } else if line.trim_start().starts_with("_Gate_") {
            // 悄悄跳过这一行，那几个词就再没有机器替人念，而这一条照样是绿的。
            return Err(fail(
                "这一行像是 `_Gate_`，但不是以 `_Gate_:` 开头，读不出来",
            ));
        }
    }
    if out.is_empty() {
        // 一个词都没有时这一条永远是绿的——那不是干净，是没在看。
        return Err(GlossaryError {
            line: None,
            message: "一个 `_Gate_` 词都没读出来，这一条等于没在看".to_string(),
        });
    }
    Ok(out)
}

/// 这一趟扫的是哪一段改动。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// 相对 `main` 的 merge base 以来的全部改动，含未提交的。
    SinceMergeBase {
        /// 拿哪个引用当 `main`：本地分支 `main`，没有就 `origin/main`。
        base: String,
        /// 求出来的 merge base，缩写的提交号。
        merge_base: String,
        /// merge base 就是 `HEAD`：人站在 `main` 上，或者这条分支还没有自己的提交。
        /// 两种都只看得到未提交的改动。
        on_base: bool,
    },
    /// 拿不到历史，**一行都没扫**。
    Skipped {
        /// 为什么拿不到，一句话。
        reason: String,
    },
}

/// 一处命中，连同它在哪份文件里。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// 相对仓库根的路径，`/` 分隔。
    pub path: String,
    /// 撞在哪儿、撞了什么。
    pub hit: Hit,
    /// 那一行源码，去掉首尾空白。
    pub excerpt: String,
}

/// 扫一趟的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// 词表里读出了多少个 `_Gate_` 词。
    pub gate_words: usize,
    /// 扫的是哪一段改动。
    pub scope: Scope,
    /// 扫了几份文件。
    pub files: usize,
    /// 扫了几行新写的代码。
    pub lines: usize,
    /// 撞上的每一处，按路径、行号排。
    pub findings: Vec<Finding>,
}

/// 这一条跑不下去：词表读不通，或者读不动文件、起不来 git。
#[derive(Debug)]
pub enum ScanError {
    /// 词表本身读不通。
    Glossary(GlossaryError),
    /// 读不动文件、起不来 git。
    Io(String),
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Glossary(err) => err.fmt(f),
            Self::Io(message) => f.write_str(message),
        }
    }
}

/// 在 `dir` 所在的仓库上扫一趟：读词表、拿改动、逐份文件找命中。
///
/// # Errors
///
/// 词表读不通、读不动文件、起不来 git 时报 [`ScanError`]。拿不到历史**不是**错，
/// 是 [`Scope::Skipped`]。
pub fn scan(dir: &Path) -> Result<Report, ScanError> {
    let toplevel = git(dir, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    let root = toplevel.as_deref().unwrap_or(dir);
    let glossary_path = root.join("CONTEXT.md");
    let glossary = fs::read_to_string(&glossary_path)
        .map_err(|err| ScanError::Io(format!("读不到词表 {}：{err}", glossary_path.display())))?;
    let words = gate_words(&glossary).map_err(ScanError::Glossary)?;
    let skipped = |reason: String| {
        Ok(Report {
            gate_words: words.len(),
            scope: Scope::Skipped { reason },
            files: 0,
            lines: 0,
            findings: Vec::new(),
        })
    };

    let root = match toplevel {
        Ok(root) => root,
        Err(why) => return skipped(format!("这里不是一个 git 仓库，没有历史可比（{why}）")),
    };
    if git(&root, &["rev-parse", "--is-shallow-repository"]).as_deref() == Ok("true") {
        return skipped(
            "这是一份浅克隆，只取了最近几层历史，相对 `main` 的 merge base 求不准；\
             CI 上要扫就给 `actions/checkout` 配 `fetch-depth: 0`"
                .to_string(),
        );
    }
    let Some((base, base_ref)) = [
        ("main", "refs/heads/main"),
        ("origin/main", "refs/remotes/origin/main"),
    ]
    .into_iter()
    .find(|(_, name)| git(&root, &["rev-parse", "--verify", "--quiet", name]).is_ok()) else {
        return skipped("找不到 `main`：本地分支 `main` 与 `origin/main` 都没有".to_string());
    };
    let (Ok(head), Ok(merge_base)) = (
        git(&root, &["rev-parse", "--verify", "--quiet", "HEAD"]),
        git(&root, &["merge-base", "HEAD", base_ref]),
    ) else {
        return skipped(format!("`HEAD` 与 `{base}` 求不出 merge base"));
    };

    let diff = git(
        &root,
        &[
            "-c",
            "core.quotePath=false",
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-relative",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--unified=0",
            "--find-renames",
            &merge_base,
        ],
    )
    .map_err(|why| ScanError::Io(format!("`git diff` 没跑通：{why}")))?;
    let untracked = git(&root, &["ls-files", "--others", "--exclude-standard", "-z"])
        .map_err(|why| ScanError::Io(format!("`git ls-files` 没跑通：{why}")))?;
    let untracked: BTreeSet<&str> = untracked.split('\0').filter(|p| !p.is_empty()).collect();
    let changed = changed_lines(&diff);

    let mut paths: BTreeSet<&str> = changed.keys().map(String::as_str).collect();
    paths.extend(untracked.iter().copied());
    let mut report = Report {
        gate_words: words.len(),
        scope: Scope::SinceMergeBase {
            base: base.to_string(),
            merge_base: git(&root, &["rev-parse", "--short", &merge_base])
                .unwrap_or_else(|_| merge_base.clone()),
            on_base: merge_base == head,
        },
        files: 0,
        lines: 0,
        findings: Vec::new(),
    };
    for path in paths.into_iter().filter(|path| in_scope(path)) {
        let full = root.join(path);
        let source = fs::read_to_string(&full)
            .map_err(|err| ScanError::Io(format!("读不动 {}：{err}", full.display())))?;
        // 没 `git add` 过的新文件没有 diff 可看——它整份都是新写的。
        let whole: BTreeSet<usize>;
        let lines = if untracked.contains(path) {
            whole = (1..=source.lines().count()).collect();
            &whole
        } else {
            &changed[path]
        };
        if lines.is_empty() {
            continue;
        }
        report.files += 1;
        report.lines += lines.len();
        for hit in hits(&words, &source, lines) {
            let excerpt = source.lines().nth(hit.line - 1).unwrap_or_default();
            report.findings.push(Finding {
                path: path.to_string(),
                excerpt: excerpt.trim().to_string(),
                hit,
            });
        }
    }
    Ok(report)
}

/// 扫哪些文件：`crates/` 底下的 `.rs`。
///
/// 这是规格定的口径。票面点名的「不扫 `.scratch/`、不扫词表自己」在这个口径下天然成立
/// ——它们都不在 `crates/` 底下，也都不是 `.rs`。
fn in_scope(path: &str) -> bool {
    path.starts_with("crates/") && path.ends_with(".rs")
}

/// 在 `dir` 里跑一条 git。跑通了交回 stdout（去掉末尾空白），跑不通交回一句原因。
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("起不来 git：{err}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// 从 `git diff --unified=0` 的输出里读出每份文件新写或改过的行号（改完之后那一侧的）。
///
/// 只认 `diff --git` 之后、第一个 `@@` 之前那几行里的 `+++`：正文里新加的一行若恰好以
/// `++ ` 开头，diff 里印出来也是 `+++ `，不能把它当成文件头。
fn changed_lines(diff: &str) -> BTreeMap<String, BTreeSet<usize>> {
    let mut out: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut in_header = false;
    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            in_header = true;
            current = None;
        } else if in_header && let Some(name) = line.strip_prefix("+++ ") {
            // 删掉的文件是 `+++ /dev/null`，没有 `b/` 前缀，也就没有新写的行。
            current = unquote(name.trim_end_matches('\t'))
                .strip_prefix("b/")
                .map(str::to_string);
        } else if let Some(range) = line.strip_prefix("@@ ") {
            in_header = false;
            if let (Some(path), Some(new)) = (&current, new_side(range)) {
                out.entry(path.clone()).or_default().extend(new);
            }
        }
    }
    out
}

/// `@@ -7,0 +8,2 @@` 里改完之后那一侧：从第 8 行起 2 行。个数省略时是 1，是 0 时是纯删除。
fn new_side(range: &str) -> Option<std::ops::Range<usize>> {
    let new = range.split(' ').find_map(|part| part.strip_prefix('+'))?;
    let (start, count) = match new.split_once(',') {
        Some((start, count)) => (start.parse().ok()?, count.parse().ok()?),
        None => (new.parse().ok()?, 1),
    };
    Some(start..start + count)
}

/// git 给带引号、反斜杠之类字符的路径加的那层双引号。
///
/// 已经递了 `core.quotePath=false`，汉字路径不会被转义成八进制；剩下的只有这两种转义。
/// 真撞上别的转义，拼出来的路径读不动，[`scan`] 会报错而不是悄悄漏掉那份文件。
fn unquote(name: &str) -> String {
    match name.strip_prefix('"').and_then(|it| it.strip_suffix('"')) {
        Some(inner) => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => name.to_string(),
    }
}

/// 命中落在哪一类词元上。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// 标识符：函数、类型、变量、字段、枚举支、宏名……
    Identifier,
    /// 字符串字面量——屏上真说出去的话多半在这儿。
    Literal,
}

/// 新写的代码里撞上 `_Gate_` 的一处。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// 撞在源码的第几行（从 1 数）。
    pub line: usize,
    /// 撞上的是词表里哪一个 `_Gate_` 词：词本身、该用的正名、它在 `CONTEXT.md` 的哪一行。
    pub gate: GateWord,
    /// 撞在标识符上还是字面量上。
    pub place: Place,
}

/// **门禁这一条的本体：（`_Gate_` 词表 × 改动的行）→ 命中。**
///
/// `source` 是一份 `.rs` 文件改完之后的全文，`changed` 是其中新写或改过的行号（从 1 数）。
/// 要全文而不是只要那几行，是因为一行是不是在注释里、是不是在一段跨行的字符串里，
/// 只看那一行判断不了。
#[must_use]
pub fn hits(words: &[GateWord], source: &str, changed: &BTreeSet<usize>) -> Vec<Hit> {
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let line_of = |offset: usize| line_starts.partition_point(|&start| start <= offset);

    let mut out: Vec<Hit> = Vec::new();
    for token in tokens(source) {
        // 只折 ASCII 的大小写：字节长度不变，偏移照旧对得上；汉字没有大小写。
        let folded = token.text.to_ascii_lowercase();
        let mut found: Vec<(usize, usize, &GateWord)> = Vec::new();
        for gate in words {
            let word = gate.word.to_ascii_lowercase();
            found.extend(
                folded
                    .match_indices(word.as_str())
                    .map(|(at, matched)| (at, at + matched.len(), gate)),
            );
        }
        // 套在更长的一处命中里头的短词不另报。
        let mut kept: Vec<(usize, usize, &GateWord)> = found
            .iter()
            .copied()
            .filter(|&(start, end, _)| {
                !found.iter().any(|&(outer_start, outer_end, _)| {
                    outer_start <= start
                        && end <= outer_end
                        && outer_end - outer_start > end - start
                })
            })
            .collect();
        kept.sort_by_key(|&(start, _, _)| start);
        for (start, _, gate) in kept {
            let line = line_of(token.start + start);
            let repeated = out
                .iter()
                .any(|it| it.line == line && it.gate == *gate && it.place == token.place);
            if changed.contains(&line) && !repeated {
                out.push(Hit {
                    line,
                    gate: gate.clone(),
                    place: token.place,
                });
            }
        }
    }
    out
}

/// 源码里一个要扫的词元：标识符或字符串字面量的内容。
struct Token<'a> {
    /// 在源码里的字节偏移。
    start: usize,
    text: &'a str,
    place: Place,
}

/// 把一份 `.rs` 源码切出要扫的词元。
///
/// 这不是一个完整的 Rust 词法器，只分得清这几样：标识符、各种字符串字面量（普通的、
/// `r#"…"#`、`b"…"`、`c"…"`）、字符字面量与生命周期。数字与标点跳过。
fn tokens(source: &str) -> Vec<Token<'_>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(c) = source[i..].chars().next() {
        let rest = &source[i..];
        if rest.starts_with("//") {
            // 行注释与 `///` / `//!` 文档注释：一路跳到行尾。
            i = rest.find('\n').map_or(source.len(), |at| i + at);
        } else if rest.starts_with("/*") {
            i = block_comment_end(source, i);
        } else if c == '"' {
            i = push_literal(source, i + 1, &mut out);
        } else if c == '\'' {
            i = after_quote_mark(source, i);
        } else if is_ident_start(c) {
            let end = ident_end(source, i);
            let prefix = &source[i..end];
            let raw = matches!(prefix, "r" | "br" | "cr")
                .then(|| push_raw_literal(source, end, &mut out))
                .flatten();
            if let Some(next) = raw {
                i = next;
            } else if matches!(prefix, "b" | "c") && bytes.get(end) == Some(&b'"') {
                i = push_literal(source, end + 1, &mut out);
            } else if prefix == "b" && bytes.get(end) == Some(&b'\'') {
                i = after_quote_mark(source, end);
            } else {
                out.push(Token {
                    start: i,
                    text: prefix,
                    place: Place::Identifier,
                });
                i = end;
            }
        } else if c.is_ascii_digit() {
            i = ident_end(source, i);
        } else {
            i += c.len_utf8();
        }
    }
    out
}

/// 块注释（含 `/** */` / `/*! */` 文档注释）：`start` 指着开头的 `/*`，交出配对的 `*/` 之后的位置。
///
/// Rust 的块注释**可以嵌套**，`/* 外 /* 内 */ 仍在外层 */` 里第一个 `*/` 不是结尾。
fn block_comment_end(source: &str, start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut i = start;
    while i + 1 < bytes.len() {
        match (bytes[i], bytes[i + 1]) {
            (b'/', b'*') => {
                depth += 1;
                i += 2;
            }
            (b'*', b'/') => {
                depth -= 1;
                i += 2;
                if depth == 0 {
                    return i;
                }
            }
            _ => i += 1,
        }
    }
    source.len()
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

/// 从 `start` 起一路吃标识符字符，交出第一个不是的位置。
fn ident_end(source: &str, start: usize) -> usize {
    source[start..]
        .char_indices()
        .find(|&(_, c)| !(c.is_alphanumeric() || c == '_'))
        .map_or(source.len(), |(at, _)| start + at)
}

/// 普通字符串：`start` 是开引号后面那个位置。把内容记成一个字面量，交出闭引号之后的位置。
fn push_literal<'a>(source: &'a str, start: usize, out: &mut Vec<Token<'a>>) -> usize {
    let mut chars = source[start..].char_indices();
    let mut end = source.len();
    while let Some((at, c)) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == '"' {
            end = start + at;
            break;
        }
    }
    out.push(Token {
        start,
        text: &source[start..end],
        place: Place::Literal,
    });
    (end + 1).min(source.len())
}

/// 原样字符串：`start` 是 `r` 后面那个位置。后面不是 `#…#"` 就不是原样字符串，交 `None`。
fn push_raw_literal<'a>(source: &'a str, start: usize, out: &mut Vec<Token<'a>>) -> Option<usize> {
    let hashes = source[start..].bytes().take_while(|&b| b == b'#').count();
    let open = start + hashes;
    if source.as_bytes().get(open) != Some(&b'"') {
        return None;
    }
    let content = open + 1;
    let closing = format!("\"{}", "#".repeat(hashes));
    let end = source[content..]
        .find(&closing)
        .map_or(source.len(), |at| content + at);
    out.push(Token {
        start: content,
        text: &source[content..end],
        place: Place::Literal,
    });
    Some((end + closing.len()).min(source.len()))
}

/// 一个单引号：字符字面量整个跳过；生命周期只跳过那个引号，名字照标识符扫。
fn after_quote_mark(source: &str, at: usize) -> usize {
    let mut rest = source[at + 1..].char_indices();
    match rest.next() {
        // `'\n'`、`'\''`、`'\u{4e00}'`：跳过转义的那个字符，再找闭引号。
        Some((_, '\\')) => {
            rest.next();
            rest.find(|&(_, c)| c == '\'')
                .map_or(source.len(), |(end, _)| at + 1 + end + 1)
        }
        Some((_, c)) if source[at + 1 + c.len_utf8()..].starts_with('\'') => {
            at + 1 + c.len_utf8() + 1
        }
        _ => at + 1,
    }
}

/// `**正名**:` 那样的一行是一个词条的词头，交出正名。
fn term_header(line: &str) -> Option<&str> {
    let name = line.strip_prefix("**")?.strip_suffix("**:")?;
    (!name.is_empty() && !name.contains("**")).then_some(name)
}

/// `_Avoid_` / `_Gate_` 冒号后面那一串，按顿号拆开。
///
/// 带附注的词（「合并（指导出时……）」）只取括号前那一截——附注是给人读的，不是词的一部分。
fn split_words(list: &str) -> Vec<&str> {
    list.split('、')
        .map(|item| item.split(['（', '(']).next().unwrap_or(item).trim())
        .filter(|word| !word.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ⚠️ 这里的词一律是编出来的（「近义一」「正名甲」），**不用词表里真的 `_Gate_` 词**：
    // 测试钉的是规则，不该因为真词表增删一个词就变红变绿。（`xtask/` 眼下不在这一条的
    // 扫描口径里，挂单 `Q541`；那条要是裁成连它一起扫，字面量里的真词也会让门禁先红在这儿。）

    const 一节词表: &str = "\
### 某一节

**正名甲**:
一句描述，里头提到近义一也无妨。
_Avoid_: 近义一、近义二（只在某种场合）、近义三
_Gate_: 近义一、近义二

**正名乙**:
_Avoid_: 近义四
";

    #[test]
    fn 只在近义词里没进门禁词的词不报() {
        // 三类豁免（外部专名、公开旗标、正名的一截）落在词表那一侧：够不上的词只留在
        // `_Avoid_` 里、不进 `_Gate_`。这一条只认 `_Gate_`——「近义三」「近义四」只在
        // `_Avoid_` 里，正名自己更不算撞。
        let 词 = gate_words(一节词表).expect("词表读得通");
        let 源码 = "fn 近义三函数() {}\nfn 正名甲() {}\nconst 说明: &str = \"近义四\";\n";
        let 命中 = hits(&词, 源码, &行(&[1, 2, 3]));
        assert!(命中.is_empty(), "只在 `_Avoid_` 里的词不许报：{命中:?}");
    }

    #[test]
    fn 门禁词那一行写走了样就当场报错而不是悄悄跳过() {
        // 悄悄跳过一行走了样的 `_Gate_`，那几个词就再没有机器替人念——而门禁照样是绿的。
        for 走样 in ["_Gate_：近义一", " _Gate_: 近义一", "_Gate_ : 近义一"] {
            let 词表 = format!("**正名甲**:\n_Avoid_: 近义一\n_Gate_: 近义一\n{走样}\n");
            let 错 = gate_words(&词表).expect_err("走了样的 `_Gate_` 行得报错");
            assert_eq!(错.line, Some(4), "「{走样}」：{错}");
        }
    }

    #[test]
    fn 门禁词那一行一个词都没有就当场报错() {
        let 错 = gate_words("**正名甲**:\n_Avoid_: 近义一\n_Gate_: 近义一\n_Gate_:\n")
            .expect_err("空的 `_Gate_` 行得报错");
        assert_eq!(错.line, Some(4), "{错}");
    }

    #[test]
    fn 一个词条里冒出第二行近义词就当场报错() {
        // 多半是下一个词条的词头写走了样（比如冒号写成全角）、没被认出来：那样后面那行
        // `_Gate_` 会挂到上一个正名底下，报红时给出的是错的正名。
        let 词表 =
            "**正名甲**:\n_Avoid_: 近义一\n\n**正名乙**：\n_Avoid_: 近义二\n_Gate_: 近义二\n";
        let 错 = gate_words(词表).expect_err("同一个词条里第二行 `_Avoid_` 得报错");
        assert_eq!(错.line, Some(5), "{错}");
    }

    #[test]
    fn 整份词表一个门禁词都读不出来就当场报错() {
        // 一个词都没有时这一条永远是绿的——那不是干净，是没在看。
        let 错 = gate_words("**正名甲**:\n_Avoid_: 近义一\n").expect_err("读不出门禁词得报错");
        assert_eq!(错.line, None, "{错}");
    }

    #[test]
    fn 仓库里那份真词表读得通而且读得出门禁词() {
        let 词 = gate_words(include_str!("../../CONTEXT.md"))
            .unwrap_or_else(|错| panic!("真词表读不通：{错}"));
        assert!(!词.is_empty());
    }

    #[test]
    fn 读出每个门禁词和它该换成的正名() {
        let 读出 = gate_words(一节词表).expect("词表读得通");
        assert_eq!(
            读出,
            [
                GateWord {
                    word: "近义一".to_string(),
                    term: "正名甲".to_string(),
                    line: 6,
                },
                GateWord {
                    word: "近义二".to_string(),
                    term: "正名甲".to_string(),
                    line: 6,
                },
            ]
        );
    }

    #[test]
    fn 门禁词越出它那条近义词就当场报错() {
        let 越界 = "**正名甲**:\n_Avoid_: 近义一\n_Gate_: 近义一、近义四\n";
        let 错 = gate_words(越界).expect_err("「近义四」不在那条 `_Avoid_` 里");
        assert_eq!(错.line, Some(3));
        assert!(错.message.contains("近义四"), "报错得点名越界的词：{错}");
    }

    /// 只有这几行算新写的。
    fn 行(lines: &[usize]) -> BTreeSet<usize> {
        lines.iter().copied().collect()
    }

    #[test]
    fn 新写的标识符撞词就报出哪一行哪个词该用哪个正名() {
        let 词 = gate_words(一节词表).expect("词表读得通");
        let 源码 = "\
fn 老近义二函数() {}
fn 新近义一函数() {
    let 正常 = 1;
}
";
        // 第 1 行也撞词，但它不在改动的行里——那是存量，不报。
        assert_eq!(
            hits(&词, 源码, &行(&[2, 3])),
            [Hit {
                line: 2,
                gate: GateWord {
                    word: "近义一".to_string(),
                    term: "正名甲".to_string(),
                    line: 6,
                },
                place: Place::Identifier,
            }]
        );
    }

    /// 撞在哪一行、撞的哪个词、撞在哪一类词元上——正名那一栏上一条测试验过了。
    fn 撞处(命中: &[Hit]) -> Vec<(usize, &str, Place)> {
        命中
            .iter()
            .map(|it| (it.line, it.gate.word.as_str(), it.place))
            .collect()
    }

    #[test]
    fn 字符串字面量撞词照样报() {
        let 词 = gate_words(一节词表).expect("词表读得通");
        let 源码 = r##"fn 正常() {
    println!("这里有近义二");
    let 跨行 = "第一行
近义一在第二行";
    let 引号 = '"';
    let 原样 = r#"带"引号"的近义一"#;
}
"##;
        assert_eq!(
            撞处(&hits(&词, 源码, &行(&[1, 2, 3, 4, 5, 6, 7]))),
            [
                (2, "近义二", Place::Literal),
                // 字面量从第 3 行开始，词落在第 4 行——报词真正所在的那一行。
                (4, "近义一", Place::Literal),
                // 第 5 行那个 `'"'` 是字符字面量，不许把后面当成一段字符串的开头。
                (6, "近义一", Place::Literal),
            ]
        );
    }

    #[test]
    fn 注释与文档注释里的同一个词不报() {
        // 注释里是在**谈论**那个词，不是拿它起名。
        let 词 = gate_words(一节词表).expect("词表读得通");
        let 源码 = r#"// 近义一 在行注释里
/// 近义一 在文档注释里
//! 近义二 在模块文档里
/* 近义一 /* 嵌套的 */ 近义二 仍在外层注释里 */
fn 正常() {} // 近义一 在行尾注释里
/** 近义一
   近义二 */
fn 近义一在代码里() {}
const 不是注释: &str = "// 近义二";
"#;
        assert_eq!(
            撞处(&hits(&词, 源码, &行(&[1, 2, 3, 4, 5, 6, 7, 8, 9]))),
            [
                (8, "近义一", Place::Identifier),
                (9, "近义二", Place::Literal)
            ]
        );
    }

    #[test]
    fn 英文门禁词不分大小写() {
        // 词表里的英文词是小写写的（真词表里有一个），而类型名、常量名照 Rust 的写法大写。
        let 词 =
            gate_words("**正名丙**:\n_Avoid_: flowline\n_Gate_: flowline\n").expect("词表读得通");
        let 源码 = "struct FlowLine;\nfn make_flowline() {}\nconst FLOWLINE_CAP: u8 = 0;\n";
        assert_eq!(
            撞处(&hits(&词, 源码, &行(&[1, 2, 3]))),
            [
                (1, "flowline", Place::Identifier),
                (2, "flowline", Place::Identifier),
                (3, "flowline", Place::Identifier),
            ]
        );
    }

    #[test]
    fn 长词里套着的短词与同一行的重复只报一次() {
        // 真词表里就有一条 `_Gate_` 同时列着一个词和它加一个字的长版本：
        // 撞上长的那个时，把套在里头的短词再报一遍只是噪音。
        let 词 = gate_words("**正名丁**:\n_Avoid_: 近义一、近义一号\n_Gate_: 近义一、近义一号\n")
            .expect("词表读得通");
        let 源码 = "let 近义一号 = 近义一 + 近义一;\n";
        assert_eq!(
            撞处(&hits(&词, 源码, &行(&[1]))),
            [
                (1, "近义一号", Place::Identifier),
                (1, "近义一", Place::Identifier)
            ]
        );
    }
}
