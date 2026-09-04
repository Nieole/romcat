//! **Bangumi Archive 的离线 dump**：从 `subject.jsonlines` 里读出游戏条目。
//!
//! dump 是官方每周三导出的一个 zip（调研 §9.1 实测 435 MB，内含九个 `.jsonlines`）。
//! 这一层只读其中的 `subject.jsonlines`，**只留游戏**，把每条折成一份
//! [`Entry`](super::Entry)。
//!
//! ## 一条记录长什么样
//!
//! ```json
//! {"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7",
//!  "infobox":"{{Infobox Game\r\n|中文名= 合金弹头7\r\n|别名={\r\n[Metal Slug 7]\r\n}\r\n|平台= NDS\r\n…}}",
//!  "platform":4001,"date":"2008-07-17","meta_tags":["ACT","NDS","游戏"], …}
//! ```
//!
//! **两个字段都叫「平台」，说的却不是一回事**，混起来会把整份 dump 读空：
//!
//! - 顶层的 `platform` 是**条目分类**的数字码（`4001` 游戏 / `4002` 软件 /
//!   `4003` DLC / `4004` Demo / `4005` 桌游，见 `bangumi/common`）。
//! - `infobox` 里的 `|平台=` 才是**跑在哪台机器上**——交叉校验要的是它。
//!
//! ## `infobox` 是原始 wiki 字符串
//!
//! 官方有正式的语法规范与解析器（`bangumi/wiki-syntax-spec`），但这里只取六个键
//! （平台、别名、发行日期、游戏类型、开发、发行），用不着一整套解析器。要的形状只有
//! 两种：`|键= 值` 与
//!
//! ```text
//! |别名={
//! [Metal Slug 7]
//! [合金彈頭7]
//! }
//! ```
//!
//! 认不出的行整行跳过——**认不出就留空**，这一层宁可少说（同 `identify::naming`）。
//! **缺键也是留空不是错误**：一条 infobox 没写 `|开发=`，它写了的那几样照样读得出来。
//!
//! ## 一个键装着好几个值时拆成好几条
//!
//! 多值块（`|开发={ [A] [B] }`）与**顿号分隔**（`|开发= A、B`）说的是同一件事，
//! 而后者在真库里更常见。[`values`] 把两种写法都拆成好几条——不拆的话「开发商」
//! 那一格就是一串带顿号的长字符串，前端里既筛不动也搜不着。
//!
//! **只有拆得动的键才走 [`values`]**：平台与别名仍旧走 [`field`]，一个名字里本来就
//! 可能带顿号，拆了就是把一个名字劈成两半。

use serde::Deserialize;

/// dump 里那条记录，只留这一层用得上的字段。
#[derive(Debug, Deserialize)]
pub struct Row {
    /// 条目 id。
    pub id: u32,
    /// 条目类型：`4` 是游戏。
    #[serde(rename = "type")]
    pub kind: u32,
    /// 原名。
    #[serde(default)]
    pub name: String,
    /// 中文名。**这是这份数据源的全部价值所在**（调研 §9.1：`name_cn` 是一级字段）。
    #[serde(default)]
    pub name_cn: String,
    /// 条目**分类**的数字码，不是「跑在哪台机器上」。
    #[serde(default)]
    pub platform: u32,
    /// 原始 wiki 字符串。
    #[serde(default)]
    pub infobox: String,
    /// **中文简介**，条目的一级字段。实测 94.2% 的游戏条目有它（中位 338 字）。
    ///
    /// **原样留着，一个字都不改**：换行、全角空格与数据源自带的排版都是内容的一部分，
    /// 压掉它们导出到前端里就是一坨。
    #[serde(default)]
    pub summary: String,
    /// 发行日期，`2008-07-17` 这样。
    #[serde(default)]
    pub date: Option<String>,
    /// 条目上的标签，平台名常常也在里面。
    #[serde(default)]
    pub meta_tags: Vec<String>,
}

/// 条目类型里的「游戏」。
pub const TYPE_GAME: u32 = 4;

