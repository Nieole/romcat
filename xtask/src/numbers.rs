//! 门禁里核对那几个算得出来的数的那一条：**票数、测试条数、测试目标数、ADR 份数过期时当场红，
//! 并说出对的那个数**。
//!
//! ## 判据：一条命令算得出来的数，才准写成断言
//!
//! 这四样各有一条命令数得出来，所以它们可以写死在文档里；真库那些数（多少变体、认出多少）
//! 机器碰不到，它们不进这个机制，一律带日期与出处记在 `docs/library-facts.md`
//! （票 `machine-checks-premises/04`）。
//!
//! 在这份文件之前，这四样是**手抄**的：同一个数在 README 与 `crates/gui/Cargo.toml` 各写一份，
//! 改一处得改两处，而没有任何东西盯着它们同步。那一族数**被记进挂单三次**
//! （`Q198`、`Q268`、`Q380`），README 里那句「以门禁自己的输出为准」写着也照样漂——
//! **光靠免责声明不管用**。
//!
//! ## 标记长什么样
//!
//! **单行标记、值紧跟其后**。Markdown 里是一个 HTML 注释，`.toml` 里是 `#` 注释，同一形状
//! ——注释开头 ＋ `数:` ＋ 名字 ＋ 值：
//!
//! ```text
//! <!-- 数:票数 -->147/152 张票落地      ← Markdown（注释不渲染，读者只看见那个数）
//! # 数:ADR份数 24                       ← TOML
//! ```
//!
//! 值是**标记后面那一串数字**（只认数字、`,`、`/`），到第一个别的字符为止，所以标记可以摆在
//! 一句话中间。成对的区段标记要处理「区段里被人手改过」，而单行标记的**替换与核对读的是同一份
//! 扫描结果**（[`Action`] 两支共用 [`run`] 那一趟）——这是 ADR-0024 那条「一个判断一处实现」。
//!
//! ⚠️ **注释开头不能省**：光认 `数:` 会把散文里的「总数:」之类认成标记
//! （`docs/research/switch-identification.md` 里就躺着一句），而那会让门禁红在一件与它无关的事上。
//!
//! ## 两个动作
//!
//! - `cargo xtask numbers --write`：把算出来的数写进每一处标记。
//! - `cargo xtask numbers --check`：核对，一个字节都不改；过期就退非零，**并说出对的那个数**，
//!   这样人不必自己去数。**门禁跑的是这一个。**
//!
//! ## 它排在 `test` 之后，不按「从便宜到贵」
//!
//! 门禁别的几条按从便宜到贵排，这一条例外：它要跑一趟 `cargo test … -- --list` 才数得出
//! 测试条数，而门禁那几条走的是继承 stdio 的方式、**不捕获子进程输出**，没法从 `test` 那一趟里
//! 顺手把数捞出来。排在 `test` 后面，那份编译缓存正热着，`--list` 只是把已经编好的那几十个
//! 测试二进制各起一次、把测试名字逐条印一遍——**它不重编，也不跑测试**。
//!
//! ⚠️ **那一趟跑的就是门禁 `test` 那一条**（从 `crate::gate::step` 取，后面加 `-- --list`），
//! 这儿一个参数都不自己抄：抄一份就是当场犯这一族数要治的病——`test` 哪天改了口径，
//! 数出来的会悄悄跟着换而没人报。
//!
//! ⚠️ 反过来说，**单独跑 `cargo xtask numbers --check` 而缓存是冷的时候，它要付一整趟编译**。
//! 那不是这一条慢，是那趟编译本来就欠着。
//!
//! ## 它拦不住什么
//!
//! **它只管带标记的那几处。** 谁再在第二个地方手抄一份同样的数而**不加标记**，这一条一个字
//! 都不会说——那正是 `Q198`／`Q268`／`Q380` 记的那个病的形状。今天那四样**各只有一处**，
//! 全在 `README.md`；把一个数写到第二处时，那一处也要带上标记。
//!
//! 能被这一条当场逮住的是另外几样，它们都会红而不是悄悄绿：
//!
//! - 某一样**一处标记都没有**了（[`Error::Unwatched`]）——有人把那句话连标记一起删掉。
//! - 标记里写了一个**不在这四样里**的名字（[`Error::Mark`]）。
//! - 一张票**没有行首锚定的 `Status:` 行**，或者有不止一行——那张票数不进去。
//! - `--list` 的输出格式变了：逐条数出来的与每个测试目标自己报的总数对不上，当场报错
//!   （而不是悄悄数出一个小了的数）。

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// 标记的两种开头：Markdown 的 HTML 注释、TOML 的 `#` 注释。**形状是同一个**——
/// 注释开头 ＋ `数:` ＋ 名字，名字到第一个空白为止。
const HEADS: [&str; 2] = ["<!-- 数:", "# 数:"];

