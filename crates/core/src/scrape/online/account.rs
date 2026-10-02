//! **ScreenScraper 的账号**：存在工作目录里、只本人读得了，而且只一套（票 `verdict-store-and-sync/15`，
//! 收挂单 `Q1063`）。
//!
//! 从前凭据只认开工具之前给的那四个环境变量（[`ENV_KEYS`]）。命令行上那不成问题，可从 Finder 启动的
//! 界面**读不到**终端里设的环境变量——联网刮削在界面上实际上用不了。已裁的形状（`grill.md` 的 `Q1063`）：
//!
//! | 那件事 | 落在哪 |
//! |---|---|
//! | 存在哪 | 工作目录里一份文件（[`workspace::screenscraper_account_path`]），**不跟着库走** |
//! | 谁读得了 | **只本机本人**：Unix 上权限 0600，写的那一刻就是，不是写完再收 |
//! | 几套 | **只一套**：文件的形状只装得下一套，再存一次就换掉上一套 |
//! | 读的顺序 | **环境变量优先，其次那份文件**；两边**不拼**成一套 |
//! | 谁来读写 | **这一处**：界面的设置屏与刮削面板、命令行 `romcat scrape` 都走它（ADR-0024） |
//!
//! ## Windows 上做不到「只本人读得了」这一半
//!
//! 标准库在 Windows 上没有设访问控制表的办法，而这个工作区禁 `unsafe`。所以那边**照常写一份文件，
//! 不另收权限**：默认工作目录在 `%APPDATA%` 底下，那里默认只有本人（与本机管理员）读得了；工作目录被换到
//! 别处时，这一层保护就跟着没了——那份文件读不读得了，看那个目录自己怎么设。
//!
//! ## 「测试连接」不在这儿
//!
//! 测一次连接扣不扣配额另议（规格的 Out of Scope），这一处只管存与读。

use std::fmt;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{Credentials, ENV_KEYS};
use crate::workspace;

/// 那份文件**谁读得了**：屏上、命令行上与文件开头说的那半句，各平台照实（模块文档「Windows 上做不到」一节）。
pub const WHO_CAN_READ: &str = if cfg!(unix) {
    "只有你本人读得了（权限 0600）"
} else {
    "不另设权限，读不读得了看工作目录那个文件夹（默认的那个在你的用户目录底下，别的用户读不了）"
};

/// devid 从哪儿来、为什么不能借：缺账号、缺开发者那两样时，核心库、命令行与界面拒下的那句话里都带着它，
/// **只写这一处**。
pub const DEVID_NOTE: &str = "devid 与 devpassword 要在 ScreenScraper 的论坛人工申请（无 devid 直接 403），\
     不要拿别人的 devid 用——那会连累对方被拉黑（426）。";

/// 文件开头那几句话。**它是给人看的**：一份不认得的文件躺在工作目录里，第一个问题是「删了会怎样」，
/// 第二个是「这里头的密码谁看得见」。
fn header() -> String {
    format!(
        "# romcat 的 ScreenScraper 账号：联网刮削用它。界面与命令行 romcat scrape 读的都是这一份。\n\
         # 这份文件{WHO_CAN_READ}。开工具之前给了 SCREENSCRAPER_ 打头的那四个环境变量时，环境变量优先。\n\
         # 只存一套：多账号轮换绕配额的处置是永久封禁。删了它就是不留账号，库里一个字节都不动。\n"
    )
}

/// 人给的那**一套**账号：四样，与 ScreenScraper 那四个参数一一对应（文件里的键用的就是那四个名字）。
///
/// `devid` / `devpassword` 是**开发者**凭据，要在它的论坛人工申请（无 devid 直接 403），**两样缺一样
/// 不算一套**；`ssid` / `sspassword` 是**终端用户**的站点账号，可以空着。
///
/// **`Debug` 不印两样密码**：账号会跟着错误与日志走，密码不许跟着出去。
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Account {
    /// 开发者标识（`devid`）。
    #[serde(rename = "devid")]
    pub dev_id: String,
    /// 开发者密码（`devpassword`）。
    #[serde(rename = "devpassword")]
    pub dev_password: String,
    /// 终端用户的用户名（`ssid`）。空着就是不给。
    #[serde(rename = "ssid", default)]
    pub user: String,
    /// 终端用户的密码（`sspassword`）。空着就是不给。
    #[serde(rename = "sspassword", default)]
    pub user_password: String,
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let masked = |text: &str| if text.is_empty() { "" } else { "••••" };
        f.debug_struct("Account")
            .field("dev_id", &self.dev_id)
            .field("dev_password", &masked(&self.dev_password))
            .field("user", &self.user)
            .field("user_password", &masked(&self.user_password))
            .finish()
    }
}

