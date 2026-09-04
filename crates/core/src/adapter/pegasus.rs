//! **Pegasus 适配器**：`metadata.pegasus.txt` 的读、写与字段映射。
//!
//! 这是第一个做到**无损往返**档的适配器。首要交付不是导出，是**无损导入维护者多年
//! 手工维护的元数据**——未知键、`x-*` 扩展键、注释、字段顺序全部原样留存，
//! 否则一次往返就会蒸发这份心血（ADR-0001）。
//!
//! ## 词法：一份 Debian control file 的变体
//!
//! 官方把格式定义为基于 Debian control file 格式，首选 UTF-8 与 LF。逐行解析
//! （解析器源码 `MetaFile.cpp` 与官方开发者文档双向印证，见
//! `docs/research/metadata-formats.md` §A.2）：
//!
//! | 行长什么样 | 是什么 |
//! |---|---|
//! | **第 0 列**是 `#` | 整行注释。⚠️ 判的是**未去空白的原始行**——缩进后的 `#` 不是注释 |
//! | trim 后为空 | 空行。**它会关闭当前属性**（`close_current_attrib()`） |
//! | 顶格、含 `:` | `键: 值`。在**第一个** `:` 处切分，两侧各自 trim |
//! | 以空白开头、trim 后非空 | 续行：追加为上一个属性的下一个值 |
//! | 续行内容恰为 `.` | 一个空值，语义是段落分隔 |
//!
//! **键名一律小写化**（源码 `toLower()`），包括 `assets.*` 的资源名那一半。
//!
//! ## 为什么快照存的是逐行拆开的结构
//!
//! 无损往返靠旁路快照（ADR-0003 修订段）。但**把原文 `memcpy` 一份也叫「通过往返」**
//! ——那证不了任何事。所以 [`Snapshot`] 存的是拆到零件的行：缩进、键的原始大小写、
//! 键与冒号之间的空白、冒号与值之间的空白、值、值后的空白、行尾是 `\n` 还是 `\r\n`、
//! 文件开头有没有 BOM。渲染时从这些零件重新拼回去。
//!
//! 于是「逐字节相同」这件事，证的是**词法真的把每一样都接住了**。少接一样——
//! 比如把 `键 : 值` 里冒号前那个空格吞掉——往返当场就不成立，档位自动降到双向。
//!
//! ## 三个必须避开的写法（导出时）
//!
//! Pegasus 自己就是有损的，而且是**静默**的。生成内容时一律避开：
//!
//! - **`rating` 必须带 `%` 或写成 `0.xx`。** 源码两条正则 `^\d+%$` 与 `^\d(\.\d+)?$`，
//!   `85` 两条都不匹配，**整条丢弃**。
//! - **`players` 只写一个数。** 源码 `setPlayerCount(std::max(a, b))`，写 `1-4` 存进去
//!   是 `4`，下界永久丢失。所以下界写进 `x-` 扩展键，主键只写它存得下的那个数。
//! - **`release` 有几位精度写几位。** 写 `1985` 它补成 `1985-01-01`（那是它的损失，
//!   避不开）；但**绝不能因为它反正要补，就干脆自己写一个不知道的 1 月 1 日**。
//!
//! 用户手写的文件里已经有的这类写法，[`Parsed::lossy`] 逐条点名——那一行本来就没生效。

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::scrape::MediaKind;

use super::{
    Adapter, AdapterError, Body, Capability, Collection, Document, Entry, Game, Lossy, LossyNote,
    MediaPlacement, Parsed, PlayerCount, Preserved, ReleaseDate,
};

/// Pegasus 适配器。
#[derive(Debug, Clone, Copy, Default)]
pub struct Pegasus;

/// 首选文件名（官方原话：the preferred file name）。
pub const FILE_NAME: &str = "metadata.pegasus.txt";

/// 一行的行尾。**逐字节往返要它**：同一份文件里 CRLF 与 LF 可以混着来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    /// `\n`。官方首选。
    Lf,
    /// `\r\n`。Windows 上手改过的文件常常是这个（主力机是 Windows，ADR-0018）。
    CrLf,
    /// 文件最后一行没有换行。
    None,
}

impl Eol {
    fn text(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
            Self::None => "",
        }
    }
}

/// 一行拆开来的样子。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineKind {
    /// **第 0 列**的 `#` 起头的整行注释。
    Comment(String),
    /// trim 后为空的行。可能含空白，所以原样留着。
    Blank(String),
    /// `键: 值`。
    Attribute(Attribute),
    /// 以空白开头、trim 后非空：上一个属性的下一个值。
    Continuation {
        /// 缩进的那几个字符。
        indent: String,
        /// trim 之后的内容。
        value: String,
        /// 内容后面的空白。
        tail: String,
    },
    /// 顶格、但一个 `:` 都没有，或者冒号左边是空的。
    ///
    /// Pegasus 会警告并跳过。**原样留着**——它多半是维护者写岔了的一行，
    /// 替他删掉不是无损。
    Malformed(String),
}

/// 一个 `键: 值` 行拆开来的零件。
///
/// 拆到这个粒度不是洁癖：`键 :  值  ` 这种写法在手工维护的文件里到处都是，
/// 而 Pegasus 的解析器两侧都 trim。不把空白单独接住，写回去就少几个字符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    /// 键的**原始大小写**。查表用小写，写回去用它。
    pub key_raw: String,
    /// 键与冒号之间的空白。
    pub pad_key: String,
    /// 冒号与值之间的空白。
    pub pad_value: String,
    /// trim 之后的值。
    pub value: String,
    /// 值后面的空白。
    pub tail: String,
}

impl Attribute {
    /// 查表用的那个键：**小写**（Pegasus 的 `toLower()`）。
    #[must_use]
    pub fn key(&self) -> String {
        self.key_raw.to_lowercase()
    }
}

/// 一行：内容加行尾。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// 这一行是什么。
    pub kind: LineKind,
    /// 行尾。
    pub eol: Eol,
}

impl Line {
    /// 把这一行拼回去。
    fn render(&self, out: &mut String) {
        match &self.kind {
            LineKind::Comment(raw) | LineKind::Blank(raw) | LineKind::Malformed(raw) => {
                out.push_str(raw);
            }
            LineKind::Attribute(attribute) => {
                out.push_str(&attribute.key_raw);
                out.push_str(&attribute.pad_key);
                out.push(':');
                out.push_str(&attribute.pad_value);
                out.push_str(&attribute.value);
                out.push_str(&attribute.tail);
            }
            LineKind::Continuation {
                indent,
                value,
                tail,
            } => {
                out.push_str(indent);
                out.push_str(value);
                out.push_str(tail);
            }
        }
        out.push_str(self.eol.text());
    }
}

/// 一段占哪几行（左闭右开）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 从第几行起（`collection:` / `game:` 那一行）。
    pub start: usize,
    /// 到第几行为止（不含）。
    pub end: usize,
}

/// **旁路快照**：一份 Pegasus 文件逐行拆开的样子。
///
/// 它是无损往返的全部依据。中立模型表达不了的一切——注释、未知键、字段顺序、
/// 奇怪的空白、CRLF——都在这里躺着。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// 文件开头有没有 UTF-8 BOM。
    pub bom: bool,
    /// 全部行。
    pub lines: Vec<Line>,
    /// 每一段占哪几行，按文件顺序。段外的行（文件头的注释、末尾的空行）不在任何段里。
    pub blocks: Vec<Span>,
}

impl Snapshot {
    /// 把整份快照原样拼回去。
    #[must_use]
    pub fn render(&self) -> Vec<u8> {
        let mut out = String::new();
        if self.bom {
            out.push('\u{feff}');
        }
        for line in &self.lines {
            line.render(&mut out);
        }
        out.into_bytes()
    }
}