/// 条目分类里的「游戏」。`4002` 软件、`4003` DLC、`4004` Demo、`4005` 桌游都不是。
///
/// **DLC 与 Demo 明确排掉**：它们与本体同名，收进来只会让每个查询多两条一模一样的
/// 候选，而**附属内容不导出为前端条目**（ADR-0013）。
pub const CATEGORY_GAME: u32 = 4001;

impl Row {
    /// 这条是不是一个游戏条目。
    #[must_use]
    pub fn is_game(&self) -> bool {
        self.kind == TYPE_GAME && self.platform == CATEGORY_GAME
    }

    /// `infobox` 里 `|平台=` 那一项，原样。
    #[must_use]
    pub fn platforms(&self) -> Vec<String> {
        let mut out = field(&self.infobox, "平台");
        if out.is_empty() {
            // 退而求其次：标签里常常也写着平台名。**只在 infobox 没写时才用**——
            // 标签是用户随手打的，`ACT`、`游戏` 这类词也在里面，交叉校验拿它当主证据
            // 会把「平台对不上」判成「平台对得上」。
            out = self.meta_tags.clone();
        }
        out
    }

    /// 全部别名：`infobox` 里 `|别名=` 那一组。
    ///
    /// **不拆顿号**：一个名字里本来就可能带顿号，拆了就是把一个名字劈成两半。
    #[must_use]
    pub fn aliases(&self) -> Vec<String> {
        field(&self.infobox, "别名")
    }

    /// 类型：`infobox` 里 `|游戏类型=`。实测 99.7% 的游戏条目写了它。
    #[must_use]
    pub fn genres(&self) -> Vec<String> {
        values(&self.infobox, "游戏类型")
    }

    /// 开发商：`infobox` 里 `|开发=`。实测 83.8% 写了它。
    #[must_use]
    pub fn developers(&self) -> Vec<String> {
        values(&self.infobox, "开发")
    }

    /// 发行商：`infobox` 里 `|发行=`。实测 78.5% 写了它。
    ///
    /// **与 `|发行日期=` 不是一个键**：[`field`] 比的是整个键名，`发行` 匹配不上
    /// `发行日期`，两者各读各的。
    #[must_use]
    pub fn publishers(&self) -> Vec<String> {
        values(&self.infobox, "发行")
    }

    /// 发行年份。顶层 `date` 优先，没有就从 `infobox` 的发行日期里读。
    #[must_use]
    pub fn year(&self) -> Option<u16> {
        let from_date = self.date.as_deref().and_then(year_in);
        if from_date.is_some() {
            return from_date;
        }
        for key in ["发行日期", "发售日", "发布日期", "发行时间"] {
            if let Some(year) = field(&self.infobox, key).iter().find_map(|it| year_in(it)) {
                return Some(year);
            }
        }
        None
    }
}

/// 一段文字里第一个像年份的四位数字。
fn year_in(text: &str) -> Option<u16> {
    let bytes: Vec<char> = text.chars().collect();
    for start in 0..bytes.len() {
        if bytes[start..].len() < 4 {
            break;
        }
        let window: String = bytes[start..start + 4].iter().collect();
        if !window.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        // 前后都不能再挨着数字，不然 `20081` 里也能切出一段来。
        let before = start == 0 || !bytes[start - 1].is_ascii_digit();
        let after = bytes.get(start + 4).is_none_or(|c| !c.is_ascii_digit());
        if !before || !after {
            continue;
        }
        if let Ok(year) = window.parse::<u16>()
            && (1950..=2100).contains(&year)
        {
            return Some(year);
        }
    }
    None
}

/// `infobox` 里某个键的值，可能有好几项。
///
/// 两种写法都认：`|平台= NDS`，以及
///
/// ```text
/// |别名={
/// [Metal Slug 7]
/// [合金彈頭7]
/// }
/// ```
#[must_use]
pub fn field(infobox: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = infobox.lines();
    while let Some(line) = lines.next() {
        let Some(rest) = line.trim().strip_prefix('|') else {
            continue;
        };
        let Some((name, value)) = rest.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        if value != "{" {
            if !value.is_empty() {
                out.push(value.to_string());
            }
            continue;
        }
        // 一组的写法：往下读到 `}`。
        for line in lines.by_ref() {
            let line = line.trim();
            if line == "}" || line.starts_with("}}") {
                break;
            }
            if let Some(item) = list_item(line) {
                out.push(item);
            }
        }
    }
    out
}

