//! **ScreenScraper 的账号落盘**（票 `verdict-store-and-sync/15`，收挂单 `Q1063`）。
//!
//! 从 Finder 启动的界面读不到开工具之前给的环境变量，于是联网刮削在界面上实际上用不了。
//! 已裁的形状（`grill.md` 的 `Q1063`）：账号存**工作目录**里一份**只本机本人读得了**（0600）的文件，
//! **只一套**；读的时候**环境变量优先，其次那份文件**；核心库一处读写，界面与命令行都走它。
//!
//! 这里验的是那一处本身：写出去、读得回、权限对、谁优先、只一套。
//! **一个网络请求都不发**，测试里的账号全是编的——真账号不许进夹具、不许进日志。

use std::fs;

use romcat_core::scrape::online::{self, Account, AccountFrom, Saved};
use romcat_core::testing::temp_dir;
use romcat_core::workspace;

/// 编出来的一套账号。四样都给。
fn 一套() -> Account {
    Account {
        dev_id: "开发者甲".to_owned(),
        dev_password: "开发者密码甲".to_owned(),
        user: "用户甲".to_owned(),
        user_password: "用户密码甲".to_owned(),
    }
}

/// 一个什么环境变量都没给的进程。**不读这台机器的真环境变量**：维护者的机器上可能真配着一套。
fn 没给环境变量(_: &str) -> Option<String> {
    None
}

#[test]
fn 写出账号之后读得回_文件只有本人读得了() {
    let 工作目录 = temp_dir("账号-落盘");
    assert_eq!(
        online::save_account(工作目录.path(), &一套()).expect("写得出账号"),
        Saved::Stored
    );

    let 文件 = workspace::screenscraper_account_path(工作目录.path());
    assert!(
        文件.starts_with(工作目录.path()),
        "账号文件该住在工作目录里：{}",
        文件.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let 权限 = fs::metadata(&文件).expect("文件在").permissions().mode() & 0o777;
        assert_eq!(权限, 0o600, "账号文件的权限是 {权限:o}，不是 600");
    }

    let 读回 = online::find_account_with(工作目录.path(), &没给环境变量).expect("读得动");
    assert_eq!(读回, Some((一套(), AccountFrom::File)));
    assert_eq!(
        online::saved_account(工作目录.path()).expect("读得动"),
        Some(一套())
    );
}

#[cfg(unix)]
#[test]
fn 原先那份文件谁都读得了_重写之后收成只本人读得了() {
    use std::os::unix::fs::PermissionsExt as _;

    let 工作目录 = temp_dir("账号-收紧");
    let 文件 = workspace::screenscraper_account_path(工作目录.path());
    fs::write(&文件, "devid = \"旧\"\ndevpassword = \"旧\"\n").expect("摆得下一份旧文件");
    fs::set_permissions(&文件, fs::Permissions::from_mode(0o644)).expect("改得动权限");

    online::save_account(工作目录.path(), &一套()).expect("写得出账号");
    let 权限 = fs::metadata(&文件).expect("文件在").permissions().mode() & 0o777;
    assert_eq!(权限, 0o600, "重写之后权限还是 {权限:o}");
}

#[test]
fn 只存一套_再存一次就换掉上一套() {
    let 工作目录 = temp_dir("账号-一套");
    online::save_account(工作目录.path(), &一套()).expect("写得出账号");
    let 另一套 = Account {
        dev_id: "开发者乙".to_owned(),
        dev_password: "开发者密码乙".to_owned(),
        user: String::new(),
        user_password: String::new(),
    };
    online::save_account(工作目录.path(), &另一套).expect("写得出账号");
    assert_eq!(
        online::saved_account(工作目录.path()).expect("读得动"),
        Some(另一套)
    );
}

#[test]
fn 文件里摆着第二套就读不懂_不挑一套将就着用() {
    // 轮换账号绕配额的处置是永久封禁（ADR-0007）：文件的形状本身就只装得下一套，
    // 人手抄进第二套时**如实说读不懂**，而不是悄悄挑其中一套。
    let 工作目录 = temp_dir("账号-两套");
    let 文件 = workspace::screenscraper_account_path(工作目录.path());
    fs::write(
        &文件,
        "devid = \"甲\"\ndevpassword = \"甲\"\n\n[另一套]\ndevid = \"乙\"\ndevpassword = \"乙\"\n",
    )
    .expect("摆得下");
    assert!(online::saved_account(工作目录.path()).is_err());
    assert!(online::find_account_with(工作目录.path(), &没给环境变量).is_err());
}

#[test]
fn 开发者那两样缺一样就存不进去() {
    // 没有 devid 的请求直接 403，而那是白扣一次的——存一套注定 403 的账号没有意义。
    let 工作目录 = temp_dir("账号-缺一样");
    let 缺开发者密码 = Account {
        dev_password: String::new(),
        ..一套()
    };
    assert!(online::save_account(工作目录.path(), &缺开发者密码).is_err());
    assert!(
        !workspace::screenscraper_account_path(工作目录.path()).exists(),
        "存不进去就不该留下一份文件"
    );
}