/// 这个机制管的那几样数。**加一样就在这儿加一支**，别处不许再列一份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// 全仓测试条数。
    Tests,
    /// 测试目标数——也就是测试二进制的个数。
    TestTargets,
    /// 票数，写成「落地的／总共的」。
    Issues,
    /// ADR 份数。
    Adrs,
}

impl Kind {
    /// 全部四样，标记名的顺序也按这个。
    ///
    /// **加一样要动的地方，编译器会逼着走全**：这儿加一支，[`Kind::name`]、[`Kind::how`]、
    /// [`Tallies::value`] 三处 `match` 各补一臂，[`Tallies`] 加一个字段，[`count`] 里加一处
    /// 数法——漏一处就编不过。**别读成「只动一处」**；一处也漏不掉，才是这儿要的。
    pub const ALL: [Self; 4] = [Self::Tests, Self::TestTargets, Self::Issues, Self::Adrs];

    /// 标记里写的那个名字。
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tests => "测试条数",
            Self::TestTargets => "测试目标数",
            Self::Issues => "票数",
            Self::Adrs => "ADR份数",
        }
    }

    /// 这一样是怎么数出来的——报红时一起印，人要自己复核就照着敲。
    ///
    /// 前两样那一行**不是写死的**，是从门禁 `test` 那一条当场折出来的（见底下那个 `list_line`）。
    #[must_use]
    pub fn how(self) -> String {
        match self {
            Self::Tests | Self::TestTargets => list_line(),
            Self::Issues => "`.scratch/*/issues/NN-*.md` 里行首锚定的那行 `Status:`".to_string(),
            Self::Adrs => "`docs/adr/` 里 `NNNN-*.md` 的份数".to_string(),
        }
    }

    /// 名字对回哪一样；不是这四样里的交回 `None`。
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|it| it.name() == name)
    }
}

/// 那四样数眼下各是多少。**每一样只在 [`count`] 里算一处。**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tallies {
    /// 全仓测试条数。
    pub tests: usize,
    /// 测试目标数。
    pub test_targets: usize,
    /// 落地的票数（`Status:` 是 `done` 的）。
    pub issues_done: usize,
    /// 总票数。
    pub issues: usize,
    /// ADR 份数。
    pub adrs: usize,
}

impl Tallies {
    /// 某一样写进标记后面的那串字符。
    ///
    /// 测试条数带千分位逗号（`12,345` 这种写法），与 README 里原先的写法一致；
    /// 票数写成「落地的／总共的」（`12/34`）。
    #[must_use]
    pub fn value(&self, kind: Kind) -> String {
        match kind {
            Kind::Tests => thousands(self.tests),
            Kind::TestTargets => self.test_targets.to_string(),
            Kind::Issues => format!("{}/{}", self.issues_done, self.issues),
            Kind::Adrs => self.adrs.to_string(),
        }
    }
}

/// 三位一撇：`1815` → `1,815`。
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// 一处标记：在第几行、管哪一样、后面眼下写着什么。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    /// 在文件的第几行（从 1 数）。
    pub line: usize,
    /// 这一处管哪一样数。
    pub kind: Kind,
    /// 标记后面眼下写着的那串字符；一个数字都没有时是空的。
    pub written: String,
    /// `written` 在整份文本里的字节位置——[`apply`] 照它原地换掉那一段。
    span: (usize, usize),
}

/// 一处对不上的数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stale {
    /// 相对仓库根的路径，`/` 分隔。
    pub path: String,
    /// 在第几行。
    pub line: usize,
    /// 哪一样数。
    pub kind: Kind,
    /// 那儿眼下写着什么。
    pub written: String,
    /// 对的那个数。**报红时必须把它说出来，这样人不必自己去数。**
    pub right: String,
}