impl Account {
    /// 开发者那两样都给了没有。**缺一样就不算一套**：没有 devid 的请求直接 403，而那是白扣一次的。
    #[must_use]
    pub fn is_complete(&self) -> bool {
        !self.dev_id.is_empty() && !self.dev_password.is_empty()
    }

    /// 四样都空着：**不留账号**。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.dev_id.is_empty()
            && self.dev_password.is_empty()
            && self.user.is_empty()
            && self.user_password.is_empty()
    }

    /// 拼成一趟联网刮削用的**凭据**：调用方软件名在这儿补上（它跟着版本走，不进文件），空着的那两样不发。
    #[must_use]
    pub fn credentials(&self) -> Credentials {
        let given = |text: &str| (!text.is_empty()).then(|| text.to_owned());
        Credentials {
            dev_id: self.dev_id.clone(),
            dev_password: self.dev_password.clone(),
            soft_name: concat!("romcat", env!("CARGO_PKG_VERSION")).to_string(),
            user: given(&self.user),
            user_password: given(&self.user_password),
        }
    }

    /// 从环境变量读那一套：开发者那两样缺一样就当没给。
    fn from_env(env: &dyn Fn(&str) -> Option<String>) -> Option<Self> {
        let get = |key: &str| env(key).filter(|value| !value.is_empty());
        Some(Self {
            dev_id: get(ENV_KEYS[0])?,
            dev_password: get(ENV_KEYS[1])?,
            user: get(ENV_KEYS[2]).unwrap_or_default(),
            user_password: get(ENV_KEYS[3]).unwrap_or_default(),
        })
    }
}

/// 那一套账号是从哪儿读到的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountFrom {
    /// 开工具之前给的那四个环境变量（[`ENV_KEYS`]）。**它优先。**
    Env,
    /// 工作目录里那份文件（[`workspace::screenscraper_account_path`]）。
    File,
}

impl AccountFrom {
    /// 屏上与命令行上说「用的是哪一套」时写的那几个字。
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Env => "开工具之前给的环境变量",
            Self::File => "工作目录里存着的那一套",
        }
    }
}

/// 账号读不动、存不进。
#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    /// 文件在，可读不出来。
    #[error("账号文件 {path} 读不出来：{source}")]
    Read {
        /// 那份文件。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
    /// 文件读得出来，可读不懂：不是那四个键、摆了第二套、值没加引号，或者不是 TOML。
    ///
    /// **不带底层那份 TOML 报错**：它照例把出错那一行原文引出来，而人手写漏了引号的那一行往往就是
    /// 密码本身——这句话要上设置屏的红字与命令行的 stderr。只留第几行。
    #[error(
        "账号文件 {path}{} 读不懂：只认 devid、devpassword、ssid、sspassword 这四个键（值要加引号），\
         而且只一套。为了不把密码印出来，这里不引那一行的原文。",
        .line.map_or_else(String::new, |line| format!(" 第 {line} 行"))
    )]
    Parse {
        /// 那份文件。
        path: String,
        /// 出错在第几行（从 1 数）；TOML 没说是哪儿就是 `None`。
        line: Option<usize>,
    },
    /// 开发者那两样缺一样（存的时候，或者文件被人改成了这样）。
    #[error("开发者标识与开发者密码缺一样。{DEVID_NOTE}")]
    Incomplete,
    /// 写不进去（建目录、写临时文件、收权限或改名失败）。
    #[error("账号文件 {path} 写不进去：{source}")]
    Write {
        /// 出问题的那一份。
        path: String,
        /// 底层错误。
        source: std::io::Error,
    },
}

/// **有没有账号、用的是哪一套**：环境变量优先，其次工作目录里那份文件。
///
/// 读的是这个进程真的环境变量；测试与截图那一路要换一份环境变量进来，走 [`find_account_with`]。
///
/// # Errors
/// 环境变量里没有一整套、而那份文件在却读不动或读不懂时返回 [`AccountError`]。
pub fn find_account(workspace: &Path) -> Result<Option<(Account, AccountFrom)>, AccountError> {
    find_account_with(workspace, &|key| std::env::var(key).ok())
}

