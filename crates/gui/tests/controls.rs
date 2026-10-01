//! **勾选框与单选只用共用的那一枚**（票 `gui-draws-the-rest-of-the-design/01`，挂单 `Q1043`、`Q946`）。
//!
//! 设计稿上的勾选框是 `.ckb`（圆角方框，勾上是强调色填满、里头一圈底色），单选是一枚圆点。两样都由观感那一层
//! （`crates/gui/src/look.rs`）画，尺寸、圆角、颜色取令牌。egui 自带的那两枚画的是 egui 自己的样子——
//! 勾选框是一个打勾的小方块，单选是一个黑点——漏换一处，那一屏上的就与别处长得不一样，而截图门只看得见
//! 恰好拍进基线的那几处。
//!
//! 这里逐个读界面源码（`crates/gui/src` 底下每一份 `.rs`，读法与 `colors.rs` 共用 `shared::source`），
//! 见 **egui 自带勾选框与单选的写法**就红，指出文件与行：
//!
//! - `Ui` 上的方法：`.checkbox(`、`.radio(`、`.radio_value(`；
//! - 控件类型：`Checkbox`、`RadioButton`（`egui::Checkbox::new(…)`、`ui.add(egui::RadioButton::new(…))`）；
//!   读屏念的类型 `WidgetType::Checkbox` / `WidgetType::RadioButton` 不算——共用那一枚也得报它。
//!
//! 不扫注释、字符串与字符字面量、`#[cfg(test)]` 管着的那一段。认的是写法、不是类型：给那两个类型起个别名
//! 就绕得过去——那一层靠审查。

mod shared;

use shared::source;

/// `Ui` 上画 egui 自带勾选框与单选的那几个方法。
const 方法: [&str; 3] = ["checkbox", "radio", "radio_value"];

/// egui 自带的那两个控件类型。
const 类型: [&str; 2] = ["Checkbox", "RadioButton"];

#[test]
fn 界面源码里没有egui自带的勾选框与单选() {
    let 源码目录 = source::源码目录();
    let 文件 = source::界面源码(&源码目录);
    assert!(
        文件.iter().any(|path| path.ends_with("look.rs")),
        "没读到界面源码：{} 底下一份 look.rs 都没找到",
        源码目录.display()
    );
    let mut 犯规 = Vec::new();
    for path in &文件 {
        let 原文 = std::fs::read_to_string(path).expect("读得出源码");
        for (行, 写法) in egui自带的(&原文) {
            let 那一行 = 原文.lines().nth(行 - 1).unwrap_or_default().trim();
            犯规.push(format!(
                "{}:{行}  {写法}    ← {那一行}",
                source::报出来的路径(&源码目录, path)
            ));
        }
    }
    assert!(
        犯规.is_empty(),
        "界面源码里还有 {} 处 egui 自带的勾选框或单选。勾选框用 `look::checkbox`（设计稿 `.ckb`），\
         单选用 `look::radio` / `look::radio_option`，要自己摆内容的用那一枚单独的圆点 `look::radio_dot`：\n  {}",
        犯规.len(),
        犯规.join("\n  ")
    );
}

#[test]
fn 方法调用与控件类型都抓_共用那一枚与注释里的不算() {
    let 源码 = r#"
fn 画(ui: &mut egui::Ui, on: &mut bool, how: &mut u8) {
    ui.checkbox(on, "勾");
    let _ = ui
        .radio(true, "点");
    ui.radio_value(how, 1, "一");
    ui.add(egui::Checkbox::new(on, "勾"));
    ui.add_enabled(false, egui::RadioButton::new(true, "点"));
    // 注释里写 ui.checkbox(on, "") 不算
    let _ = "字符串里的 egui::Checkbox 也不算";
    look::checkbox(ui, on, "共用的那一枚");
    look::radio(ui, true, "共用的那一枚");
    look::radio_dot(ui, true);
    let _ = egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, true, "读屏念的类型不算");
    let _ = WidgetType::RadioButton;
    let checkbox = 1;
    let _ = checkbox;
}

#[cfg(test)]
mod tests {
    fn 测试里的(ui: &mut egui::Ui, on: &mut bool) {
        ui.checkbox(on, "测试代码不扫");
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
        egui自带的(源码),
        [
            (第几行("ui.checkbox(on, \"勾\")"), ".checkbox(".to_owned()),
            (第几行(".radio(true"), ".radio(".to_owned()),
            (第几行("ui.radio_value"), ".radio_value(".to_owned()),
            (第几行("egui::Checkbox::new"), "Checkbox".to_owned()),
            (第几行("egui::RadioButton::new"), "RadioButton".to_owned()),
        ]
    );
}

/// 一份源码里 egui 自带勾选框与单选的写法：`(第几行, 写法)`，行从 1 数。
fn egui自带的(源码: &str) -> Vec<(usize, String)> {
    let 代码: Vec<char> = source::会编进界面的代码(源码).chars().collect();
    let 前一个不是空白的 = |at: usize| {
        代码[..at]
            .iter()
            .rev()
            .find(|c| !c.is_whitespace())
            .copied()
    };
    let 后一个不是空白的 = |at: usize| 代码[at..].iter().find(|c| !c.is_whitespace()).copied();
    let mut out = Vec::new();
    let mut 行 = 1;
    let mut at = 0;
    while at < 代码.len() {
        let c = 代码[at];
        if !source::是字(c) || (at > 0 && source::是字(代码[at - 1])) {
            if c == '\n' {
                行 += 1;
            }
            at += 1;
            continue;
        }
        let 起 = at;
        while at < 代码.len() && source::是字(代码[at]) {
            at += 1;
        }
        let 名字: String = 代码[起..at].iter().collect();
        if 方法.contains(&名字.as_str())
            && 前一个不是空白的(起) == Some('.')
            && 后一个不是空白的(at) == Some('(')
        {
            out.push((行, format!(".{名字}(")));
        } else if 类型.contains(&名字.as_str()) && !是读屏的类型(&代码, 起) {
            out.push((行, 名字));
        }
    }
    out
}

/// `起` 上那个名字是不是 `WidgetType::Checkbox` / `WidgetType::RadioButton`——读屏念的「这是一枚勾选框」，
/// 共用那一枚自己也要报，不是 egui 自带的控件。
fn 是读屏的类型(代码: &[char], 起: usize) -> bool {
    let 前面: String = 代码[..起].iter().collect();
    前面
        .trim_end()
        .strip_suffix("::")
        .is_some_and(|前| 前.trim_end().ends_with("WidgetType"))
}