/// 这一趟干哪件事。两支走的是同一趟扫描，只差最后写不写回去。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// 改写：把算出来的数写进每一处标记。
    Write,
    /// 核对：过期就报，一个字节都不改。
    Check,
}

/// 跑一趟的结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// 干的哪件事。
    pub action: Action,
    /// 算出来的那四样。
    pub tallies: Tallies,
    /// 扫了几份文件。
    pub files: usize,
    /// 见到几处标记。
    pub marks: usize,
    /// 对不上的那几处。`--write` 那一支里，这几处是**刚刚被改掉**的。
    pub stale: Vec<Stale>,
}

/// 这一条跑不下去。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// 某一处标记本身写不通。
    Mark {
        /// 哪份文件，相对仓库根。
        path: String,
        /// 第几行。
        line: usize,
        /// 哪儿不对。
        message: String,
    },
    /// 数不出来：读不动文件、起不来 cargo、`--list` 的输出读不懂。
    Count(String),
    /// 这一样一处标记都没有——它等于没在看。
    Unwatched(Kind),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mark {
                path,
                line,
                message,
            } => write!(f, "{path}:{line}: {message}"),
            Self::Count(message) => f.write_str(message),
            Self::Unwatched(kind) => write!(
                f,
                "「{}」一处标记都没有——这一样等于没在看（它该怎么数：{}）",
                kind.name(),
                kind.how()
            ),
        }
    }
}

/// 跑一趟：算出那四样，扫遍带标记的文件，按 `action` 改写或核对。
///
/// # Errors
///
/// 数不出来、某处标记写不通、或者某一样一处标记都没有时报 [`Error`]。
pub fn run(root: &Path, action: Action) -> Result<Report, Error> {
    let tallies = count(root)?;
    let mut report = Report {
        action,
        tallies,
        files: 0,
        marks: 0,
        stale: Vec::new(),
    };
    let mut seen: BTreeSet<Kind> = BTreeSet::new();
    let mut 扫到的: Vec<(PathBuf, String, Vec<Mark>)> = Vec::new();
    for path in marked_files(root) {
        let shown = rel_path(root, &path);
        let text = fs::read_to_string(&path)
            .map_err(|err| Error::Count(format!("读不动 {shown}：{err}")))?;
        let marks = marks(&text).map_err(|(line, message)| Error::Mark {
            path: shown.clone(),
            line,
            message,
        })?;
        if marks.is_empty() {
            continue;
        }
        report.files += 1;
        report.marks += marks.len();
        for mark in &marks {
            seen.insert(mark.kind);
            let right = report.tallies.value(mark.kind);
            if mark.written != right {
                report.stale.push(Stale {
                    path: shown.clone(),
                    line: mark.line,
                    kind: mark.kind,
                    written: mark.written.clone(),
                    right,
                });
            }
        }
        扫到的.push((path, text, marks));
    }
    // ⭐ **一样都不许没人看。** 少了一处标记，这一条会一直绿——绿着而没在看，正是这个机制
    // 要治的那个病本身。
    //
    // ⚠️ **这一关排在写盘之前。** 一边报「这一样没人看着」、一边盘上已经被改过，那句报错
    // 自己就不成立了；在这儿被拦下时，`--write` 也是一个字节都没动。
    for kind in Kind::ALL {
        if !seen.contains(&kind) {
            return Err(Error::Unwatched(kind));
        }
    }
    if action == Action::Write {
        for (path, text, marks) in 扫到的 {
            let fixed = apply(&text, &marks, &report.tallies);
            if fixed != text {
                fs::write(&path, fixed).map_err(|err| {
                    Error::Count(format!("写不动 {}：{err}", rel_path(root, &path)))
                })?;
            }
        }
    }
    Ok(report)
}

/// 算出那四样。
///
/// # Errors
///
/// 起不来 cargo、`--list` 的输出读不懂、票或 ADR 数不出来时报 [`Error::Count`]。
pub fn count(root: &Path) -> Result<Tallies, Error> {
    let (tests, test_targets) = count_tests(root)?;
    let (issues_done, issues) = count_issues(root)?;
    let adrs = count_adrs(root)?;
    Ok(Tallies {
        tests,
        test_targets,
        issues_done,
        issues,
        adrs,
    })
}

