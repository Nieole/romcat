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

mod shared;

// 列源码、抹注释与字面量、抹测试代码那几样与 `controls.rs` 共用（`shared::source`）。
use shared::source;

/// 放行的两个具名颜色。只放这两样：再往里加一个，等于在令牌之外又开了一个颜色的来处。
const 放行: [&str; 2] = ["TRANSPARENT", "PLACEHOLDER"];

/// 这几个类型后面跟常量或构造，就是写死了一个颜色。
const 颜色类型: [&str; 4] = ["Color32", "Rgba", "Hsva", "HsvaGamma"];

/// 不扫的那一份：令牌装配。
const 令牌装配: &str = "tokens.rs";

#[test]
fn 界面源码里没有写死的颜色() {
    let 源码目录 = source::源码目录();
    let 文件 = 界面源码(&源码目录);
    assert!(
        文件.iter().any(|path| path.ends_with("lib.rs")),
        "没读到界面源码：{} 底下一份 lib.rs 都没找到",
        源码目录.display()
    );
    let mut 犯规 = Vec::new();
    for path in &文件 {
        let 原文 = std::fs::read_to_string(path).expect("读得出源码");
        let 报 = source::报出来的路径(&源码目录, path);
        for (行, 写法) in 写死的颜色(&原文) {
            let 那一行 = 原文.lines().nth(行 - 1).unwrap_or_default().trim();
            犯规.push(format!("{报}:{行}  {写法}    ← {那一行}"));
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

/// 要扫的那几份：界面源码里每一份 `.rs`，除了令牌装配那一份。
fn 界面源码(源码目录: &Path) -> Vec<PathBuf> {
    let mut out = source::界面源码(源码目录);
    out.retain(|path| path != &源码目录.join(令牌装配));
    out
}

/// 一份源码里写死的颜色：`(第几行, 写法)`，行从 1 数。
fn 写死的颜色(源码: &str) -> Vec<(usize, String)> {
    let 代码: Vec<char> = source::会编进界面的代码(源码).chars().collect();
    let 读名字 = |mut at: usize| {
        let start = at;
        while at < 代码.len() && source::是字(代码[at]) {
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
        if !source::是字(c) || (at > 0 && source::是字(代码[at - 1])) {
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
