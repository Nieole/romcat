//! **界面里没有写死的颜色**（票 `gate-and-tests/06`，挂单 `Q486`）。
//!
//! 颜色只从令牌来（`crates/gui/src/tokens.toml`，见 `look.rs` 头上「颜色与字号只从令牌来」）：
//! 两套主题各取一套，改一个颜色只改令牌那一份。一处写死的 `Color32::WHITE` 在亮色主题里看不出错，
//! 到了暗色主题就不跟令牌走——这条纪律从前只靠走查守，已经滑出过好几处。
//!
//! 这里逐个读界面源码（`crates/gui/src` 底下每一份 `.rs`），见**颜色字面量或具名颜色常量**就红，
//! 指出文件与行。认的是写法：
//!
//! - 具名颜色常量：`Color32::WHITE` 这类（`Color32` / `Rgba` / `Hsva` / `HsvaGamma` 后面跟一个全大写的名字）；
//! - 颜色构造：同样那几个类型的 `from_…`（`from_rgb`、`from_gray`、`from_hex`……）与 `new`；
//! - `hex_color!`。
//!
//! **不扫的**：注释、字符串与字符字面量里的字（那是说明，不是颜色）；`#[cfg(test)]` 管着的那一段
//! （测试里的哨兵色、期望值）；令牌装配那一处 `tokens.rs`（颜色从 `#RRGGBB` 读成 `Color32` 就在那儿）。
//!
//! **只放行两样，都不是「一个颜色」**：透明（`Color32::TRANSPARENT`，什么都不画）与占位色
//! （`Color32::PLACEHOLDER`，排版时占着位、画的时候由画笔换成真颜色）。别的一律走令牌，
//! 令牌里没有的，新立一格（`tokens.toml` 头上写着怎么立）。
//!
//! 认的是写法、不是类型：给 `Color32` 起个别名、或者写结构体字面量 `Hsva { h, s, v, a }`，就绕得过去——
//! 那一层靠审查。

use std::path::{Path, PathBuf};

/// 放行的两个具名颜色。只放这两样：再往里加一个，等于在令牌之外又开了一个颜色的来处。
const 放行: [&str; 2] = ["TRANSPARENT", "PLACEHOLDER"];

/// 这几个类型后面跟常量或构造，就是写死了一个颜色。
const 颜色类型: [&str; 4] = ["Color32", "Rgba", "Hsva", "HsvaGamma"];

/// 不扫的那一份：令牌装配。
const 令牌装配: &str = "tokens.rs";

#[test]
fn 界面源码里没有写死的颜色() {
    let 源码目录 = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let 文件 = 界面源码(&源码目录);
    assert!(
        文件.iter().any(|path| path.ends_with("lib.rs")),
        "没读到界面源码：{} 底下一份 lib.rs 都没找到",
        源码目录.display()
    );
    let mut 犯规 = Vec::new();
    for path in &文件 {
        let 原文 = std::fs::read_to_string(path).expect("读得出源码");
        let 相对 = path
            .strip_prefix(&源码目录)
            .expect("在源码目录底下")
            .to_string_lossy()
            .replace('\\', "/");
        for (行, 写法) in 写死的颜色(&原文) {
            let 那一行 = 原文.lines().nth(行 - 1).unwrap_or_default().trim();
            犯规.push(format!("crates/gui/src/{相对}:{行}  {写法}    ← {那一行}"));
        }
    }
    assert!(
        犯规.is_empty(),
        "界面源码里写死了 {} 处颜色。颜色一律取令牌（crates/gui/src/tokens.toml），令牌里没有的新立一格；\
         只放行透明与占位色：\n  {}",
        犯规.len(),
        犯规.join("\n  ")
    );
}