// ── 四样各怎么数 ──

/// 数测试那一趟：**就是门禁 `test` 那一条，后面加 `-- --list`**。
///
/// ⚠️ **这儿一个参数都不自己抄。** 门禁那几条的唯一定义在 `crate::gate::steps` 里；在这儿
/// 另写一份 `cargo test --workspace --all-features`，就是当场犯这一族数要治的病——`test`
/// 那条哪天改了口径，数出来的会悄悄跟着换而没有任何东西报。口径一致也正是它排在 `test`
/// 后面**一行都不重编**的原因。
fn list_step() -> crate::gate::Step {
    let mut step =
        crate::gate::step("test", crate::gate::Limits::default()).expect("门禁里有 `test` 这一条");
    step.args.push("--".to_string());
    step.args.push("--list".to_string());
    step
}

/// 上面那一条印出来是什么样。报红时那句「怎么数出来的」印的就是它。
fn list_line() -> String {
    list_step().display()
}

/// 测试条数与测试目标数。
///
/// 子进程的 stderr 直通终端：缓存冷的时候那一片 `Compiling` 是人该看见的。
fn count_tests(root: &Path) -> Result<(usize, usize), Error> {
    let mut command = list_step().command_in(root);
    let out = command
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|err| Error::Count(format!("起不来 cargo：{err}")))?;
    if !out.status.success() {
        return Err(Error::Count(format!(
            "`{}` 没跑通（原因印在上面）",
            list_line()
        )));
    }
    read_list(&String::from_utf8_lossy(&out.stdout)).map_err(Error::Count)
}

/// 从 `--list` 的输出里读出（测试条数, 测试目标数）。
///
/// 每个测试二进制先逐条印 `名字: test`，最后印一行 `N tests, M benchmarks`。
/// **两边对着数**：逐条数出来的与每个目标自己报的总数对不上，就是这份格式变了——那时报错，
/// 不许悄悄交出一个小了的数。
fn read_list(text: &str) -> Result<(usize, usize), String> {
    let mut tests = 0usize;
    let mut targets = 0usize;
    let mut summed = 0usize;
    for line in text.lines() {
        if line.ends_with(": test") {
            tests += 1;
        } else if let Some(n) = summary(line) {
            targets += 1;
            summed += n;
        }
    }
    if targets == 0 {
        return Err("`-- --list` 一个测试目标都没数出来，这一样等于没在看".to_string());
    }
    if tests != summed {
        return Err(format!(
            "`-- --list` 的输出读不懂：逐条数出 {tests} 条，而那 {targets} 个测试目标自己报的是 \
             {summed} 条。多半是这份格式变了——先把这儿的读法改对，别让它数出一个小了的数"
        ));
    }
    Ok((tests, targets))
}

/// `12 tests, 0 benchmarks` 这一行里的 `12`；不是这个形状的交回 `None`。
fn summary(line: &str) -> Option<usize> {
    let (tests, benches) = line.split_once(", ")?;
    if !benches.ends_with(" benchmarks") && !benches.ends_with(" benchmark") {
        return None;
    }
    tests
        .strip_suffix(" tests")
        .or_else(|| tests.strip_suffix(" test"))?
        .parse()
        .ok()
}

/// 票数：`.scratch/*/issues/NN-*.md` 一份一张票，落地的是 `Status:` 为 `done` 的那些。
///
/// ⚠️ **文件名那两位数字不是摆设**：`issues/` 底下允许躺着维护者自己的非票据文件
/// （`.scratch/rom-metadata-automation/issues/.gitignore` 明写着这件事），照 `NN-` 认才不会把
/// 它们数成票。
fn count_issues(root: &Path) -> Result<(usize, usize), Error> {
    let scratch = root.join(".scratch");
    let mut done = 0usize;
    let mut all = 0usize;
    let features = fs::read_dir(&scratch)
        .map_err(|err| Error::Count(format!("读不动 {}：{err}", rel_path(root, &scratch))))?;
    let mut dirs: Vec<PathBuf> = features
        .filter_map(Result::ok)
        .map(|it| it.path().join("issues"))
        .filter(|it| it.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let mut files: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|err| Error::Count(format!("读不动 {}：{err}", rel_path(root, &dir))))?
            .filter_map(Result::ok)
            .map(|it| it.path())
            .filter(|it| is_issue(it))
            .collect();
        files.sort();
        for file in files {
            let text = fs::read_to_string(&file)
                .map_err(|err| Error::Count(format!("读不动 {}：{err}", rel_path(root, &file))))?;
            let status = status_of(&text)
                .map_err(|why| Error::Count(format!("{}：{why}", rel_path(root, &file))))?;
            all += 1;
            if status == "done" {
                done += 1;
            }
        }
    }
    if all == 0 {
        return Err(Error::Count(
            "`.scratch/*/issues/` 底下一张票都没数出来，这一样等于没在看".to_string(),
        ));
    }
    Ok((done, all))
}

