//! **逐个读界面源码**的那几样：列出 `crates/gui/src` 底下每一份 `.rs`、把注释与字面量抹掉、把 `#[cfg(test)]`
//! 管着的那一段抹掉——剩下的就是会编进界面的那些代码，拿它去认写法。
//!
//! 两份「读源码守纪律」的测试共用：`colors.rs` 守「颜色只从令牌来」，`controls.rs` 守「勾选框与单选只用共用的那一枚」。
//! 各抄一份的话，哪天认注释、字面量或测试代码的办法要改，只改得动其中一份。

use std::path::{Path, PathBuf};

/// 界面源码在哪：`crates/gui/src`。
#[must_use]
pub fn 源码目录() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// `源码目录` 底下每一份 `.rs`（连子目录），按路径排好，报出来的次序每趟一样。
#[must_use]
pub fn 界面源码(源码目录: &Path) -> Vec<PathBuf> {
    fn 收(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("列得开源码目录") {
            let path = entry.expect("读得出目录项").path();
            if path.is_dir() {
                收(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    收(源码目录, &mut out);
    out.sort();
    out
}

/// 标识符里的一个字（中文标识符也算）。
pub fn 是字(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 一份源码里**会编进界面的那些代码**：注释、字符串与字符字面量、`#[cfg(test)]` 管着的那一段都抹成空格，
/// 换行照留——行号不变，报出来的第几行就是原文的第几行。
#[must_use]
pub fn 会编进界面的代码(源码: &str) -> String {
    抹掉测试代码(&抹掉注释与字面量(源码))
}

/// 注释、字符串与字符字面量抹成空格，换行照留——行号不变，括号也不会被字面量里的 `{` 带歪。
/// 生命期（`'a`）不是字符字面量，照留。
fn 抹掉注释与字面量(源码: &str) -> String {
    let s: Vec<char> = 源码.chars().collect();
    let mut out = String::with_capacity(源码.len());
    let 抹 = |out: &mut String, c: char| out.push(if c == '\n' { '\n' } else { ' ' });
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        let 下一个 = s.get(i + 1).copied();
        if c == '/' && 下一个 == Some('/') {
            while i < s.len() && s[i] != '\n' {
                抹(&mut out, s[i]);
                i += 1;
            }
        } else if c == '/' && 下一个 == Some('*') {
            let mut 层 = 0;
            while i < s.len() {
                if s[i] == '/' && s.get(i + 1) == Some(&'*') {
                    层 += 1;
                    out.push_str("  ");
                    i += 2;
                } else if s[i] == '*' && s.get(i + 1) == Some(&'/') {
                    层 -= 1;
                    out.push_str("  ");
                    i += 2;
                    if 层 == 0 {
                        break;
                    }
                } else {
                    抹(&mut out, s[i]);
                    i += 1;
                }
            }
        } else if let Some(井号) = 原始字符串开头(&s, i) {
            // r"…"、r#"…"#、br"…"：从 r 起，到引号后面跟着同样多个 # 为止。
            let 收尾: Vec<char> = std::iter::once('"')
                .chain(std::iter::repeat_n('#', 井号))
                .collect();
            let mut end = i + 2 + 井号;
            while end < s.len() && !s[end..].starts_with(&收尾) {
                end += 1;
            }
            let end = (end + 收尾.len()).min(s.len());
            for &c in &s[i..end] {
                抹(&mut out, c);
            }
            i = end;
        } else if c == '"' {
            抹(&mut out, c);
            i += 1;
            while i < s.len() && s[i] != '"' {
                if s[i] == '\\' {
                    抹(&mut out, s[i]);
                    i += 1;
                }
                if i < s.len() {
                    抹(&mut out, s[i]);
                    i += 1;
                }
            }
            if i < s.len() {
                抹(&mut out, s[i]);
                i += 1;
            }
        } else if c == '\'' && (下一个 == Some('\\') || s.get(i + 2) == Some(&'\'')) {
            // 字符字面量：'x'、'\n'、'\''、'\u{…}'。别的 ' 是生命期或标签。
            let mut end = if 下一个 == Some('\\') {
                i + 3
            } else {
                i + 2
            };
            while end < s.len() && s[end] != '\'' {
                end += 1;
            }
            let end = (end + 1).min(s.len());
            for &c in &s[i..end] {
                抹(&mut out, c);
            }
            i = end;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// `i` 上是不是一个原始字符串的开头（`r` 或 `br` / `cr` 的那个 `r`）；是的话交回 `#` 有几个。
/// `r#type` 这种原始标识符不算：`#` 后面跟的不是引号。
fn 原始字符串开头(s: &[char], i: usize) -> Option<usize> {
    if s[i] != 'r' {
        return None;
    }
    let 前面是词头 = match i.checked_sub(1).map(|j| s[j]) {
        None => true,
        Some(p) if !是字(p) => true,
        Some('b' | 'c') => i < 2 || !是字(s[i - 2]),
        Some(_) => false,
    };
    if !前面是词头 {
        return None;
    }
    let 井号 = s[i + 1..].iter().take_while(|&&c| c == '#').count();
    (s.get(i + 1 + 井号) == Some(&'"')).then_some(井号)
}

/// 一项以这些词打头，就是一个条目（模块、函数、impl……），它到 `;` 或第一段 `{…}` 收口为止，
/// 中间的逗号（泛型参数、where 子句）不算收口。
const 条目打头: [&str; 16] = [
    "pub",
    "mod",
    "fn",
    "impl",
    "struct",
    "enum",
    "union",
    "trait",
    "use",
    "const",
    "static",
    "type",
    "extern",
    "unsafe",
    "async",
    "macro_rules",
];

/// `#[cfg(test)]` 管着的那一段抹成空格（注释与字面量已经抹过，括号配得准），换行照留。
///
/// 抹到哪儿为止：从属性起，**宁可少抹、不许多抹**——多抹一截，后面那段代码里写的东西就悄悄看不见了；
/// 少抹只会让测试代码里的写法被报出来，当场看得见。
///
/// - 挂在条目上（`mod`、`fn`、`impl`……）：到第一个 `;`，或配完第一段 `{…}`。
/// - 挂在字段、match 分支、语句上：到第一个 `,` 或 `;`，或配完第一段 `{…}`。
/// - 两种都不越过外层的收口括号：碰到一个没配上的 `}` `)` `]` 就停在它前面。
///
/// 整份文件挂着 `#![cfg(test)]` 的，整份抹掉。
///
/// **认不出的两种，都是多报不是漏报**：`cfg(all(test, …))` 这类组合写法不抹；`#[cfg(test)] mod tests;`
/// 指向的那一份外置文件照样会被逐个读。真撞上了，在这里补一条。
fn 抹掉测试代码(代码: &str) -> String {
    let 抹成空白 = |s: &mut [char]| {
        for c in s {
            if *c != '\n' {
                *c = ' ';
            }
        }
    };
    let mut s: Vec<char> = 代码.chars().collect();
    if 代码.contains("#![cfg(test)]") {
        抹成空白(&mut s);
        return s.into_iter().collect();
    }
    let 属性: Vec<char> = "#[cfg(test)]".chars().collect();
    let mut i = 0;
    while i + 属性.len() <= s.len() {
        if s[i..i + 属性.len()] != 属性[..] {
            i += 1;
            continue;
        }
        let 起 = i + 属性.len();
        let 头一个词: String = s[起..]
            .iter()
            .skip_while(|c| c.is_whitespace())
            .take_while(|&&c| 是字(c))
            .collect();
        let 是条目 = 条目打头.contains(&头一个词.as_str());
        let mut end = 起;
        let mut 层 = 0usize;
        while end < s.len() {
            match s[end] {
                '(' | '[' => 层 += 1,
                '{' if 层 == 0 => {
                    // 配完这一段花括号就收口。
                    let mut 花括号 = 0usize;
                    while end < s.len() {
                        match s[end] {
                            '{' => 花括号 += 1,
                            '}' => {
                                花括号 -= 1;
                                if 花括号 == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        end += 1;
                    }
                    end += 1;
                    break;
                }
                '{' => 层 += 1,
                ')' | ']' | '}' if 层 == 0 => break,
                ')' | ']' | '}' => 层 -= 1,
                ';' if 层 == 0 => {
                    end += 1;
                    break;
                }
                ',' if 层 == 0 && !是条目 => {
                    end += 1;
                    break;
                }
                _ => {}
            }
            end += 1;
        }
        let end = end.min(s.len());
        抹成空白(&mut s[i..end]);
        i = end;
    }
    s.into_iter().collect()
}

/// 报出来时用的路径：`crates/gui/src/…`（`path` 在 `源码目录` 底下），分隔符一律 `/`。
#[must_use]
pub fn 报出来的路径(源码目录: &Path, path: &Path) -> String {
    let 相对 = path
        .strip_prefix(源码目录)
        .expect("在源码目录底下")
        .to_string_lossy()
        .replace('\\', "/");
    format!("crates/gui/src/{相对}")
}
