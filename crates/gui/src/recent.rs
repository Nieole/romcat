//! **上次开的那份**：这个程序自己记着的一条路径。
//!
//! [**开场**](crate::opening)不该每天挡在你前面。开进一份**现场**的时候把那份
//! **中立库**的完整路径记下来，下一趟启动直接开它——开场只在两种时候出现：记的那份
//! 打不开了（挪走、删了、结构版本对不上），或者人主动要换一份（ADR-0023）。
//!
//! ## 记的是那份中立库的路径，不是**工作目录**
//!
//! 从「工作目录 / `catalog` / 某某文件」那个形状反推工作目录的办法这一层早就有了
//! （[`crate::site`] 里那条推断，`--catalog` 独有的那一段）。**记一条路径两样都拿得到，
//! 也不会两样对不上**——两样各记一份，迟早出现「记着的工作目录里压根没有记着的那份库」。
//!
//! ## ⚠️ 它不住在任何工作目录里
//!
//! 界面的偏好一律住在工作目录里（版式偏好就是
//! [`romcat_core::workspace::gui_layout_path`]：
//! `工作目录/gui/layout.txt`）。这一份不能——**「工作目录是哪个」没法住在工作目录里**。
//! 于是它是**程序级**的：位置固定取默认工作目录那条链的结果
//! （[`workspace::default_dir`]），谁也换不动。
//!
//! **同一个目录下于是住着两种东西**：那条链在 Windows 上折出 `%APPDATA%\romcat\`，
//! 而那**恰好也是默认工作目录**——`catalog/`、`verdict/`、`gui/` 与这份文件并排躺着。
//! 这不会出错（换工作目录只换前几样，这一份原地不动，正是要的行为），但读代码的人
//! 到这儿会绊一下：**这份文件不属于它旁边那个工作目录，它只是碰巧落在同一处。**
//!
//! ## 一份文本文件，人改得动也删得掉
//!
//! 格式照[版式偏好](crate::layout)那一份办：几行注释交代这是什么、一行 `键 = 值`。
//! 读不懂就当没记过——**这一份丢了的全部后果是下次启动看见开场**，为它报一句错、
//! 拦住一个正要开工的人，不成比例。

use std::path::{Path, PathBuf};

use romcat_core::path;
use romcat_core::workspace;

/// 那份文件叫什么。
///
/// **名字里带着 `catalog`**：它与默认工作目录落在同一个目录下（见模块文档），
/// 旁边就是 `catalog/`、`gui/` 那几样。一个叫 `last.txt` 的文件躺在那儿，
/// 下一个人得打开才知道它是什么。
const FILE: &str = "last-catalog.txt";

/// 记着的那一条前面写的键。
const KEY: &str = "catalog";

/// 文件开头那几句话。**它是给人看的**：一个不认得的文件躺在那儿，第一个问题永远是
/// 「删了会怎样」。
const HEADER: &str = "\
# romcat 记着上次开的是哪一份中立库：下一趟启动直接开它，不必再挑一遍。
# 这一份**随时可以删**：删了就回到开场挑一份，库里一个字节都不动。
# 它**不属于任何工作目录**——所有界面偏好都住在工作目录里，而「工作目录是哪个」
# 没法住在工作目录里，所以它是程序级的，位置固定（ADR-0023）。
";

/// 上次开的那份中立库记在哪儿。
///
/// 拿住的是**那份文件的路径**而不是内容：这一趟里它要被读一次（启动时）、写零到几次
/// （每换一份库写一次），中间隔着人的一整段操作，把内容缓存在这儿只会让两边对不上。
#[derive(Debug, Clone)]
pub struct Recent {
    /// 那份文件在哪。
    path: PathBuf,
}

impl Recent {
    /// 固定的那一处：默认工作目录那条链的结果底下
    /// （[`workspace::default_dir`] 底下的 `last-catalog.txt`）。
    ///
    /// **它不跟着 `--workspace` 走**——那正是这份记忆存在的理由（见模块文档）。
    #[must_use]
    pub fn here() -> Self {
        Self::at(workspace::default_dir().join(FILE))
    }