#[test]
fn 四样都清空再存就是不留账号() {
    let 工作目录 = temp_dir("账号-清空");
    online::save_account(工作目录.path(), &一套()).expect("写得出账号");
    assert_eq!(
        online::save_account(工作目录.path(), &Account::default()).expect("清得掉"),
        Saved::Cleared
    );
    assert!(!workspace::screenscraper_account_path(工作目录.path()).exists());
    assert_eq!(
        online::find_account_with(工作目录.path(), &没给环境变量).expect("读得动"),
        None
    );
}

/// 环境变量里给了一套（开发者那两样都在）。
fn 环境变量里一套(key: &str) -> Option<String> {
    match key {
        "SCREENSCRAPER_DEVID" => Some("开发者丙".to_owned()),
        "SCREENSCRAPER_DEVPASSWORD" => Some("开发者密码丙".to_owned()),
        "SCREENSCRAPER_SSID" => Some("用户丙".to_owned()),
        _ => None,
    }
}

#[test]
fn 设了环境变量时环境变量优先() {
    let 工作目录 = temp_dir("账号-环境变量优先");
    online::save_account(工作目录.path(), &一套()).expect("写得出账号");
    let 读到 = online::find_account_with(工作目录.path(), &环境变量里一套).expect("读得动");
    assert_eq!(
        读到,
        Some((
            Account {
                dev_id: "开发者丙".to_owned(),
                dev_password: "开发者密码丙".to_owned(),
                user: "用户丙".to_owned(),
                user_password: String::new(),
            },
            AccountFrom::Env,
        )),
        "两样都在时该用环境变量那一套，而且**不拼**——一样都不从文件里补"
    );
}

#[test]
fn 环境变量只给了半套时不算_退到文件那一套() {
    // 开发者那两样缺一样，环境变量那一套就不成立；两边**不拼成一套**。
    let 工作目录 = temp_dir("账号-半套");
    online::save_account(工作目录.path(), &一套()).expect("写得出账号");
    let 只给用户名 = |key: &str| (key == "SCREENSCRAPER_SSID").then(|| "用户丁".to_owned());
    assert_eq!(
        online::find_account_with(工作目录.path(), &只给用户名).expect("读得动"),
        Some((一套(), AccountFrom::File))
    );
}

#[test]
fn 有没有账号_文件那一条也算() {
    // `has_account` 读的是这个进程真的环境变量；这里只验「文件在就算有」这一半——
    // 环境变量那一半由 `find_account_with` 那几条验，它与 `has_account` 走的是同一条路。
    let 工作目录 = temp_dir("账号-有没有");
    online::save_account(工作目录.path(), &一套()).expect("写得出账号");
    assert!(online::has_account(工作目录.path()));
}

#[test]
fn 账号拼成的凭据带着那一套_空着的两样不发() {
    let 凭据 = Account {
        user: String::new(),
        user_password: String::new(),
        ..一套()
    }
    .credentials();
    assert_eq!(凭据.dev_id, "开发者甲");
    assert_eq!(凭据.dev_password, "开发者密码甲");
    assert_eq!(凭据.user, None);
    assert_eq!(凭据.user_password, None);
}

#[test]
fn 账号印进日志时不露密码() {
    let 印出来 = format!("{:?}", 一套());
    for 密码 in ["开发者密码甲", "用户密码甲"] {
        assert!(!印出来.contains(密码), "Debug 印出了密码：{印出来}");
    }
}

#[test]
fn 文件读不懂时报错不带那一行原文_密码不跟着报错出去() {
    // TOML 的报错照例把出错那一行原文引出来；人手把密码写漏了引号，那一行就是密码本身。
    // 这句报错要上设置屏的红字、上命令行的 stderr——它只许说第几行、为什么，不许引原文。
    for (那一行, 密码) in [
        ("sspassword = 编的密码丁", "编的密码丁"),
        ("sspassword = 9876543", "9876543"),
    ] {
        let 工作目录 = temp_dir("账号-读不懂");
        fs::write(
            workspace::screenscraper_account_path(工作目录.path()),
            format!("devid = \"甲\"\ndevpassword = \"甲\"\n{那一行}\n"),
        )
        .expect("摆得下");
        let 错 = online::saved_account(工作目录.path()).expect_err("该读不懂");
        let 说的 = format!("{错}\n{错:?}");
        assert!(!说的.contains(密码), "报错把密码带出去了：{说的}");
        assert!(说的.contains("第 3 行"), "该说清是第几行：{说的}");
        assert!(
            std::error::Error::source(&错).is_none(),
            "底层那份 TOML 报错带着原文，不许挂在错误链上"
        );
    }
}