impl Adapter for Pegasus {
    fn name(&self) -> &'static str {
        "Pegasus"
    }

    fn ceiling(&self) -> Capability {
        Capability::LosslessRoundTrip
    }

    fn file_name(&self) -> &'static str {
        FILE_NAME
    }

    fn read(&self, bytes: &[u8]) -> Result<Parsed, AdapterError> {
        let snapshot = snapshot_of(bytes)?;
        let (doc, lossy) = fold(&snapshot);
        let preserved = Preserved {
            lines: snapshot.lines.len() as u64,
            comments: snapshot
                .lines
                .iter()
                .filter(|line| matches!(line.kind, LineKind::Comment(_)))
                .count() as u64,
            unknown_keys: doc
                .entries
                .iter()
                .map(|entry| unknown_of(&entry.body).len() as u64)
                .sum(),
            extension_keys: doc
                .entries
                .iter()
                .map(|entry| extra_of(&entry.body).len() as u64)
                .sum(),
            // **Pegasus 这一侧永远是 0。** 收藏在 `favorites.txt`、游玩统计在
            // `stats.db`，一样都不在 `metadata.pegasus.txt` 里（ADR-0006）。
            user_state: 0,
        };
        Ok(Parsed {
            doc,
            source: bytes.to_vec(),
            preserved,
            lossy,
        })
    }

    fn write(&self, doc: &Document, baseline: Option<&Parsed>) -> Result<Vec<u8>, AdapterError> {
        // **一个文件最多一个合集段。** Pegasus 的语义是「一个 `game` 会被加入到该文件中
        // 此前定义过的**所有** collection」——写文件的顺序本身就有语义。两个合集写进
        // 同一个文件，里面每个游戏就同时属于两个，而那几乎不是任何人想要的。
        // 导出因此是**一个合集一个文件**（`*.metadata.pegasus.txt` 同目录可放多个）。
        //
        // 这一条只管**新生成**的文档：有基线的那份是用户自己写的，他写了几个合集
        // 是他的事，原样还回去。
        if baseline.is_none() {
            let collections = doc
                .entries
                .iter()
                .filter(|entry| entry.collection().is_some())
                .count();
            if collections > 1 {
                return Err(AdapterError::Unrepresentable {
                    format: "Pegasus",
                    detail: format!(
                        "一个文件里写了 {collections} 个合集段。Pegasus 会把此后的每个 \
                         game 加进此前定义过的**所有**合集——一个合集一个文件才说得清"
                    ),
                });
            }
        }
        // 基线的原文在这里**重新拆一遍**。同一串字节拆两次是同一个结果，代价是一次词法
        // ——真机上 22 份文件合计 15 MiB，一趟导出多花不到一秒。换来的是跨格式的
        // [`Parsed`] 不必装着某一个适配器的私有形状（票 17 的 ES gamelist 拆出来的
        // 根本不是「行」）。
        let baseline = baseline
            .map(|parsed| snapshot_of(&parsed.source).map(|snapshot| (&parsed.doc, snapshot)))
            .transpose()?;
        Ok(render(
            doc,
            baseline.as_ref().map(|(was, snapshot)| (*was, snapshot)),
        ))
    }

    fn media_placement(
        &self,
        _rom_key: &str,
        kind: MediaKind,
        hash: &str,
        ext: &str,
    ) -> Option<MediaPlacement> {
        Some(MediaPlacement {
            // **内容寻址**：路径就是内容的哈希，与池自己的分层一模一样。理由见
            // [`crate::sync::media`] 的模块文档——文件名不是媒体的主键（ADR-0009），
            // 而 Pegasus 靠条目里写死的 `assets.*` 路径找媒体，不需要认得任何约定。
            path: format!("{MEDIA_DIR}/{}/{hash}.{ext}", hash.get(..2).unwrap_or("00")),
            slot: Some(slot_of(kind)?),
        })
    }
}

/// 媒体在子库里的落脚目录。
///
/// ASCII 且短：目标多半是 exFAT / FAT32 的 SD 卡，路径长度是稀缺资源（票 21 查的
/// 正是这个）。
/// **它有可能与主库里一个真叫 `media` 的平台目录撞上**——撞上时那条路径会被报成
/// 「落点被占」而不是被覆盖，因为工具在清单之外没有写的权利（ADR-0015）。
pub const MEDIA_DIR: &str = "media";

/// 一份媒体落在 Pegasus 的哪个**资源槽**上。
///
/// **认不出是什么的图没有槽**：不猜——猜错了就是把说明书扫描件当封面铺到掌机上。
#[must_use]
pub fn slot_of(kind: MediaKind) -> Option<&'static str> {
    Some(match kind {
        MediaKind::Cover => "boxFront",
        MediaKind::Screenshot => "screenshot",
        MediaKind::Video => "video",
        MediaKind::Other => return None,
    })
}

/// 把一份原文拆成快照。**读与写共用它**，各拆一遍必然有一天拆得不一样。
fn snapshot_of(bytes: &[u8]) -> Result<Snapshot, AdapterError> {
    let text = std::str::from_utf8(bytes).map_err(|error| AdapterError::NotUtf8 {
        path: FILE_NAME.to_string(),
        offset: error.valid_up_to(),
    })?;
    Ok(lex(text))
}

// ════════════════════════════════════════════════════════════════════════
// 词法
// ════════════════════════════════════════════════════════════════════════

/// 把原文拆成行。**只做词法，不认字段**。
fn lex(text: &str) -> Snapshot {
    let (bom, body) = match text.strip_prefix('\u{feff}') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let mut lines = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        let (raw, eol, next) = match rest.find('\n') {
            Some(at) => {
                let raw = &rest[..at];
                let (raw, eol) = match raw.strip_suffix('\r') {
                    Some(without) => (without, Eol::CrLf),
                    None => (raw, Eol::Lf),
                };
                (raw, eol, &rest[at + 1..])
            }
            None => (rest, Eol::None, ""),
        };
        lines.push(Line {
            kind: classify(raw),
            eol,
        });
        rest = next;
    }
    let blocks = spans(&lines);
    Snapshot { bom, lines, blocks }
}

/// 一行是哪一种。顺序照解析器源码，**不能重排**。
fn classify(raw: &str) -> LineKind {
    // 一、注释判的是**未去空白的原始行**（`line.startsWith('#')`）。
    // 缩进后的 `#` 因此**不是**注释——它是上一个属性的续行值。
    if raw.starts_with('#') {
        return LineKind::Comment(raw.to_string());
    }
    // 二、trim 后为空。
    if raw.trim().is_empty() {
        return LineKind::Blank(raw.to_string());
    }
    // 三、以空白开头：续行。
    if raw.starts_with([' ', '\t']) {
        let trimmed = raw.trim_start();
        let indent = raw[..raw.len() - trimmed.len()].to_string();
        let value = trimmed.trim_end();
        let tail = trimmed[value.len()..].to_string();
        return LineKind::Continuation {
            indent,
            value: value.to_string(),
            tail,
        };
    }
    // 四、顶格：`键: 值`。在**第一个** `:` 处切分。
    let Some(at) = raw.find(':') else {
        return LineKind::Malformed(raw.to_string());
    };
    let (left, right) = (&raw[..at], &raw[at + 1..]);
    let key_raw = left.trim_end();
    if key_raw.is_empty() {
        return LineKind::Malformed(raw.to_string());
    }
    let value = right.trim();
    let pad_value_len = right.len() - right.trim_start().len();
    LineKind::Attribute(Attribute {
        key_raw: key_raw.to_string(),
        pad_key: left[key_raw.len()..].to_string(),
        pad_value: right[..pad_value_len].to_string(),
        value: value.to_string(),
        tail: right[pad_value_len + value.len()..].to_string(),
    })
}