/// 同 [`find_account`]，只是环境变量从 `env` 问。
///
/// **两边不拼**：环境变量里有一整套就用它，文件一个字都不读；没有一整套（一样都没给，或者开发者那两样
/// 缺一样）就整套用文件里的——从两处各取几样拼成一套，等于没人见过的第三套账号。
///
/// # Errors
/// 同 [`find_account`]。
pub fn find_account_with(
    workspace: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<(Account, AccountFrom)>, AccountError> {
    if let Some(account) = Account::from_env(env) {
        return Ok(Some((account, AccountFrom::Env)));
    }
    Ok(saved_account(workspace)?.map(|account| (account, AccountFrom::File)))
}

/// **有没有账号**——环境变量或工作目录里那份文件，哪一条都算。
///
/// 立给刮削弹层「没有账号」那一行（票 `gui-draws-the-rest-of-the-design/17` 的 `F-7`）：那一行只问有没有，
/// 不问用的是哪一套。文件在却读不动、读不懂的，算**没有**：拿它去联网只会在按下去那一刻被拒，
/// 那句为什么由 [`find_account`] 说。
#[must_use]
pub fn has_account(workspace: &Path) -> bool {
    matches!(find_account(workspace), Ok(Some(_)))
}

/// 工作目录里**存着的那一套**，不看环境变量。设置屏把它摆回那几格里。
///
/// 文件不在是 `Ok(None)`。
///
/// # Errors
/// 文件在却读不出来、读不懂，或者开发者那两样缺一样时返回 [`AccountError`]。
pub fn saved_account(workspace: &Path) -> Result<Option<Account>, AccountError> {
    let path = workspace::screenscraper_account_path(workspace);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(AccountError::Read {
                path: crate::path::display(&path),
                source,
            });
        }
    };
    let account: Account = toml::from_str(&text).map_err(|error| AccountError::Parse {
        path: crate::path::display(&path),
        line: error
            .span()
            .and_then(|span| text.get(..span.start))
            .map(|before| before.matches('\n').count() + 1),
    })?;
    if !account.is_complete() {
        return Err(AccountError::Incomplete);
    }
    Ok(Some(account))
}

/// 按一下「保存」落成了哪一样。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// 存下了这一套，换掉了原来那一套。
    Stored,
    /// 四样都空着：**不留账号**，工作目录里那份删掉了（本来就没有也算）。
    Cleared,
}

/// **存下这一套**，换掉工作目录里原来那一套。四样都空着就是**不留账号**：那份文件删掉。
///
/// 先写进旁边一份临时文件再改名换上去，读的那一侧读到的要么是旧的一套、要么是新的一套。
/// **密码落盘之前权限就已经收好**（Unix 上 0600）：先建文件、收权限，再往里写字——没有哪一刻
/// 那几个密码躺在一份别人读得了的文件里。原来那份权限松的，换上去之后也收紧了。
///
/// # Errors
/// 开发者那两样缺一样时返回 [`AccountError::Incomplete`]（一个字节都不写）；建目录、写、收权限、
/// 改名或删除失败时返回 [`AccountError::Write`]。
pub fn save_account(workspace: &Path, account: &Account) -> Result<Saved, AccountError> {
    let path = workspace::screenscraper_account_path(workspace);
    let failed = |at: &Path, source: std::io::Error| AccountError::Write {
        path: crate::path::display(at),
        source,
    };
    if account.is_empty() {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(Saved::Cleared),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Saved::Cleared),
            Err(error) => Err(failed(&path, error)),
        };
    }
    if !account.is_complete() {
        return Err(AccountError::Incomplete);
    }
    let body =
        toml::to_string(account).map_err(|error| failed(&path, std::io::Error::other(error)))?;
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|error| failed(parent, error))?;
    }
    let mut temp = path.as_os_str().to_os_string();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    write_private(&temp, &format!("{}{body}", header())).map_err(|error| failed(&temp, error))?;
    std::fs::rename(&temp, &path).map_err(|error| failed(&path, error))?;
    Ok(Saved::Stored)
}

/// 写一份**只本人读得了**的文件：先建（或截空）、收权限，再写字。
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    // `mode` 只管**新建**的那一刻，而且要过一道 umask；上一趟改名之前断掉、留下的那份临时文件
    // 权限是它自己的。所以不论新旧，写字之前再明着收一次。
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(text.as_bytes())?;
    file.sync_all()
}