/// 这份文件是不是一张票：`NN-*.md`。
fn is_issue(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|it| it.to_str()) else {
        return false;
    };
    name.ends_with(".md")
        && name.len() > 3
        && name.as_bytes()[..2].iter().all(u8::is_ascii_digit)
        && name.as_bytes()[2] == b'-'
}

/// 一张票的 `Status:`，**行首锚定**。
///
/// ⚠️ **锚定不是讲究，是实测出来的**：松一点的匹配会把票末尾正文里出现的 `done` 也算进去
/// （写规格那天实测撞过：松匹配数出 88，锚定数出 87）。所以这里只认顶格写的那一行，
/// 缩进的、引用块里的、句子中间的一律不算。
///
/// 没有那一行、或者有不止一行，都当场报——那张票数不进去，而「数不进去」不许悄悄发生。
fn status_of(text: &str) -> Result<&str, String> {
    let mut found: Option<&str> = None;
    for line in text.lines() {
        let Some(rest) = line
            .strip_prefix("**Status:**")
            .or_else(|| line.strip_prefix("Status:"))
        else {
            continue;
        };
        if found.is_some() {
            return Err("这张票有不止一行行首锚定的 `Status:`，数不准".to_string());
        }
        found = Some(rest.trim().trim_matches(['`', '*', ' ']));
    }
    found.ok_or_else(|| {
        "这张票没有行首锚定的 `Status:` 行（见 `docs/agents/issue-tracker.md`），数不进去"
            .to_string()
    })
}

/// ADR 份数：`docs/adr/` 里 `NNNN-*.md` 的份数。
fn count_adrs(root: &Path) -> Result<usize, Error> {
    let dir = root.join("docs").join("adr");
    let count = fs::read_dir(&dir)
        .map_err(|err| Error::Count(format!("读不动 {}：{err}", rel_path(root, &dir))))?
        .filter_map(Result::ok)
        .filter(|it| {
            it.file_name().to_str().is_some_and(|name| {
                name.ends_with(".md")
                    && name.len() > 5
                    && name.as_bytes()[..4].iter().all(u8::is_ascii_digit)
                    && name.as_bytes()[4] == b'-'
            })
        })
        .count();
    if count == 0 {
        return Err(Error::Count(
            "`docs/adr/` 里一份 ADR 都没数出来，这一样等于没在看".to_string(),
        ));
    }
    Ok(count)
}

// ── 标记怎么扫、怎么换 ──