/// `infobox` 里某个键的值，**顿号也拆开**，去掉重复的那些。
///
/// 与 [`field`] 的分工：那一个交出原样的几项，这一个再把每一项按顿号劈开。
/// 拆得动的键（类型、开发、发行）走这里，名字那几个键（平台、别名）走 [`field`]。
#[must_use]
pub fn values(infobox: &str, key: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in field(infobox, key) {
        for part in item.split('、') {
            let part = part.trim();
            if part.is_empty() || out.iter().any(|seen| seen == part) {
                continue;
            }
            out.push(part.to_string());
        }
    }
    out
}

/// 一组里的一行：`[值]` 或者 `[键|值]`。
fn list_item(line: &str) -> Option<String> {
    let body = line.strip_prefix('[')?.strip_suffix(']')?;
    // `[发行商|世嘉]` 这种带键的写法，要的是竖线后面那一半。
    let value = body.rsplit_once('|').map_or(body, |(_, value)| value);
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 调研 §9.1 实测的那条记录（id=4）。
    ///
    /// `开发` 与 `发行` 两个键是**照真库的写法补上的**——调研当时抄的那一版只留了
    /// 平台、别名与发行日期三样，而这一票要取的正是它没抄的那几样。两种写法各摆一个：
    /// `开发` 是顿号分隔的单值，`发行` 是多值块。
    const 合金弹头七: &str = r#"{"id":4,"type":4,"name":"メタルスラッグ7","name_cn":"合金弹头7","infobox":"{{Infobox Game\r\n|中文名= 合金弹头7\r\n|别名={\r\n[Metal Slug 7]\r\n[合金彈頭7]\r\n}\r\n|平台= NDS\r\n|游戏类型= ACT\r\n|开发= SNK、北斗\r\n|发行={\r\n[世嘉]\r\n}\r\n|发行日期= 2008-07-17\r\n}}","platform":4001,"summary":"　　以细腻的画风…","date":"2008-07-17","meta_tags":["ACT","NDS","游戏"]}"#;

    fn 一条() -> Row {
        serde_json::from_str(合金弹头七).expect("读得进来")
    }

    #[test]
    fn 一条真实记录读得出中文名平台与年份() {
        let row = 一条();
        assert!(row.is_game());
        assert_eq!(row.name_cn, "合金弹头7");
        assert_eq!(row.name, "メタルスラッグ7");
        assert_eq!(row.platforms(), vec!["NDS"]);
        assert_eq!(row.aliases(), vec!["Metal Slug 7", "合金彈頭7"]);
        assert_eq!(row.year(), Some(2008));
    }

    #[test]
    fn 只留游戏不留软件与_dlc() {
        // **附属内容不导出为前端条目**（ADR-0013），而 DLC 与本体同名——收进来只会让
        // 每个查询多两条一模一样的候选。
        for category in [4002, 4003, 4004, 4005] {
            let text = 合金弹头七.replace("\"platform\":4001", &format!("\"platform\":{category}"));
            let row: Row = serde_json::from_str(&text).expect("读得进来");
            assert!(!row.is_game(), "{category}");
        }
        // 动画（`type` 是 2）同样不是游戏。
        let text = 合金弹头七.replace("\"type\":4", "\"type\":2");
        let row: Row = serde_json::from_str(&text).expect("读得进来");
        assert!(!row.is_game());
    }

    #[test]
    fn 平台写成一组时也读得出来() {
        let infobox = "{{Infobox Game\r\n|平台={\r\n[PS4]\r\n[PS5]\r\n[Nintendo Switch]\r\n}\r\n}}";
        assert_eq!(
            field(infobox, "平台"),
            vec!["PS4", "PS5", "Nintendo Switch"]
        );
    }

    #[test]
    fn 带键的那种列表项取竖线后面那一半() {
        let infobox = "{{Infobox Game\r\n|别名={\r\n[英文名|Metal Slug 7]\r\n}\r\n}}";
        assert_eq!(field(infobox, "别名"), vec!["Metal Slug 7"]);
    }

    #[test]
    fn infobox_单值多值块与顿号三种写法都拆得开() {
        // 单值。
        assert_eq!(values("{{Infobox Game\n|开发= 任天堂\n}}", "开发"), vec!["任天堂"]);
        // 多值块。
        assert_eq!(
            values("{{Infobox Game\n|开发={\n[任天堂]\n[HAL研究所]\n}\n}}", "开发"),
            vec!["任天堂", "HAL研究所"]
        );
        // 顿号分隔——真库里比多值块常见。
        assert_eq!(
            values("{{Infobox Game\n|开发= 任天堂、HAL研究所\n}}", "开发"),
            vec!["任天堂", "HAL研究所"]
        );
        // 两种写法混着来，重复的只留一条。
        assert_eq!(
            values(
                "{{Infobox Game\n|开发={\n[任天堂、HAL研究所]\n[任天堂]\n}\n}}",
                "开发"
            ),
            vec!["任天堂", "HAL研究所"]
        );
    }

    #[test]
    fn infobox_缺键空值与键名带空格() {
        let infobox = "{{Infobox Game\n| 游戏类型 = ACT\n|开发=\n|发行=  \n}}";
        // **键名两边带空格照样认得出**：这份数据是人手写的 wiki，空格全凭手感。
        assert_eq!(values(infobox, "游戏类型"), vec!["ACT"]);
        // 空值就是没有值——不产出一条空串，那会让优先级链在它身上停下来。
        assert!(values(infobox, "开发").is_empty());
        assert!(values(infobox, "发行").is_empty());
        // **缺键是留空不是错误**：它写了的那几样照样读得出来。
        assert!(values(infobox, "别名").is_empty());
        assert!(field(infobox, "别名").is_empty());
    }

    #[test]
    fn 发行与发行日期是两个键() {
        // `field` 比的是整个键名，`发行` 匹配不上 `发行日期`——两者各读各的。
        let infobox = "{{Infobox Game\n|发行= 世嘉\n|发行日期= 2008-07-17\n}}";
        assert_eq!(values(infobox, "发行"), vec!["世嘉"]);
        let row = Row {
            id: 1,
            kind: 4,
            name: String::new(),
            name_cn: String::new(),
            platform: 4001,
            infobox: infobox.to_string(),
            summary: String::new(),
            date: None,
            meta_tags: Vec::new(),
        };
        assert_eq!(row.publishers(), vec!["世嘉"]);
        assert_eq!(row.year(), Some(2008));
    }

    #[test]
    fn 一条真实记录读得出简介类型开发商与发行商() {
        // 这四样以前一条都没被取出来过——那份 435 MB 的数据被当成「撞名字的索引」在用。
        let row = 一条();
        assert_eq!(row.summary, "　　以细腻的画风…", "简介原样留着");
        assert_eq!(row.genres(), vec!["ACT"]);
        assert_eq!(row.developers(), vec!["SNK", "北斗"]);
        assert_eq!(row.publishers(), vec!["世嘉"]);
    }

    #[test]
    fn 平台读不出来时才退回标签() {
        // 标签是用户随手打的，`ACT` 与 `游戏` 也在里面——拿它当主证据会把
        // 「平台对不上」判成「对得上」。
        let text = 合金弹头七.replace("|平台= NDS\\r\\n", "");
        let row: Row = serde_json::from_str(&text).expect("读得进来");
        assert_eq!(row.platforms(), vec!["ACT", "NDS", "游戏"]);
    }

    #[test]
    fn 年份认不出来就留空() {
        let row = Row {
            id: 1,
            kind: 4,
            name: "Foo".to_string(),
            name_cn: String::new(),
            platform: 4001,
            infobox: "{{Infobox Game\r\n|发行日期= 未知\r\n}}".to_string(),
            summary: String::new(),
            date: Some(String::new()),
            meta_tags: Vec::new(),
        };
        assert_eq!(row.year(), None);
        assert_eq!(year_in("20081"), None, "前后不能再挨着数字");
        assert_eq!(year_in("1985-12-11"), Some(1985));
    }
}
