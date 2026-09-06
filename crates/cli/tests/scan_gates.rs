//! `romcat scan` 开工之前的两道闸：根不在就别建库，两个打架的开关别默默收下。
//!
//! 两条都长在同一句话上——**判据要在开工之前给出，而不是开工之后**。
//!
//! - **根不在**时 `scan` 也退 1，可退之前 `Catalog::open` 已经把中立库文件建出来了。
//!   一个打错的路径从此在工作目录里算「存在」，往后 `romcat report` / `triage list`
//!   都当它是一份真库；而中立库是**每个主库一份**（`CONTEXT.md`），凭空多出来的那些
//!   全是垃圾。开库前先看一眼这个根在不在，一次 `stat` 的事。
//! - `--resume` 与 `--no-checkpoint` 同时给：不写断点也就没有断点可续，`--resume`
//!   从头到尾没被读到，标准错误上一个字都不提——用户以为接着扫，实际整个库重扫一遍。

use std::fs;
use std::path::Path;
use std::process::Command;

use romcat_core::testing::temp_dir;

fn 扫() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_romcat"));
    command.arg("scan");
    command
}

/// 工作目录里眼下有哪些文件，排好序。
fn 工作目录里的文件(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(
                    path.strip_prefix(dir)
                        .unwrap_or(&path)
                        .display()
                        .to_string(),
                );
            }
        }
    }
    out.sort();
    out
}

#[test]
fn 根目录不在时一个中立库文件都不留下() {
    // ⭐ **建库这件事本身就是开工。** `Catalog::open` 建文件建表在前，`scan::scan` 里
    // 化开扫描根失败在后，于是一条打错的路径也会在工作目录里留下一份空中立库。
    let workspace = temp_dir("scan-gate-missing-root");
    let 不存在 = workspace.path().join("压根没有这个目录");

    let output = 扫()
        .arg(&不存在)
        .arg("--workspace")
        .arg(workspace.path())
        .args(["--no-checkpoint", "--samples-per-class", "0", "--quiet"])
        .output()
        .expect("能启动 romcat");

    assert!(!output.status.success(), "根不在就该退非零");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("扫描根不可用"),
        "得说清是哪一条判据拦下来的，实际是：{stderr}"
    );
    assert_eq!(
        工作目录里的文件(workspace.path()),
        Vec::<String>::new(),
        "开工之前就该拦下来，中立库一个字节都不该建出来"
    );
}

#[test]
fn 续跑与不写断点同时给要当场拦下() {
    // ⭐ 不写断点就没有断点可续。两个一起给的时候 `--resume` 从头到尾没被读到，
    // 而扫描照跑——用户以为接着扫，实际是整个库重扫一遍，10T 库上那是几个钟头。
    let workspace = temp_dir("scan-gate-resume-conflict");
    let 主库 = temp_dir("scan-gate-library");
    fs::create_dir_all(主库.path().join("FC")).expect("能建目录");
    fs::write(主库.path().join("FC/魂斗罗.zip"), b"pretend").expect("能写文件");

    let output = 扫()
        .arg(主库.path())
        .args(["--root-name", "库"])
        .arg("--workspace")
        .arg(workspace.path())
        .args([
            "--resume",
            "--no-checkpoint",
            "--samples-per-class",
            "0",
            "--quiet",
        ])
        .output()
        .expect("能启动 romcat");

    assert!(!output.status.success(), "两个打架的开关不许默默收下");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("--resume") && stderr.contains("--no-checkpoint"),
        "得把打架的那两个开关都点出来，实际是：{stderr}"
    );
}