/// 扫哪些文件：仓库里的 `.md` 与 `.toml`。
///
/// **不扫 `.scratch/`**：票与规格里记的是当时那一趟的实测读数，带着日期与出处
/// （票 `machine-checks-premises/04`），不该被这条命令改写；写票的人要引一个标记的样子，
/// 也得在那儿写得出来。`target/` 与 `.git/` 同样不扫。
fn marked_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|it| it.to_str()) else {
                continue;
            };
            if path.is_dir() {
                if !matches!(name, "target" | ".git" | ".scratch") {
                    stack.push(path);
                }
            } else if name.ends_with(".md") || name.ends_with(".toml") {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// 路径印出来的样子：相对仓库根，`/` 分隔。
fn rel_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// **这一条的本体：一份文本 → 里头每一处标记。**
///
/// 形状见模块文档。名字跟在 `数:` 后面、到第一个空白为止；值是再往后那一串数字
/// （数字、`,`、`/`），中间允许隔一个 Markdown 注释的收尾 `-->` 与空白。
///
/// 出错时交回（第几行, 哪儿不对）。
fn marks(text: &str) -> Result<Vec<Mark>, (usize, String)> {
    let mut out = Vec::new();
    let mut offset = 0usize;
    for (index, raw) in text.split('\n').enumerate() {
        let line_start = offset;
        offset += raw.len() + 1;
        let number = index + 1;
        let mut from = 0usize;
        while let Some((at, head)) = next_head(raw, from) {
            let after = at + head.len();
            let name: String = raw[after..]
                .chars()
                .take_while(|it| !it.is_whitespace())
                .collect();
            let Some(kind) = Kind::from_name(&name) else {
                return Err((
                    number,
                    format!(
                        "标记里的「{name}」不是这个机制管的那几样（{}）",
                        Kind::ALL.map(Kind::name).join("、")
                    ),
                ));
            };
            let mut cursor = after + name.len();
            cursor += blank_len(&raw[cursor..]);
            if raw[cursor..].starts_with("-->") {
                cursor += "-->".len();
            }
            // 「紧跟其后」的那个位置。值真在那儿时从那儿读；值与标记之间隔着空白也照读
            // （TOML 那一种注释就隔着一个空格），但**数被人删光时补回去的位置是这一个**
            // ——补到空白后面会把那个空格挤到数的前头。
            let tight = cursor;
            let value_at = tight + blank_len(&raw[tight..]);
            let written: String = raw[value_at..]
                .chars()
                .take_while(|it| it.is_ascii_digit() || *it == ',' || *it == '/')
                .collect();
            let at = if written.is_empty() { tight } else { value_at };
            out.push(Mark {
                line: number,
                kind,
                span: (line_start + at, line_start + at + written.len()),
                written,
            });
            from = value_at.max(at + 1);
        }
    }
    Ok(out)
}

/// 一段文本开头那一截空白有多少个字节。
fn blank_len(rest: &str) -> usize {
    rest.len() - rest.trim_start().len()
}

/// 从 `from` 起找下一个标记开头，交回（它在这一行的字节位置, 是哪一种开头）。
fn next_head(line: &str, from: usize) -> Option<(usize, &'static str)> {
    HEADS
        .iter()
        .filter_map(|head| line[from..].find(head).map(|at| (from + at, *head)))
        .min_by_key(|it| it.0)
}

/// 照扫出来的那几处，把每一处的值原地换成算出来的那个。**别处一个字节都不动。**
fn apply(text: &str, marks: &[Mark], tallies: &Tallies) -> String {
    let mut out = String::with_capacity(text.len());
    let mut cut = 0usize;
    for mark in marks {
        let (start, end) = mark.span;
        out.push_str(&text[cut..start]);
        out.push_str(&tallies.value(mark.kind));
        cut = end;
    }
    out.push_str(&text[cut..]);
    out
}

#[cfg(test)]
mod 测试 {
    use super::*;

    /// ⚠️ 这几个数是**编出来的**，理由同词表那几条单元测试里那份编出来的词表：
    /// 测试钉的是规则，不该因为仓库里真的多了一张票、多了一条测试就变红变绿。
    fn 四样() -> Tallies {
        Tallies {
            tests: 1234,
            test_targets: 56,
            issues_done: 7,
            issues: 8,
            adrs: 9,
        }
    }

    #[test]
    fn 标记读得出名字与眼下那个值() {
        let 文本 = "**<!-- 数:票数 -->11/22 张票落地**，<!-- 数:测试条数 -->3,333 条测试。\n";
        let 读出 = marks(文本).expect("这一行读得通");
        assert_eq!(
            读出
                .iter()
                .map(|it| (it.kind, it.written.as_str()))
                .collect::<Vec<_>>(),
            [(Kind::Issues, "11/22"), (Kind::Tests, "3,333")],
            "一行里摆两处标记要各读各的，值到第一个非数字为止"
        );
    }

    #[test]
    fn toml_注释里同一个形状照样读得出来() {
        // ⭐ 标记形状在两种文件里是同一个：Markdown 用 HTML 注释、TOML 用 `#` 注释，
        // 都是「单行标记、值紧跟其后」，读它的也是同一段代码。
        let 文本 = "# 这一族数由一条命令写。\n# 数:ADR份数 33\n";
        let 读出 = marks(文本).expect("这一行读得通");
        assert_eq!(读出.len(), 1);
        assert_eq!(读出[0].line, 2);
        assert_eq!((读出[0].kind, 读出[0].written.as_str()), (Kind::Adrs, "33"));
    }

    #[test]
    fn 改写只动那几处值别处一个字节不动() {
        let 文本 =
            "前面。**<!-- 数:票数 -->11/22 张票落地**，<!-- 数:ADR份数 -->33 份 ADR。后面。\n";
        let 改完 = apply(文本, &marks(文本).expect("读得通"), &四样());
        assert_eq!(
            改完,
            "前面。**<!-- 数:票数 -->7/8 张票落地**，<!-- 数:ADR份数 -->9 份 ADR。后面。\n"
        );
    }

    #[test]
    fn 标记后面一个数字都没有时照样补得上() {
        // 有人把数删了、标记留着：那也是「对不上」，`--write` 要补得回来。
        let 文本 = "<!-- 数:测试目标数 --> 个测试目标\n";
        let 读出 = marks(文本).expect("读得通");
        assert_eq!(读出[0].written, "");
        assert_eq!(
            apply(文本, &读出, &四样()),
            "<!-- 数:测试目标数 -->56 个测试目标\n"
        );
    }

    #[test]
    fn 标记里写了个不认识的名字就当场报() {
        // ⭐ 悄悄跳过一个不认识的名字，那一处就再没有东西看着，而这一条照样绿。
        let err = marks("<!-- 数:真库变体数 -->3,000\n").expect_err("必须报出来");
        assert_eq!(err.0, 1);
        assert!(
            err.1.contains("真库变体数"),
            "报错得点名是哪个名字：{}",
            err.1
        );
    }

    #[test]
    fn 票数只认行首锚定那一行而松匹配那种数法数出的是另一个数() {
        // ⚠️ **这条是反向钉子。** 松一点的匹配（比如「正文里出现 `done` 就算」）会把下面
        // 这张票数成落地的——实测撞过，松匹配数出 88，锚定数出 87。
        let 票 = concat!(
            "# 一张票\n\n",
            "**Status:** ready-for-agent\n\n",
            "收尾时把这一行改成 done 就算落地。\n",
            "> **Status:** done\n",
            "  **Status:** done\n",
        );
        assert_eq!(status_of(票).expect("读得出来"), "ready-for-agent");
        assert_eq!(
            票.matches("done").count(),
            3,
            "这张票正文里真的躺着三个 `done`——松匹配会把它数成落地的"
        );
    }

    #[test]
    fn 票上没有行首锚定的_status_就报而不是悄悄不数() {
        assert!(status_of("# 一张票\n\n没有状态行。\n").is_err());
        assert!(status_of("**Status:** done\n**Status:** done\n").is_err());
    }

    #[test]
    fn 哪个文件算一张票() {
        assert!(is_issue(Path::new("/x/.scratch/q/issues/03-abc.md")));
        assert!(!is_issue(Path::new("/x/.scratch/q/issues/spec.md")));
        assert!(!is_issue(Path::new(
            "/x/.scratch/q/issues/AI短剧行业调研.md"
        )));
        assert!(!is_issue(Path::new("/x/.scratch/q/issues/.gitignore")));
    }

    #[test]
    fn 列出全部测试那一趟逐条数与每个目标自报的总数要对得上() {
        let 输出 = "\
甲: test
乙: test
2 tests, 0 benchmarks
丙: test
1 test, 0 benchmarks
0 tests, 0 benchmarks
";
        assert_eq!(read_list(输出).expect("读得懂"), (3, 3));

        // ⭐ 格式变了要报错，不许悄悄交出一个小了的数——那正是「绿着但没在看」。
        let 变样了 = "甲: test\n乙: test\n5 tests, 0 benchmarks\n";
        let err = read_list(变样了).expect_err("对不上就得报");
        assert!(err.contains("读不懂"), "{err}");
        assert!(read_list("什么都没有\n").is_err(), "一个目标都没有也得报");
    }

    #[test]
    fn 三位一撇() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(70), "70");
        assert_eq!(thousands(1815), "1,815");
        assert_eq!(thousands(1234567), "1,234,567");
    }
}