/// 每一段从哪一行到哪一行。段由 `collection:` / `game:` 开头，到下一段之前为止。
fn spans(lines: &[Line]) -> Vec<Span> {
    let mut starts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let LineKind::Attribute(attribute) = &line.kind
            && matches!(attribute.key().as_str(), "collection" | "game")
        {
            starts.push(index);
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (nth, start) in starts.iter().enumerate() {
        out.push(Span {
            start: *start,
            end: starts.get(nth + 1).copied().unwrap_or(lines.len()),
        });
    }
    out
}

// ════════════════════════════════════════════════════════════════════════
// 折成中立文档
// ════════════════════════════════════════════════════════════════════════

/// 一个属性：键加它的全部值（自己那一行的值 + 续行）。
struct Collected {
    key: String,
    values: Vec<String>,
    /// 属性行在文件里的行号（从 0 数），报**有损点**时要它。
    line: usize,
}

/// 把一段里的属性收出来。**空行关闭当前属性**，注释既不关闭也不贡献。
fn collect(lines: &[Line], span: Span) -> Vec<Collected> {
    let mut out: Vec<Collected> = Vec::new();
    let mut open = false;
    for (index, line) in lines.iter().enumerate().take(span.end).skip(span.start) {
        match &line.kind {
            LineKind::Comment(_) => {}
            LineKind::Blank(_) => open = false,
            LineKind::Malformed(_) => open = false,
            LineKind::Attribute(attribute) => {
                let mut values = Vec::new();
                if !attribute.value.is_empty() {
                    values.push(attribute.value.clone());
                }
                out.push(Collected {
                    key: attribute.key(),
                    values,
                    line: index,
                });
                open = true;
            }
            LineKind::Continuation { value, .. } => {
                if open && let Some(last) = out.last_mut() {
                    last.values.push(value.clone());
                }
            }
        }
    }
    out
}

/// 把快照折成中立文档，顺带点名格式自己会吞掉的写法。
fn fold(snapshot: &Snapshot) -> (Document, Vec<LossyNote>) {
    let mut entries = Vec::new();
    let mut lossy = Vec::new();
    for (nth, span) in snapshot.blocks.iter().enumerate() {
        let attributes = collect(&snapshot.lines, *span);
        let head = attributes
            .first()
            .map_or_else(String::new, |first| first.key.clone());
        let body = if head == "collection" {
            Body::Collection(fold_collection(&attributes, &mut lossy))
        } else {
            Body::Game(fold_game(&attributes, &mut lossy))
        };
        entries.push(Entry {
            origin: Some(nth),
            body,
        });
    }
    (Document { entries }, lossy)
}

fn joined(values: &[String]) -> String {
    // Pegasus 的 `merge_lines()`：`.` 是段落分隔（`\n\n`），普通换行折成空格。
    let mut out = String::new();
    for value in values {
        if value == "." {
            out.push_str("\n\n");
            continue;
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push(' ');
        }
        out.push_str(value);
    }
    out.trim().to_string()
}

fn single(values: &[String]) -> Option<String> {
    let text = joined(values);
    (!text.is_empty()).then_some(text)
}

fn fold_collection(attributes: &[Collected], lossy: &mut Vec<LossyNote>) -> Collection {
    let mut out = Collection::default();
    for attribute in attributes {
        match attribute.key.as_str() {
            "collection" => out.name = joined(&attribute.values),
            "shortname" => out.shortname = single(&attribute.values),
            "launch" | "command" => out.launch = single(&attribute.values),
            "summary" => out.summary = single(&attribute.values),
            "description" => out.description = single(&attribute.values),
            "directory" | "directories" => out.directories = attribute.values.clone(),
            "extension" | "extensions" => out.extensions = attribute.values.clone(),
            // 这三样合集段也收（调研 A.3 的 `m_coll_attribs`）。**漏掉它们不只是少建模**
            // ——它们会掉进 `unknown`，然后被报成「Pegasus 认不出，这一行本来就没生效」。
            // 那是**假警报**：维护者照着报告删掉那几行，他的合集配置就真没了。
            "file" | "files" => out.files = attribute.values.clone(),
            "workdir" | "cwd" => out.workdir = single(&attribute.values),
            other if is_sort_key(other) => out.sort_title = single(&attribute.values),
            other => {
                if let Some(rest) = other.strip_prefix("x-") {
                    out.extra.insert(rest.to_string(), attribute.values.clone());
                } else if let Some(kind) = asset_key(other) {
                    out.assets
                        .entry(kind.to_string())
                        .or_default()
                        .extend(attribute.values.clone());
                } else {
                    note_unknown(other, attribute, lossy, Segment::Collection);
                    out.unknown
                        .insert(other.to_string(), attribute.values.clone());
                }
            }
        }
    }
    out
}

fn fold_game(attributes: &[Collected], lossy: &mut Vec<LossyNote>) -> Game {
    let mut out = Game::default();
    for attribute in attributes {
        match attribute.key.as_str() {
            "game" => out.title = joined(&attribute.values),
            "file" | "files" => out.files.extend(attribute.values.clone()),
            "developer" | "developers" => out.developers.extend(attribute.values.clone()),
            "publisher" | "publishers" => out.publishers.extend(attribute.values.clone()),
            "genre" | "genres" => out.genres.extend(attribute.values.clone()),
            "tag" | "tags" => out.tags.extend(attribute.values.clone()),
            "summary" => out.summary = single(&attribute.values),
            "description" => out.description = single(&attribute.values),
            "launch" | "command" => out.launch = single(&attribute.values),
            "workdir" | "cwd" => out.workdir = single(&attribute.values),
            "players" => {
                let raw = joined(&attribute.values);
                out.players = parse_players(&raw);
                match out.players {
                    None => lossy.push(LossyNote {
                        line: attribute.line + 1,
                        key: attribute.key.clone(),
                        value: raw,
                        kind: Lossy::Dropped,
                        detail: "`players` 只认 `4` 或 `1-4`，别的写法 Pegasus 读不进去"
                            .to_string(),
                    }),
                    Some(players) if players.is_range() => lossy.push(LossyNote {
                        line: attribute.line + 1,
                        key: attribute.key.clone(),
                        value: raw,
                        kind: Lossy::Flattened,
                        detail: format!(
                            "Pegasus 只存最大值（源码 `setPlayerCount(std::max(a, b))`）\
                             ——写进去是 {}，下界 {} **永久丢失**",
                            players.max, players.min
                        ),
                    }),
                    Some(_) => {}
                }
            }
            "release" => {
                let raw = joined(&attribute.values);
                out.release = parse_release(&raw);
                match out.release {
                    None => lossy.push(LossyNote {
                        line: attribute.line + 1,
                        key: attribute.key.clone(),
                        value: raw,
                        kind: Lossy::Dropped,
                        detail: "`release` 只认 `YYYY` / `YYYY-MM` / `YYYY-MM-DD`，\
                                 别的写法 Pegasus 整条丢弃"
                            .to_string(),
                    }),
                    Some(date) if date.day.is_none() => lossy.push(LossyNote {
                        line: attribute.line + 1,
                        key: attribute.key.clone(),
                        value: raw,
                        kind: Lossy::Padded,
                        detail: format!(
                            "Pegasus 解析后补成 {}-{:02}-{:02}——「只精确到这一级」\
                             这件事它的模型里存不下",
                            date.year,
                            date.month.unwrap_or(1),
                            date.day.unwrap_or(1)
                        ),
                    }),
                    Some(_) => {}
                }
            }
            "rating" => {
                let raw = joined(&attribute.values);
                out.rating = parse_rating(&raw);
                if out.rating.is_none() {
                    lossy.push(LossyNote {
                        line: attribute.line + 1,
                        key: attribute.key.clone(),
                        value: raw,
                        kind: Lossy::Dropped,
                        detail: "两条正则 `^\\d+%$` 与 `^\\d(\\.\\d+)?$` 都不匹配，\
                                 Pegasus **静默丢弃**这一行——写 `85` 是最常见的一次"
                            .to_string(),
                    });
                }
            }
            other if is_sort_key(other) => out.sort_title = single(&attribute.values),
            other => {
                if let Some(rest) = other.strip_prefix("x-") {
                    out.extra.insert(rest.to_string(), attribute.values.clone());
                } else if let Some(kind) = asset_key(other) {
                    out.assets
                        .entry(kind.to_string())
                        .or_default()
                        .extend(attribute.values.clone());
                } else {
                    note_unknown(other, attribute, lossy, Segment::Game);
                    out.unknown
                        .insert(other.to_string(), attribute.values.clone());
                }
            }
        }
    }
    // **把人数的下界从扩展键里捡回来。** 这是我们自己写出去的那条保真通道
    // （见 [`PLAYERS_RANGE_KEY`]）：`players` 那一行只剩上界，区间在这里。
    // 认它的条件是**上界对得上**——对不上说明这两行说的不是同一件事，那就谁也别信。
    if let Some(range) = out
        .extra
        .get(PLAYERS_RANGE_KEY)
        .and_then(|values| values.first())
        .and_then(|value| parse_players(value))
        && out.players.is_some_and(|players| players.max == range.max)
    {
        out.players = Some(range);
    }
    out
}

/// 这是哪一种段。`regex` 这类键在两种段里的待遇正好相反，所以这个区分是必须的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Segment {
    Collection,
    Game,
}

impl Segment {
    fn label(self) -> &'static str {
        match self {
            Self::Collection => "合集",
            Self::Game => "游戏",
        }
    }
}