#[test]
fn 具名颜色与颜色构造都抓_只放行透明与占位色() {
    let 源码 = "
let a = Color32::WHITE;
let b = egui::Color32::from_rgb(1, 2, 3);
let c = Color32::from_gray(9);
let d = egui::Rgba::from_rgb(0.1, 0.2, 0.3);
let e = egui::ecolor::Hsva::new(0.1, 0.2, 0.3, 1.0);
let f = egui::hex_color!(\"#123456\");
let g = Color32 :: DARK_GRAY;
let h = Color32::TRANSPARENT;
let i = egui::Color32::PLACEHOLDER;
let j = egui::ColorImage::from_rgba_unmultiplied(size, rgba);
let k = color.gamma_multiply(0.5);
fn l(color: Color32) -> Color32 { color }
";
    let 抓到: Vec<String> = 写死的颜色(源码).into_iter().map(|(_, 写法)| 写法).collect();
    assert_eq!(
        抓到,
        [
            "Color32::WHITE",
            "Color32::from_rgb",
            "Color32::from_gray",
            "Rgba::from_rgb",
            "Hsva::new",
            "hex_color!",
            "Color32::DARK_GRAY",
        ]
    );
}

#[test]
fn 注释字符串与测试模块里的不算_测试模块之后的照抓() {
    let 源码 = r####"
/// 文档注释里提一句 `Color32::WHITE` 不算。
fn 画(painter: &egui::Painter) {
    // 行注释里的 Color32::WHITE 不算
    /* 块注释 /* 套着一层 */ Color32::BLACK 也不算 */
    let _ = "字符串里的 Color32::RED 不算，\" 转义的引号不收场";
    let _ = r#"原始字符串里的 "Color32::RED" 也不算"#;
    let _ = ['{', '"', '\''];
    let _: &'static str = "";
}

#[cfg(test)]
mod tests {
    fn 哨兵() -> Color32 {
        if true { Color32::from_rgb(1, 2, 3) } else { Color32::BLUE }
    }
}

fn 之后(ui: &egui::Ui) -> egui::Color32 {
    egui::Color32::WHITE
}
"####;
    let 那一行 = 源码
        .lines()
        .position(|line| line.contains("egui::Color32::WHITE"))
        .expect("样例里有这一行")
        + 1;
    assert_eq!(写死的颜色(源码), [(那一行, "Color32::WHITE".to_owned())]);
}

#[test]
fn 测试专用的字段与分支只抹它自己_整份测试文件不扫() {
    // `#[cfg(test)]` 挂在字段、match 分支上：只抹那一个字段、那一个分支，不许顺着抹到后面的代码去。
    let 源码 = r#"
struct P {
    b: u8,
    #[cfg(test)]
    a: u8
}
fn f(t: T) -> egui::Color32 {
    match t {
        #[cfg(test)]
        T::A => egui::Color32::BLUE,
        #[cfg(test)]
        T::B => { egui::Color32::GREEN }
        T::C => <egui::Color32>::RED,
        _ => egui::Color32::WHITE,
    }
}
"#;
    let 第几行 = |写法: &str| {
        源码
            .lines()
            .position(|line| line.contains(写法))
            .expect("样例里有这一行")
            + 1
    };
    assert_eq!(
        写死的颜色(源码),
        [
            (第几行("::RED"), "Color32::RED".to_owned()),
            (第几行("::WHITE"), "Color32::WHITE".to_owned()),
        ]
    );
    // 整份文件挂着 `#![cfg(test)]`：那一份全是测试，一个都不算。
    assert_eq!(
        写死的颜色("#![cfg(test)]\nfn f() -> Color32 { Color32::WHITE }\n"),
        []
    );
}

/// `源码目录` 底下每一份 `.rs`（连子目录），除了令牌装配那一份。按路径排好，报出来的次序每趟一样。
fn 界面源码(源码目录: &Path) -> Vec<PathBuf> {
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
    out.retain(|path| path != &源码目录.join(令牌装配));
    out.sort();
    out
}

/// 标识符里的一个字（中文标识符也算）。
fn 是字(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// 一份源码里写死的颜色：`(第几行, 写法)`，行从 1 数。
fn 写死的颜色(源码: &str) -> Vec<(usize, String)> {
    let 代码: Vec<char> = 抹掉测试代码(&抹掉注释与字面量(源码)).chars().collect();
    let 读名字 = |mut at: usize| {
        let start = at;
        while at < 代码.len() && 是字(代码[at]) {
            at += 1;
        }
        (代码[start..at].iter().collect::<String>(), at)
    };
    let 跳空白 = |mut at: usize| {
        while at < 代码.len() && 代码[at].is_whitespace() {
            at += 1;
        }
        at
    };
    let mut out = Vec::new();
    let mut 行 = 1;
    let mut at = 0;
    while at < 代码.len() {
        let c = 代码[at];
        if c == '\n' {
            行 += 1;
        }
        if !是字(c) || (at > 0 && 是字(代码[at - 1])) {
            at += 1;
            continue;
        }
        let (名字, 名字后) = 读名字(at);
        if 名字 == "hex_color" && 代码.get(跳空白(名字后)) == Some(&'!') {
            out.push((行, "hex_color!".to_owned()));
        } else if 颜色类型.contains(&名字.as_str()) {
            // `Color32::…`，也认限定路径 `<Color32>::…`。
            let mut 冒号 = 跳空白(名字后);
            if 代码.get(冒号) == Some(&'>') {
                冒号 = 跳空白(冒号 + 1);
            }
            if 代码.get(冒号..冒号 + 2) == Some(&[':', ':'][..]) {
                let (成员, _) = 读名字(跳空白(冒号 + 2));
                let 是常量 = !成员.is_empty()
                    && 成员
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
                let 是构造 = 成员.starts_with("from_") || 成员 == "new";
                if (是常量 && !放行.contains(&成员.as_str())) || 是构造 {
                    out.push((行, format!("{名字}::{成员}")));
                }
            }
        }
        at = 名字后;
    }
    out
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
/// 抹到哪儿为止：从属性起，**宁可少抹、不许多抹**——多抹一截，后面那段代码里的颜色就悄悄看不见了；
/// 少抹只会让测试代码里的颜色被报出来，当场看得见。
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