    /// 记在别的地方。
    ///
    /// **测试要它**：[`Self::here`] 折出来的那一处是维护者机器上真的那一份，
    /// 一条测试都不许读它、更不许写它。
    #[must_use]
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    /// 那份文件在哪。测试拿它核对「不在工作目录里」。
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 上次开的是哪一份。**没记过、读不动、读不懂，一律是「没有」。**
    ///
    /// 三种情形在这儿是同一件事：那时就走开场，而开场本来就是这个程序的正常入口。
    #[must_use]
    pub fn read(&self) -> Option<PathBuf> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        parse(&text)
    }

    /// 把这一份记下来。**已经记着它了就一个字节都不写。**
    ///
    /// **写不下去时什么都不说。** 这不是把错吞了——它是这个程序里少有的、**说了也没用**
    /// 的一处：这一句发生在库刚开好、主窗口正要画出来的那一刻，而它失败的全部后果是
    /// **下一趟启动多看见一次开场**。为它在人正要开工的那一帧上摆一句红字，与那个后果
    /// 完全不成比例；而界面上眼下也没有一处画得下它（挂单 `Q396`）。
    pub fn remember(&self, catalog: &Path) {
        if self.read().as_deref() == Some(catalog) {
            return;
        }
        // **连目录一起建**：默认工作目录那条链折出来的目录第一次跑的时候还不在。
        if let Some(parent) = self.path.parent()
            && std::fs::create_dir_all(parent).is_err()
        {
            return;
        }
        let _ = std::fs::write(&self.path, render(catalog));
    }
}

/// 摊成要写进文件的那几行。
fn render(catalog: &Path) -> String {
    format!("{HEADER}{KEY} = {}\n", path::display(catalog))
}

/// 读那几行，挑出记着的那一条。
///
/// **读不懂的行、不认得的键一律跳过**，照[版式偏好](crate::layout)那一份的办法：
/// 这份文件全部的内容就是一条路径，为一行坏掉的东西把整个程序拦下不成比例。
fn parse(text: &str) -> Option<PathBuf> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // **只切第一个 `=`**：路径里带等号是合法的，切在后头会把它拦腰截断。
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != KEY {
            continue;
        }
        let value = value.trim();
        if !value.is_empty() {
            return Some(PathBuf::from(value));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use romcat_core::testing::temp_dir;

    #[test]
    fn 记下来的那一条读回来还是同一条() {
        let dir = temp_dir("gui-recent-来回一趟");
        let recent = Recent::at(dir.path().join(FILE));
        assert_eq!(recent.read(), None, "还没记过的时候不该凭空冒出一条");

        let 库文件 = dir
            .path()
            .join("工作目录")
            .join("catalog")
            .join("某某.sqlite3");
        recent.remember(&库文件);

        assert_eq!(recent.read(), Some(库文件), "记下的与读回来的不是同一条");
    }

    #[test]
    fn 换一份就记新的那一份() {
        // 验收第 6 条：换过库之后记住的是新那份，而不是攒出两条互相打架的记录。
        let dir = temp_dir("gui-recent-换一份");
        let recent = Recent::at(dir.path().join(FILE));
        recent.remember(Path::new("/工作目录/catalog/甲.sqlite3"));
        recent.remember(Path::new("/工作目录/catalog/乙.sqlite3"));

        assert_eq!(
            recent.read(),
            Some(PathBuf::from("/工作目录/catalog/乙.sqlite3")),
        );
    }

    #[test]
    fn 读不懂的一份当没记过而不是拦住整个程序() {
        // 人手改坏了、别的程序写进来一段别的东西：那时走开场——而开场本来就是
        // 这个程序的正常入口，为它报一句错拦住一个正要开工的人不成比例。
        let dir = temp_dir("gui-recent-读不懂");
        let recent = Recent::at(dir.path().join(FILE));
        std::fs::write(recent.path(), "这不是一份配置\n乱七八糟 = \n").expect("写得进去");

        assert_eq!(recent.read(), None);
    }

    #[test]
    fn 那份文件是给人看的说得出删了会怎样() {
        // 一个不认得的文件躺在维护者的默认工作目录里，第一个问题永远是「删了会怎样」。
        let 写出来的 = render(Path::new("/工作目录/catalog/某某.sqlite3"));
        assert!(写出来的.starts_with('#'), "开头没有给人看的那几句话");
        assert!(写出来的.contains("随时可以删"), "没说删了会怎样");
        assert!(
            写出来的.contains("/工作目录/catalog/某某.sqlite3"),
            "记的不是那条完整路径",
        );
    }

    #[test]
    fn 固定那一处落在默认工作目录那条链的结果底下() {
        // 它**不跟着 `--workspace` 走**：换工作目录不会把这份记忆弄丢（验收第 3 条）。
        let 在哪儿 = Recent::here();
        assert_eq!(
            在哪儿.path().parent(),
            Some(workspace::default_dir().as_path())
        );
        assert_eq!(
            在哪儿
                .path()
                .file_name()
                .map(|it| it.to_string_lossy().into_owned()),
            Some(FILE.to_string())
        );
    }
}