fn note_unknown(key: &str, attribute: &Collected, lossy: &mut Vec<LossyNote>, what: Segment) {
    // `regex` / `ignore-*` 是 Pegasus 在**合集段**认得的键，中立模型没有对应概念而已
    // （调研摘要第 2 条的「只此一家」能力）。它们不是有损点，只是这一层不建模。
    //
    // **游戏段不豁免**：`m_game_attribs` 里没有它们，写在 game 段里 Pegasus 真的会
    // 警告并忽略。两边一起豁免就是漏报——而漏报的那一行，维护者以为它在生效。
    if what == Segment::Collection && COLLECTION_ONLY_KEYS.contains(&key) {
        return;
    }
    if key.starts_with("assets.") || key.starts_with("asset.") {
        lossy.push(LossyNote {
            line: attribute.line + 1,
            key: key.to_string(),
            value: attribute.values.join(" "),
            kind: Lossy::Dropped,
            detail: "认不出这个资源类型，Pegasus 会警告并整条忽略".to_string(),
        });
        return;
    }
    lossy.push(LossyNote {
        line: attribute.line + 1,
        key: key.to_string(),
        value: attribute.values.join(" "),
        kind: Lossy::Dropped,
        detail: format!(
            "Pegasus 认不出{}段的这个键，会警告并忽略——这一行本来就没生效",
            what.label()
        ),
    });
}

/// Pegasus 认得、但中立模型没有对应概念的合集级键。
const COLLECTION_ONLY_KEYS: &[&str] = &[
    "regex",
    "ignore-regex",
    "ignore-extension",
    "ignore-extensions",
    "ignore-file",
    "ignore-files",
];

/// `sort-by` 的九个别名（源码注释为 "sort name variations"）。
///
/// **读的时候全部接受，写的时候统一用 `sort-by`。**
fn is_sort_key(key: &str) -> bool {
    matches!(
        key,
        "sort-by"
            | "sort_by"
            | "sortby"
            | "sort-title"
            | "sort_title"
            | "sorttitle"
            | "sort-name"
            | "sort_name"
            | "sortname"
    )
}

/// `assets.<类型>` / `asset.<类型>` → 规范类型名；认不出是 `None`。
fn asset_key(key: &str) -> Option<&'static str> {
    let rest = key
        .strip_prefix("assets.")
        .or_else(|| key.strip_prefix("asset."))?;
    asset_type(rest)
}

/// 资源名 → 规范类型名。**先精确匹配，再按最长前缀兜底**。
///
/// 前缀兜底是源码的行为（`str.startsWith(it.first)`），`boxFront01.png` 之类带序号的
/// 名字靠它认出来。源码遍历的是 `HashMap`，**顺序不确定**；这里按最长前缀取，
/// 于是同一个名字每次折出来是同一个类型——调研点名的那个不确定性在我们这一侧不存在。
#[must_use]
pub fn asset_type(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let mut best: Option<(&'static str, usize)> = None;
    for (canonical, aliases) in ASSET_TYPES {
        for alias in *aliases {
            if lower == *alias {
                return Some(canonical);
            }
            if lower.starts_with(alias) && best.is_none_or(|(_, len)| alias.len() > len) {
                best = Some((canonical, alias.len()));
            }
        }
    }
    best.map(|(canonical, _)| canonical)
}

/// 规范类型名 → 接受的名称。取自源码 `PegasusAssets.cpp` 的 `str_to_type()`。
///
/// 别名一律小写：键被 `toLower()` 之后查表用的就是小写。
const ASSET_TYPES: &[(&str, &[&str])] = &[
    ("boxFront", &["boxfront", "box_front", "boxart2d"]),
    ("boxBack", &["boxback", "box_back"]),
    (
        "boxSpine",
        &["boxspine", "box_spine", "boxside", "box_side"],
    ),
    ("boxFull", &["boxfull", "box_full", "box"]),
    ("cartridge", &["cartridge", "disc", "cart"]),
    ("logo", &["logo", "wheel"]),
    ("poster", &["poster", "flyer"]),
    ("marquee", &["marquee"]),
    ("bezel", &["bezel", "screenmarquee", "border"]),
    ("panel", &["panel"]),
    ("cabinetLeft", &["cabinetleft", "cabinet_left"]),
    ("cabinetRight", &["cabinetright", "cabinet_right"]),
    ("tile", &["tile"]),
    ("banner", &["banner"]),
    ("steam", &["steam", "steamgrid", "grid"]),
    ("background", &["background"]),
    ("music", &["music"]),
    ("screenshot", &["screenshot", "screenshots"]),
    ("titlescreen", &["titlescreen"]),
    ("video", &["video", "videos"]),
];

// ════════════════════════════════════════════════════════════════════════
// 三个取值域：照源码的正则读，照源码的口味写
// ════════════════════════════════════════════════════════════════════════

/// 读 `rating`。源码两条正则：`^\d+%$`（除以 100）与 `^\d(\.\d+)?$`（直接用）。
///
/// **都不匹配就是 `None`**，而 Pegasus 那边是「警告一句然后不写入」——同一件事。
#[must_use]
pub fn parse_rating(raw: &str) -> Option<f64> {
    if let Some(digits) = raw.strip_suffix('%')
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
    {
        return digits.parse::<f64>().ok().map(|value| value / 100.0);
    }
    // `^\d(\.\d+)?$`：**小数点前只允许一位数字**。`10` 与 `85` 因此都不匹配。
    let mut chars = raw.chars();
    let first = chars.next()?;
    if !first.is_ascii_digit() {
        return None;
    }
    let rest: String = chars.collect();
    if rest.is_empty() {
        return raw.parse::<f64>().ok();
    }
    let decimals = rest.strip_prefix('.')?;
    if decimals.is_empty() || !decimals.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse::<f64>().ok()
}

/// 写 `rating`。**永远带 `%` 或写成 `0.xx`**，绝不写一个裸整数。
///
/// 整百分点写成 `85%`（人一眼读得懂），带小数的写成 `0.855`——后者匹配
/// `^\d(\.\d+)?$`，前者匹配 `^\d+%$`，两条路 Pegasus 都收得下。
#[must_use]
pub fn format_rating(value: f64) -> String {
    let clamped = value.clamp(0.0, 1.0);
    let percent = clamped * 100.0;
    if (percent - percent.round()).abs() < 1e-9 {
        format!("{}%", percent.round() as i64)
    } else {
        // 小数点前只许一位，`clamp` 已经保证了；小数位取三位够用，也不会写出 `1.000`
        // 之外的越界值。
        format!("{clamped:.3}")
    }
}

/// 读 `players`。源码正则 `^(\d+)(-(\d+))?$`。
#[must_use]
pub fn parse_players(raw: &str) -> Option<PlayerCount> {
    let (min, max) = match raw.split_once('-') {
        Some((low, high)) => (low, high),
        None => (raw, raw),
    };
    if min.is_empty() || max.is_empty() {
        return None;
    }
    if !min.bytes().all(|b| b.is_ascii_digit()) || !max.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(PlayerCount {
        min: min.parse().ok()?,
        max: max.parse().ok()?,
    })
}

/// 写 `players`。**只写它存得下的那个数**。
///
/// 源码是 `setPlayerCount(std::max(a, b))`：写 `1-4` 存进去就是 `4`，下界永久丢失。
/// 与其写一个一半会被吃掉的区间，不如把 Pegasus 存得下的那半写进主键、把下界写进
/// [`PLAYERS_RANGE_KEY`] 那个 `x-` 扩展键——那是格式官方指定的保真通道。
#[must_use]
pub fn format_players(players: PlayerCount) -> String {
    players.max.to_string()
}

/// 人数区间的下界存在哪个 `x-` 扩展键里。
pub const PLAYERS_RANGE_KEY: &str = "romcat-players";

/// 读 `release`。源码正则 `^(\d{4})(-(\d{1,2}))?(-(\d{1,2}))?$`。
#[must_use]
pub fn parse_release(raw: &str) -> Option<ReleaseDate> {
    let mut parts = raw.split('-');
    let year = parts.next()?;
    if year.len() != 4 || !year.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let number = |part: Option<&str>| -> Option<Option<u8>> {
        match part {
            None => Some(None),
            Some(text) => {
                if text.is_empty() || text.len() > 2 || !text.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                text.parse::<u8>().ok().map(Some)
            }
        }
    };
    let month = number(parts.next())?;
    let day = number(parts.next())?;
    if parts.next().is_some() {
        return None;
    }
    // `1985--02` 这种：月缺日在，形态上不成立。
    if month.is_none() && day.is_some() {
        return None;
    }
    Some(ReleaseDate {
        year: year.parse().ok()?,
        month,
        day,
    })
}

/// 写 `release`：**有几位精度写几位**。
///
/// 只知道年份就写 `1985`。Pegasus 读回来会补成 `1985-01-01`——那是它的损失，避不开；
/// 但**绝不能因为它反正要补，就自己写一个谁也不知道的 1 月 1 日**。
#[must_use]
pub fn format_release(date: ReleaseDate) -> String {
    match (date.month, date.day) {
        (Some(month), Some(day)) => format!("{:04}-{month:02}-{day:02}", date.year),
        (Some(month), None) => format!("{:04}-{month:02}", date.year),
        _ => format!("{:04}", date.year),
    }
}

// ════════════════════════════════════════════════════════════════════════
// 写
// ════════════════════════════════════════════════════════════════════════

/// 生成内容用的缩进。官方示例用两个空格。
const INDENT: &str = "  ";

/// 一个中立字段在 Pegasus 里叫什么。**读的时候认全部别名，写的时候只用这一个。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Field {
    Title,
    SortTitle,
    Files,
    Developers,
    Publishers,
    Genres,
    Tags,
    Players,
    Release,
    Rating,
    Summary,
    Description,
    Launch,
    Workdir,
    Name,
    Shortname,
    Directories,
    Extensions,
    Asset(&'static str),
    Extra,
}

impl Field {
    /// 写出去时用哪个键。
    ///
    /// **读的时候认全部别名，写的时候只用这一个**（`sort-by` 那九个别名尤其如此）。
    ///
    /// [`Field::Asset`] 与 [`Field::Extra`] 不在这里：它们的键带一截**运行期才知道**的
    /// 名字（哪个资源类型、哪个扩展键），由 [`write_changed`] 就地拼。给这个函数加两个
    /// 可选参数去兼顾它们，结果是每个调用处都传 `None`，而那两支永远走不到。
    fn key(self) -> &'static str {
        match self {
            Self::Title => "game",
            Self::SortTitle => "sort-by",
            Self::Files => "files",
            Self::Developers => "developer",
            Self::Publishers => "publisher",
            Self::Genres => "genre",
            Self::Tags => "tag",
            Self::Players => "players",
            Self::Release => "release",
            Self::Rating => "rating",
            Self::Summary => "summary",
            Self::Description => "description",
            Self::Launch => "launch",
            Self::Workdir => "workdir",
            Self::Name => "collection",
            Self::Shortname => "shortname",
            Self::Directories => "directories",
            Self::Extensions => "extensions",
            // 这两支的键要运行期拼，见函数文档。
            Self::Asset(kind) => kind,
            Self::Extra => "x-",
        }
    }
}

