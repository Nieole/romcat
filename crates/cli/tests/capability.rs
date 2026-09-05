//! 端到端验收 `romcat capability` 这一组，以及子库怎么挑一份**能力档案**。
//!
//! 这条命令的全部意义是**让矩阵可被复核**。ADR-0017 的原话：矩阵错误比不转换更糟
//! ——用户会以为工具已经处理妥当，直到在掌机上打不开才发现。于是 `show` 印的不是
//! 「支持哪些格式」这一行结论，而是**结论加它的出处**：哪个源码文件、哪份官方文档、
//! 哪天核实的。这个文件验的正是那几样真的印出来了。
//!
//! 工作目录一律显式指到临时目录：绝不能让测试往开发者真实的
//! `~/.local/share/romcat` 里写东西。目标设备一律拿本地临时目录模拟。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::sample::zip;
use romcat_core::testing::{TempDir, temp_dir};

fn 写(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().expect("有上级目录")).expect("能建目录");
    fs::write(path, bytes).expect("能写文件");
}

fn romcat(workspace: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_romcat"))
        .args(args)
        .args(["--workspace", &workspace.display().to_string()])
        .output()
        .expect("能启动 romcat")
}

fn 出来的话(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn 现场() -> (TempDir, TempDir) {
    let dir = temp_dir("cap-cli");
    let workspace = temp_dir("cap-cli-ws");
    写(&dir.path().join("FC/魂斗罗.zip"), &zip(2048));
    写(&dir.path().join("SFC/超级玛丽.rar"), &zip(4096));
    let out = romcat(
        workspace.path(),
        &[
            "scan",
            "--root-name",
            "库",
            &dir.path().display().to_string(),
            "--library",
            "测试库",
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    (dir, workspace)
}

#[test]
fn 列得出内置档案_各自对着什么设备与什么文件系统() {
    let workspace = temp_dir("cap-list-ws");
    let out = romcat(workspace.path(), &["capability"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    for name in [
        "不作声称",
        "retroarch-exfat",
        "retroarch-fat32",
        "es-de-exfat",
    ] {
        assert!(text.contains(name), "该列出「{name}」：{text}");
    }
    assert!(text.contains("FAT32"), "{text}");
    assert!(text.contains("内置"), "该说清用的是内置那一份：{text}");
}

#[test]
fn show_印的是结论加它的出处与核实日期() {
    let workspace = temp_dir("cap-show-ws");
    let out = romcat(workspace.path(), &["capability", "retroarch-fat32"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);

    // ⭐ 目标存储那一半：**FAT32 的 4 GiB 单文件上限**（ADR-0017 补充段）。
    assert!(text.contains("单文件上限"), "{text}");
    assert!(text.contains("4.00 GiB"), "{text}");
    assert!(text.contains("文件名上限"), "{text}");
    assert!(text.contains("完整路径上限"), "{text}");
    // 出处与核实日期，每条都得有。
    assert!(text.contains("核实"), "{text}");
    assert!(text.contains("来源"), "{text}");
    assert!(
        text.contains("archive_file.c"),
        "RetroArch 那条要写清是哪个源码文件：{text}",
    );
    assert!(
        text.contains("天前"),
        "核实日期要印成「多少天前」，陈没陈旧一眼看得出：{text}",
    );
    // 「不作声称」是一等状态，印得出来。
    assert!(text.contains("不作声称"), "{text}");
}

#[test]
fn 认不出的档案名当场报错并列出有哪些() {
    let workspace = temp_dir("cap-bad-ws");
    let out = romcat(workspace.path(), &["capability", "没有这一份"]);
    assert!(!out.status.success());
    let text = 出来的话(&out);
    assert!(text.contains("没有叫「没有这一份」"), "{text}");
    assert!(text.contains("retroarch-exfat"), "要列出有哪些：{text}");
}

#[test]
fn 导得出底稿_改完放进工作目录就自动生效() {
    let workspace = temp_dir("cap-dump-ws");
    let 底稿 = workspace.path().join("导出的.toml");
    let out = romcat(
        workspace.path(),
        &["capability", "--dump-builtin", &底稿.display().to_string()],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(底稿.is_file());
    let 原文 = fs::read_to_string(&底稿).expect("读得出");
    assert!(原文.contains("这份文件是数据不是代码"), "{原文}");

    // 改一份自己的放进工作目录：**自动生效**，而且 `capability` 印的就是它。
    let 我的 = 原文.replace(r#""名" = "es-de-exfat""#, r#""名" = "我的掌机""#);
    fs::write(workspace.path().join("capability.toml"), &我的).expect("写得进");
    let text = 出来的话(&romcat(workspace.path(), &["capability"]));
    assert!(text.contains("我的掌机"), "{text}");
    assert!(!text.contains("es-de-exfat"), "整份换掉了：{text}");
    assert!(!text.contains("用的是**内置**"), "{text}");
}

#[test]
fn 坏掉的名册不静默退回内置的() {
    // 用户放了一份在工作目录里就是要用它。退回内置的等于拿另一张矩阵替他做决定，
    // 而那正是 ADR-0017 说的「用户以为工具已经处理妥当」。
    let workspace = temp_dir("cap-broken-ws");
    fs::write(workspace.path().join("capability.toml"), "这不是 TOML {{{").expect("写得进");
    let out = romcat(workspace.path(), &["capability"]);
    assert!(!out.status.success(), "{}", 出来的话(&out));
    assert!(出来的话(&out).contains("解析失败"), "{}", 出来的话(&out));
}

#[test]
fn 子库挑得了档案_挑不存在的当场拦下来() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    let 卡 = ws.join("卡");
    fs::create_dir_all(&卡).expect("能建目标目录");
    let 卡 = 卡.display().to_string();

    // 不挑：默认**不作声称**，而且说清那是什么意思。
    let out = romcat(
        ws,
        &[
            "sublibrary",
            "set",
            "掌机",
            "--target",
            &卡,
            "--library",
            "测试库",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("不作声称"), "{text}");
    assert!(text.contains("不转换、不检查"), "{text}");

    // 挑一个不存在的：**当场拦下来**。存下去的话，同步时会悄悄退回「不作声称」，
    // 于是用户以为配了 FAT32 的检查，其实一条都没查。
    let out = romcat(
        ws,
        &[
            "sublibrary",
            "set",
            "掌机",
            "--capability",
            "没有这一份",
            "--library",
            "测试库",
        ],
    );
    assert!(!out.status.success(), "{}", 出来的话(&out));
    assert!(
        出来的话(&out).contains("没有叫「没有这一份」的能力档案"),
        "{}",
        出来的话(&out)
    );

    // 挑一个真的：**存得住**，另起一次进程照样读得回来。
    let out = romcat(
        ws,
        &[
            "sublibrary",
            "set",
            "掌机",
            "--capability",
            "retroarch-fat32",
            "--library",
            "测试库",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    assert!(
        出来的话(&out).contains("retroarch-fat32"),
        "{}",
        出来的话(&out)
    );
    let text = 出来的话(&romcat(ws, &["sublibrary", "list", "--library", "测试库"]));
    assert!(text.contains("retroarch-fat32"), "{text}");
}

#[test]
fn 差量预览里说得出哪些到了掌机上打不开() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    let 卡 = ws.join("卡2");
    fs::create_dir_all(&卡).expect("能建目标目录");
    let 卡 = 卡.display().to_string();
    let out = romcat(
        ws,
        &[
            "sublibrary",
            "set",
            "掌机",
            "--target",
            &卡,
            "--capability",
            "retroarch-exfat",
            "--library",
            "测试库",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let out = romcat(
        ws,
        &[
            "sublibrary",
            "rule",
            "掌机",
            "--add",
            "平台=FC,SFC",
            "--library",
            "测试库",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));

    let out = romcat(ws, &["sublibrary", "plan", "掌机", "--library", "测试库"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("能力档案        retroarch-exfat"), "{text}");
    // ⭐ RetroArch 不支持 rar（源码级：`archive_file_rar.c` 根本不存在），
    // 而这一版转不了它——**如实报出来**，而不是悄悄搬过去让人在掌机上才发现。
    assert!(text.contains("到了目标上打不开"), "{text}");
    assert!(text.contains("超级玛丽.rar"), "{text}");
    assert!(
        text.contains("romcat capability retroarch-exfat"),
        "要指得出这条判断的出处在哪儿看：{text}",
    );
}

#[test]
fn 子库记着的档案在名册里没有时不静默当没事_明说退回了不作声称() {
    let (_library, workspace) = 现场();
    let ws = workspace.path();
    let 卡 = ws.join("卡3");
    fs::create_dir_all(&卡).expect("能建目标目录");
    let 卡 = 卡.display().to_string();
    let out = romcat(
        ws,
        &[
            "sublibrary",
            "set",
            "掌机",
            "--target",
            &卡,
            "--capability",
            "retroarch-exfat",
            "--library",
            "测试库",
        ],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));

    // 换一份名册，里面没有 `retroarch-exfat`。
    let 底稿 = ws.join("底稿.toml");
    let out = romcat(
        ws,
        &["capability", "--dump-builtin", &底稿.display().to_string()],
    );
    assert!(out.status.success(), "{}", 出来的话(&out));
    let 原文 = fs::read_to_string(&底稿).expect("读得出");
    fs::write(
        ws.join("capability.toml"),
        原文.replace(r#""名" = "retroarch-exfat""#, r#""名" = "别的名字""#),
    )
    .expect("写得进");

    let out = romcat(ws, &["sublibrary", "plan", "掌机", "--library", "测试库"]);
    assert!(out.status.success(), "{}", 出来的话(&out));
    let text = 出来的话(&out);
    assert!(text.contains("找不到"), "{text}");
    assert!(text.contains("别以为它替你查过了"), "{text}");
    // ⭐ **预览头上印的必须是真正生效的那一份**，不是子库上记着的那个名字。
    // 印着 `retroarch-exfat` 而实际一条都没查，正是「用户以为工具已经处理妥当」。
    assert!(text.contains("能力档案        不作声称"), "{text}");
    assert!(
        !text.contains("能力档案        retroarch-exfat"),
        "退回之后不该还印着原来那个名字：{text}",
    );
}