/// 一个键落到哪个字段上。认不出（未知键）是 `None`。
fn field_of(key: &str) -> Option<Field> {
    Some(match key {
        "game" => Field::Title,
        "collection" => Field::Name,
        "shortname" => Field::Shortname,
        "file" | "files" => Field::Files,
        "developer" | "developers" => Field::Developers,
        "publisher" | "publishers" => Field::Publishers,
        "genre" | "genres" => Field::Genres,
        "tag" | "tags" => Field::Tags,
        "players" => Field::Players,
        "release" => Field::Release,
        "rating" => Field::Rating,
        "summary" => Field::Summary,
        "description" => Field::Description,
        "launch" | "command" => Field::Launch,
        "workdir" | "cwd" => Field::Workdir,
        "directory" | "directories" => Field::Directories,
        "extension" | "extensions" => Field::Extensions,
        other if is_sort_key(other) => Field::SortTitle,
        other => {
            if let Some(kind) = asset_key(other) {
                Field::Asset(kind)
            } else if other.starts_with("x-") {
                Field::Extra
            } else {
                return None;
            }
        }
    })
}

/// 一个字段在文档里的值，摊成「要写几行」的样子。
fn values_of(body: &Body, field: Field, extra: Option<&str>) -> Vec<String> {
    match (body, field) {
        (Body::Game(game), Field::Title) => vec![game.title.clone()],
        (Body::Game(game), Field::SortTitle) => game.sort_title.clone().into_iter().collect(),
        (Body::Game(game), Field::Files) => game.files.clone(),
        (Body::Game(game), Field::Developers) => game.developers.clone(),
        (Body::Game(game), Field::Publishers) => game.publishers.clone(),
        (Body::Game(game), Field::Genres) => game.genres.clone(),
        (Body::Game(game), Field::Tags) => game.tags.clone(),
        (Body::Game(game), Field::Players) => {
            game.players.map(format_players).into_iter().collect()
        }
        (Body::Game(game), Field::Release) => {
            game.release.map(format_release).into_iter().collect()
        }
        (Body::Game(game), Field::Rating) => game.rating.map(format_rating).into_iter().collect(),
        (Body::Game(game), Field::Summary) => game.summary.clone().into_iter().collect(),
        (Body::Game(game), Field::Description) => game.description.clone().into_iter().collect(),
        (Body::Game(game), Field::Launch) => game.launch.clone().into_iter().collect(),
        (Body::Game(game), Field::Workdir) => game.workdir.clone().into_iter().collect(),
        (Body::Game(game), Field::Asset(kind)) => {
            game.assets.get(kind).cloned().unwrap_or_default()
        }
        (Body::Game(game), Field::Extra) => extra
            .and_then(|name| game.extra.get(name))
            .cloned()
            .unwrap_or_default(),
        (Body::Collection(collection), Field::Name) => vec![collection.name.clone()],
        (Body::Collection(collection), Field::Shortname) => {
            collection.shortname.clone().into_iter().collect()
        }
        (Body::Collection(collection), Field::Launch) => {
            collection.launch.clone().into_iter().collect()
        }
        (Body::Collection(collection), Field::Summary) => {
            collection.summary.clone().into_iter().collect()
        }
        (Body::Collection(collection), Field::Description) => {
            collection.description.clone().into_iter().collect()
        }
        (Body::Collection(collection), Field::Directories) => collection.directories.clone(),
        (Body::Collection(collection), Field::Extensions) => collection.extensions.clone(),
        (Body::Collection(collection), Field::Files) => collection.files.clone(),
        (Body::Collection(collection), Field::Workdir) => {
            collection.workdir.clone().into_iter().collect()
        }
        (Body::Collection(collection), Field::SortTitle) => {
            collection.sort_title.clone().into_iter().collect()
        }
        (Body::Collection(collection), Field::Asset(kind)) => {
            collection.assets.get(kind).cloned().unwrap_or_default()
        }
        (Body::Collection(collection), Field::Extra) => extra
            .and_then(|name| collection.extra.get(name))
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn unknown_of(body: &Body) -> &BTreeMap<String, Vec<String>> {
    match body {
        Body::Game(game) => &game.unknown,
        Body::Collection(collection) => &collection.unknown,
    }
}

fn extra_of(body: &Body) -> &BTreeMap<String, Vec<String>> {
    match body {
        Body::Game(game) => &game.extra,
        Body::Collection(collection) => &collection.extra,
    }
}

fn assets_of(body: &Body) -> &BTreeMap<String, Vec<String>> {
    match body {
        Body::Game(game) => &game.assets,
        Body::Collection(collection) => &collection.assets,
    }
}

/// 写一个属性：一个值就写一行，多个值就写成续行。
///
/// **值里带换行的也走续行**。早先单值一律 `writeln!("{key}: {value}")`，值里那个 `\n`
/// 于是变成一行**顶格**的文字：读回来 `classify` 判它 `Malformed`，那一行里若还有半角
/// `:`（简介里的 URL 很常见）更会被当成一个新属性键——写出来的是一份**坏文件**。
/// 中文离线源的简介让这件事从偶发变成了默认必然（挂单 Q22）。
///
/// **段落分隔往返得回来，单个换行折成空格**：读那一侧的 `joined` 认 `.` 为 `\n\n`、
/// 把普通续行折成空格，这里照它的规矩写，两边就对上了。单个换行折成空格是 Pegasus
/// 这个格式本身的天花板，不是这里的疏漏——**该由能力档位说出口**（ADR-0003）。
fn write_attribute(out: &mut String, key: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    if values.len() == 1 && !values[0].contains('\n') {
        let _ = writeln!(out, "{key}: {}", values[0]);
        return;
    }
    // 多值走**续行**形式（官方文档里 `files:` 的写法）。首行的值留空，
    // 于是每个值都在自己那一行上，值里带逗号也不会被拆开。
    let _ = writeln!(out, "{key}:");
    for value in values {
        for line in value.split('\n') {
            // 空行写成 `.`——那是 Pegasus 的段落分隔记号，`joined` 读回来还它 `\n\n`。
            if line.trim().is_empty() {
                let _ = writeln!(out, "{INDENT}.");
            } else {
                let _ = writeln!(out, "{INDENT}{line}");
            }
        }
    }
}

/// 从头生成一段。
fn write_block(out: &mut String, body: &Body) {
    let order: &[Field] = match body {
        Body::Collection(_) => &[
            Field::Name,
            Field::Shortname,
            Field::SortTitle,
            Field::Directories,
            Field::Extensions,
            Field::Files,
            Field::Launch,
            Field::Workdir,
            Field::Summary,
            Field::Description,
        ],
        Body::Game(_) => &[
            Field::Title,
            Field::SortTitle,
            Field::Files,
            Field::Developers,
            Field::Publishers,
            Field::Genres,
            Field::Tags,
            Field::Players,
            Field::Release,
            Field::Rating,
            Field::Launch,
            Field::Workdir,
            Field::Summary,
            Field::Description,
        ],
    };
    for field in order {
        write_attribute(out, field.key(), &values_of(body, *field, None));
        // **人数的下界只能走扩展键。** `players` 那一行 Pegasus 存的是 `max(a, b)`，
        // 写 `1-4` 进去下界当场没了。所以主键只写它存得下的那个数，区间原样写进
        // `x-romcat-players`——那是格式官方指定的保真通道（调研 D.3 第一级），
        // 而且下一次读回来时 `fold_game` 认得它。
        if *field == Field::Players
            && let Body::Game(game) = body
            && let Some(players) = game.players.filter(|players| players.is_range())
            && !game.extra.contains_key(PLAYERS_RANGE_KEY)
        {
            write_attribute(
                out,
                &format!("x-{PLAYERS_RANGE_KEY}"),
                &[format!("{}-{}", players.min, players.max)],
            );
        }
    }
    for (kind, values) in assets_of(body) {
        write_attribute(out, &format!("assets.{kind}"), values);
    }
    for (name, values) in extra_of(body) {
        write_attribute(out, &format!("x-{name}"), values);
    }
    for (key, values) in unknown_of(body) {
        write_attribute(out, key, values);
    }
}

/// 把一份中立文档写成 Pegasus 文件。
///
/// 有基线就**以基线为准逐行还原**：只有值真的变了的那些行才重写，其余（注释、未知键、
/// 字段顺序、空白）一字不动。没有基线就整份新生成。
fn render(doc: &Document, baseline: Option<(&Document, &Snapshot)>) -> Vec<u8> {
    let Some((was, snapshot)) = baseline else {
        let mut out = String::new();
        for (nth, entry) in doc.entries.iter().enumerate() {
            if nth > 0 {
                out.push('\n');
            }
            write_block(&mut out, &entry.body);
        }
        return out.into_bytes();
    };

    // 基线里第几段 → 新文档里的哪一段。没对上的段**原样留着**，绝不删。
    let mut by_origin: BTreeMap<usize, &Entry> = BTreeMap::new();
    for entry in &doc.entries {
        if let Some(origin) = entry.origin {
            by_origin.insert(origin, entry);
        }
    }

    let mut out = String::new();
    if snapshot.bom {
        out.push('\u{feff}');
    }
    let mut line = 0usize;
    for (nth, span) in snapshot.blocks.iter().enumerate() {
        // 段与段之间的行（文件头的注释、段间空行）原样搬。
        while line < span.start {
            snapshot.lines[line].render(&mut out);
            line += 1;
        }
        match by_origin.get(&nth) {
            Some(entry) => merge_block(&mut out, snapshot, *span, &was.entries[nth], entry),
            // 新文档里没有它：**照搬**。用户手里那一段不因为工具不认得就消失。
            None => {
                for index in span.start..span.end {
                    snapshot.lines[index].render(&mut out);
                }
            }
        }
        line = span.end;
    }
    while line < snapshot.lines.len() {
        snapshot.lines[line].render(&mut out);
        line += 1;
    }

    // 新文档里基线上没有的段，接在后面。
    for entry in &doc.entries {
        if entry.origin.is_none() {
            if !out.is_empty() && !out.ends_with("\n\n") {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            write_block(&mut out, &entry.body);
        }
    }
    out.into_bytes()
}

/// 一段：照着基线那几行写，只重写真的变了的字段。
fn merge_block(out: &mut String, snapshot: &Snapshot, span: Span, was: &Entry, now: &Entry) {
    // 哪些字段变了。**没变的一律照搬原文**——这正是「逐字节相同」成立的地方。
    //
    // ⚠️ **「库里没有」不等于「用户想删掉它」。** 新文档在某个字段上是空的，就一律
    // 照搬基线那几行，绝不当成一次删除。这是这张票的首要交付落到实处的地方：维护者
    // 手打的 `developer`、他自己加的 `x-通关`、我们根本不建模的键，都不会因为
    // 「中立库里恰好没有对应的值」而在一次导出里蒸发掉。
    //
    // 代价是**这条路删不掉字段**。那是有意的：ADR-0001 的修订段把编辑收敛到工具内，
    // 而工具眼下还没有「删掉这个字段」这个动作——没有的动作不该由一次导出替用户做。
    let mut changed: Vec<(Field, Option<String>)> = Vec::new();
    let mut consider = |field: Field, extra: Option<String>| {
        let before = values_of(&was.body, field, extra.as_deref());
        let after = values_of(&now.body, field, extra.as_deref());
        if before != after && !after.is_empty() {
            changed.push((field, extra));
        }
    };
    for field in [
        Field::Title,
        Field::Name,
        Field::Shortname,
        Field::SortTitle,
        Field::Files,
        Field::Developers,
        Field::Publishers,
        Field::Genres,
        Field::Tags,
        Field::Players,
        Field::Release,
        Field::Rating,
        Field::Summary,
        Field::Description,
        Field::Launch,
        Field::Workdir,
        Field::Directories,
        Field::Extensions,
    ] {
        consider(field, None);
    }
    // 资源与 `x-` 扩展键**只看新文档有的那几个**：基线里独有的（维护者自己加的
    // `x-通关`、他手工指定的封面）一个都不碰，理由同上。
    for (kind, after) in assets_of(&now.body) {
        let before = assets_of(&was.body).get(kind).cloned().unwrap_or_default();
        if &before != after {
            changed.push((Field::Asset(""), Some(format!("assets.{kind}"))));
        }
    }
    for (name, after) in extra_of(&now.body) {
        let before = extra_of(&was.body).get(name).cloned().unwrap_or_default();
        if &before != after {
            changed.push((Field::Extra, Some(name.clone())));
        }
    }

    // 一遍走完这一段的行。
    let mut written: Vec<usize> = Vec::new();
    let mut index = span.start;
    // 段末尾的空行留到最后写——新加的字段要插在它们**前面**。
    let mut content_end = span.end;
    while content_end > span.start
        && matches!(snapshot.lines[content_end - 1].kind, LineKind::Blank(_))
    {
        content_end -= 1;
    }
    while index < content_end {
        let line = &snapshot.lines[index];
        let LineKind::Attribute(attribute) = &line.kind else {
            line.render(out);
            index += 1;
            continue;
        };
        let key = attribute.key();
        let slot = changed.iter().position(|(field, extra)| match field {
            Field::Asset(_) => extra.as_deref() == asset_slot(&key).as_deref(),
            Field::Extra => key
                .strip_prefix("x-")
                .is_some_and(|name| extra.as_deref() == Some(name)),
            other => field_of(&key) == Some(*other),
        });
        let Some(slot) = slot else {
            // 这个字段没变（或者这个键中立模型根本不认）：**整块照搬**，续行一起。
            line.render(out);
            index += 1;
            while index < content_end
                && matches!(snapshot.lines[index].kind, LineKind::Continuation { .. })
            {
                snapshot.lines[index].render(out);
                index += 1;
            }
            continue;
        };
        // 变了：在**第一次出现的位置**重写，后面同键的行连同续行一起丢掉。
        if !written.contains(&slot) {
            written.push(slot);
            write_changed(out, &changed[slot], now);
        }
        index += 1;
        while index < content_end
            && matches!(snapshot.lines[index].kind, LineKind::Continuation { .. })
        {
            index += 1;
        }
    }
    // 基线里没有、这次新出现的字段，补在内容的末尾。
    for (slot, entry) in changed.iter().enumerate() {
        if !written.contains(&slot) {
            write_changed(out, entry, now);
        }
    }
    for index in content_end..span.end {
        snapshot.lines[index].render(out);
    }
}

/// 这个键落在哪个资源槽上（`assets.boxfront` → `assets.boxFront`）。
fn asset_slot(key: &str) -> Option<String> {
    asset_key(key).map(|kind| format!("assets.{kind}"))
}

fn write_changed(out: &mut String, changed: &(Field, Option<String>), now: &Entry) {
    let (field, extra) = changed;
    match field {
        Field::Asset(_) => {
            let slot = extra.as_deref().unwrap_or_default();
            let kind = slot.strip_prefix("assets.").unwrap_or(slot);
            let values = assets_of(&now.body).get(kind).cloned().unwrap_or_default();
            write_attribute(out, slot, &values);
        }
        Field::Extra => {
            let name = extra.as_deref().unwrap_or_default();
            let values = extra_of(&now.body).get(name).cloned().unwrap_or_default();
            write_attribute(out, &format!("x-{name}"), &values);
        }
        other => write_attribute(out, other.key(), &values_of(&now.body, *other, None)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::assert_capability;

    /// 一份**照手工维护的样子**写的文件：注释、未知键、`x-` 扩展键、字段顺序乱、
    /// 缩进不齐、CRLF 混着 LF、末尾没有换行。往返要一个字节都不差。
    const 手写的: &str = "# 我的 FC 库\r\n\
        # 2019 年开始攒的，别乱动\r\n\
        \r\n\
        collection: FC\n\
        shortname: nes\n\
        extensions: nes,zip\n\
        regex: .*\\.(nes|zip)$\n\
        x-我的备注: 这一栏是给我自己看的\n\
        \n\
        game: 魂斗罗\n\
        sort-title: CONTRA\n\
        # 这一版是 1990 年买的那张卡\n\
        files:\n\
        \x20 魂斗罗.nes\n\
        \x20 魂斗罗 (备份).nes\n\
        developer : Konami\n\
        rating: 85\n\
        players: 1-2\n\
        release: 1988\n\
        没有这个键: 但它得留着\n\
        x-通关: 是\n\
        description: 第一段\n\
        \x20 .\n\
        \x20 第二段\n\
        \n\
        game: 沙罗曼蛇\n\
        file: 沙罗曼蛇.nes";

    #[test]
    fn 手工维护的文件往返一个字节都不差() {
        let assertion = assert_capability(&Pegasus, 手写的.as_bytes()).expect("读得动");
        assert!(
            assertion.identical,
            "第 {} 行分岔：原文 {:?}，写回去 {:?}",
            assertion.difference.as_ref().map_or(0, |d| d.line),
            assertion.difference.as_ref().map(|d| &d.expected),
            assertion.difference.as_ref().map(|d| &d.found),
        );
        assert_eq!(assertion.asserted, Capability::LosslessRoundTrip);
    }

    #[test]
    fn 未知键与扩展键与注释一样不少地留了下来() {
        let parsed = Pegasus.read(手写的.as_bytes()).expect("读得动");
        let game = parsed.doc.entries[1].game().expect("第二段是游戏");
        assert_eq!(
            game.unknown.get("没有这个键").map(Vec::as_slice),
            Some(["但它得留着".to_string()].as_slice()),
            "中立模型不认的键要原样留着"
        );
        assert_eq!(
            game.extra.get("通关").map(Vec::as_slice),
            Some(["是".to_string()].as_slice()),
            "`x-` 是格式官方指定的保真通道"
        );
        let collection = parsed.doc.entries[0].collection().expect("第一段是合集");
        assert!(
            collection.unknown.contains_key("regex"),
            "`regex` 是 Pegasus「只此一家」的能力，中立模型不建模但要留着"
        );
        assert_eq!(parsed.preserved.comments, 3, "三条注释都在快照里");
        assert_eq!(parsed.preserved.unknown_keys, 2, "`regex` 与 `没有这个键`");
        assert_eq!(
            parsed.preserved.extension_keys, 2,
            "`x-我的备注` 与 `x-通关`"
        );
    }

    #[test]
    fn 字段顺序原样留着() {
        // `sort-title` 写在 `files` 前面、`developer` 写在中间——这是维护者的排版，
        // 不是我们的。
        let parsed = Pegasus.read(手写的.as_bytes()).expect("读得动");
        let written = Pegasus.write(&parsed.doc, Some(&parsed)).expect("写得出");
        let text = String::from_utf8(written).expect("是 UTF-8");
        let sort = text.find("sort-title").expect("有 sort-title");
        let files = text.find("files:").expect("有 files");
        let developer = text.find("developer :").expect("有 developer");
        assert!(sort < files, "排版顺序不许被重排");
        assert!(files < developer);
    }

    #[test]
    fn 格式自己会吞掉的写法逐条点名_而且分得出轻重() {
        let parsed = Pegasus.read(手写的.as_bytes()).expect("读得动");
        let 找 = |key: &str| parsed.lossy.iter().find(|note| note.key == key);
        // 三种吃法轻重差得很远，混成一个总数会把最要命的那种淹掉。
        assert_eq!(
            找("rating").map(|note| note.kind),
            Some(Lossy::Dropped),
            "`rating: 85` Pegasus **整条丢弃**"
        );
        assert_eq!(
            找("players").map(|note| note.kind),
            Some(Lossy::Flattened),
            "`players: 1-2` 值进去了，下界没了"
        );
        assert_eq!(
            找("release").map(|note| note.kind),
            Some(Lossy::Padded),
            "`release: 1988` 年份没丢，「只知道年份」这件事丢了"
        );
        assert_eq!(
            找("没有这个键").map(|note| note.kind),
            Some(Lossy::Dropped),
            "认不出的键本来就没生效"
        );
    }

    #[test]
    fn 缩进后的井号不是注释而是续行() {
        // 判的是**未去空白的原始行**（源码 `line.startsWith('#')`）。
        let text = "game: 甲\ndescription: 一\n  # 这不是注释\n";
        let parsed = Pegasus.read(text.as_bytes()).expect("读得动");
        let game = parsed.doc.entries[0].game().expect("是游戏");
        assert_eq!(game.description.as_deref(), Some("一 # 这不是注释"));
        assert!(
            assert_capability(&Pegasus, text.as_bytes())
                .expect("读得动")
                .identical
        );
    }

    #[test]
    fn 空行关闭当前属性() {
        // 空行之后的续行不再挂到上一个属性上（源码 `close_current_attrib()`）。
        let text = "game: 甲\ndescription: 一\n\n  二\n";
        let parsed = Pegasus.read(text.as_bytes()).expect("读得动");
        let game = parsed.doc.entries[0].game().expect("是游戏");
        assert_eq!(game.description.as_deref(), Some("一"));
        assert!(
            assert_capability(&Pegasus, text.as_bytes())
                .expect("读得动")
                .identical
        );
    }

    #[test]
    fn 评分带百分号或写成零点几_绝不写裸整数() {
        // 源码两条正则：`^\d+%$` 与 `^\d(\.\d+)?$`。`85` 两条都不匹配。
        assert_eq!(parse_rating("85"), None, "Pegasus 会静默丢弃它");
        assert_eq!(parse_rating("85%"), Some(0.85));
        assert_eq!(parse_rating("0.85"), Some(0.85));
        assert_eq!(parse_rating("10"), None, "小数点前只许一位数字");
        assert_eq!(format_rating(0.85), "85%");
        assert_eq!(format_rating(1.0), "100%");
        // 写出去的一定读得回来——这是「避开会被静默丢弃的写法」的判据。
        for value in [0.0, 0.125, 0.333, 0.85, 1.0] {
            let text = format_rating(value);
            assert!(
                parse_rating(&text).is_some(),
                "{value} 写成 {text}，Pegasus 读不回来"
            );
        }
    }

    #[test]
    fn 人数的下界走扩展键_写得出去也读得回来() {
        // 主键只写 Pegasus 存得下的那个数，区间写进 `x-romcat-players`——
        // 那是格式官方指定的保真通道（调研 D.3 第一级）。
        let doc = Document {
            entries: vec![Entry::new(Body::Game(Game {
                title: "四人对战".to_string(),
                files: vec!["a.zip".to_string()],
                players: Some(PlayerCount { min: 1, max: 4 }),
                ..Game::default()
            }))],
        };
        let bytes = Pegasus.write(&doc, None).expect("写得出");
        let text = String::from_utf8(bytes.clone()).expect("是 UTF-8");
        assert!(
            text.contains("players: 4"),
            "主键只写它存得下的那个：{text}"
        );
        assert!(
            text.contains("x-romcat-players: 1-4"),
            "下界走扩展键，不然它就真没了：{text}"
        );
        // 读回来区间还在——「保真通道」不是一句话，是一条闭合的路。
        let back = Pegasus.read(&bytes).expect("读得动");
        assert_eq!(
            back.doc.entries[0].game().expect("是游戏").players,
            Some(PlayerCount { min: 1, max: 4 })
        );
        // 而且它照样逐字节往返。
        assert!(
            assert_capability(&Pegasus, &bytes)
                .expect("读得动")
                .identical
        );
    }

    #[test]
    fn 合集段的_files_与_workdir_不该被报成没生效() {
        // 调研 A.3 的 `m_coll_attribs`：合集段也收 `file`/`files`、`workdir`/`cwd`、
        // `sort-by`。把它们报成「Pegasus 认不出」是**假警报**——维护者照着删，
        // 他的合集配置就真没了。
        let text = "collection: FC
files:
  a.zip
workdir: /tmp
sort-by: FC
regex: .*
";
        let parsed = Pegasus.read(text.as_bytes()).expect("读得动");
        let collection = parsed.doc.entries[0].collection().expect("是合集");
        assert_eq!(collection.files, vec!["a.zip".to_string()]);
        assert_eq!(collection.workdir.as_deref(), Some("/tmp"));
        assert_eq!(collection.sort_title.as_deref(), Some("FC"));
        assert!(
            parsed.lossy.is_empty(),
            "一条假警报都不该有：{:#?}",
            parsed.lossy
        );

        // 反过来，`regex` 写在**游戏**段里 Pegasus 真的会忽略——那一条要报出来。
        let parsed = Pegasus
            .read("game: 甲\nregex: .*\n".as_bytes())
            .expect("读得动");
        assert_eq!(
            parsed
                .lossy
                .iter()
                .map(|note| note.key.as_str())
                .collect::<Vec<_>>(),
            vec!["regex"],
            "游戏段不豁免，漏报的那一行维护者以为它在生效"
        );
    }

    #[test]
    fn 人数只写它存得下的那个数() {
        // 源码 `setPlayerCount(std::max(a, b))`：写区间等于把下界扔了。
        assert_eq!(parse_players("1-4"), Some(PlayerCount { min: 1, max: 4 }));
        assert_eq!(parse_players("4"), Some(PlayerCount::exactly(4)));
        assert_eq!(parse_players("一到四"), None);
        assert_eq!(format_players(PlayerCount { min: 1, max: 4 }), "4");
        assert!(PlayerCount { min: 1, max: 4 }.is_range());
        assert!(!PlayerCount::exactly(2).is_range());
    }

    #[test]
    fn 日期有几位精度写几位() {
        assert_eq!(parse_release("1985"), Some(ReleaseDate::year_only(1985)));
        assert_eq!(
            parse_release("1985-3"),
            Some(ReleaseDate {
                year: 1985,
                month: Some(3),
                day: None
            })
        );
        assert_eq!(parse_release("85"), None);
        assert_eq!(
            parse_release("1985-13-40").map(format_release).as_deref(),
            Some("1985-13-40"),
            "越界的月日照读——`clamp` 是 Pegasus 那一侧的事，我们不替它改用户的字"
        );
        assert_eq!(format_release(ReleaseDate::year_only(1985)), "1985");
        assert_eq!(
            format_release(ReleaseDate {
                year: 1985,
                month: Some(3),
                day: None
            }),
            "1985-03",
            "不知道是哪一天就不许写出一个日"
        );
    }

    #[test]
    fn 资源名先精确后最长前缀() {
        assert_eq!(asset_type("box"), Some("boxFull"), "精确匹配优先");
        assert_eq!(asset_type("boxfront"), Some("boxFront"));
        assert_eq!(
            asset_type("boxFront01"),
            Some("boxFront"),
            "带序号的靠前缀认出来"
        );
        assert_eq!(asset_type("screenshot-02"), Some("screenshot"));
        assert_eq!(asset_type("没这个"), None);
        assert_eq!(
            asset_key("asset.logo"),
            Some("logo"),
            "`asset.` 前缀源码也收"
        );
    }

    #[test]
    fn 改一个字段只重写那一行() {
        let mut parsed = Pegasus.read(手写的.as_bytes()).expect("读得动");
        let Body::Game(game) = &mut parsed.doc.entries[1].body else {
            panic!("第二段是游戏");
        };
        game.title = "魂斗罗（汉化）".to_string();
        let doc = parsed.doc.clone();
        let baseline = Pegasus.read(手写的.as_bytes()).expect("读得动");
        let written = Pegasus.write(&doc, Some(&baseline)).expect("写得出");
        let text = String::from_utf8(written).expect("是 UTF-8");
        assert!(text.contains("game: 魂斗罗（汉化）"), "{text}");
        assert!(
            text.contains("没有这个键: 但它得留着"),
            "未知键还在：{text}"
        );
        assert!(
            text.contains("# 这一版是 1990 年买的那张卡"),
            "注释还在：{text}"
        );
        assert!(text.contains("rating: 85"), "没动的行一个字都不改：{text}");
    }

    #[test]
    fn 基线里有而新文档没有的段照搬不删() {
        let baseline = Pegasus.read(手写的.as_bytes()).expect("读得动");
        // 新文档只留第一段与第二段，第三段（沙罗曼蛇）整个不见了。
        let doc = Document {
            entries: baseline.doc.entries[..2].to_vec(),
        };
        let written = Pegasus.write(&doc, Some(&baseline)).expect("写得出");
        let text = String::from_utf8(written).expect("是 UTF-8");
        assert!(
            text.contains("game: 沙罗曼蛇"),
            "工具不认得的段不因此消失——那正是「一次往返蒸发心血」要避开的事：{text}"
        );
    }

    #[test]
    fn 一个文件里两个合集段写不出去() {
        let doc = Document {
            entries: vec![
                Entry::new(Body::Collection(Collection {
                    name: "甲".to_string(),
                    ..Collection::default()
                })),
                Entry::new(Body::Collection(Collection {
                    name: "乙".to_string(),
                    ..Collection::default()
                })),
            ],
        };
        // 一个 `game` 会被加进此前定义过的**所有** collection，两个合集共处一室
        // 说不清归属。
        assert!(Pegasus.write(&doc, None).is_err());
    }

    #[test]
    fn 不是utf8就不猜编码() {
        let error = Pegasus.read(&[0xC4, 0xE3]).expect_err("该拒绝");
        assert!(matches!(error, AdapterError::NotUtf8 { .. }));
    }

    #[test]
    fn 带bom的文件也往返() {
        let text = "\u{feff}game: 甲\nfile: 甲.nes\n";
        assert!(
            assert_capability(&Pegasus, text.as_bytes())
                .expect("读得动")
                .identical
        );
    }

    #[test]
    fn 带换行的简介写成续行_读回来不是坏行也不冒出新键() {
        // 中文离线源的简介带换行与空行（票 `offline-chinese-fields/03`）。早先单值一律
        // 写成一行，那个 `\n` 于是变成一行**顶格**的文字——读回来是 `Malformed`，而
        // 「更新: https://x」那一行里的半角 `:` 还会被当成一个新属性键（挂单 Q22）。
        let mut doc = Document::default();
        doc.entries.push(Entry {
            origin: None,
            body: Body::Game(Game {
                title: "魂斗罗".to_string(),
                files: vec!["魂斗罗.nes".to_string()],
                description: Some("第一段\n\n第二段 更新: https://例子/a\n收尾".to_string()),
                ..Game::default()
            }),
        });

        let written = Pegasus.write(&doc, None).expect("写得出");
        let text = String::from_utf8(written).expect("是 UTF-8");
        // 简介那几行都缩进着，没有一行是顶格的裸文字。
        assert!(text.contains("description:\n"), "走的是续行形式：{text}");
        assert!(!text.contains("\n第二段"), "第二段不该顶格：{text}");

        let back = Pegasus.read(text.as_bytes()).expect("读得回来");
        assert!(
            back.lossy.iter().all(|note| !format!("{note:?}").contains("Malformed")),
            "读回来不该有坏行：{:?}",
            back.lossy
        );
        let Body::Game(game) = &back.doc.entries[0].body else {
            panic!("是游戏段");
        };
        // **段落分隔往返得回来**；单个换行折成空格是 Pegasus 这个格式自己的天花板。
        let 简介 = game.description.as_deref().expect("简介还在");
        assert!(简介.starts_with("第一段\n\n第二段"), "段落分隔还在：{简介:?}");
        assert!(简介.contains("https://例子/a"), "带冒号的那截没丢：{简介:?}");
        // 那个半角 `:` 没有变成一个新属性键。
        assert!(game.unknown.is_empty(), "不该冒出新键：{:?}", game.unknown);
    }
}
