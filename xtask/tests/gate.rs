//! 门禁那条子命令的钉子。
//!
//! 这份文件只钉两件事，别的都不该在这儿验：
//!
//! - **fmt 那一条真的会拦人。** 给它一份故意不 fmt-clean 的输入，它红；
//!   跑一趟 `cargo fmt --all` 改回来，它绿。这条走的是**真的子进程**——
//!   门禁的价值全在「它真的会退非零」上，断言参数串对不对是验不到这件事的。
//! - **定义只有一份。** 四条的名字与形态从 `gate::steps` 一处来，
//!   README 与 CI 都只指向它。

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs};

use xtask::gate::{Limits, Step, steps};

// ── 一份用完即删的丢弃 crate ──
//
// 落在系统临时目录里，因此**不在本仓库的 workspace 里**，也读不到仓库根的
// `rustfmt.toml`。这条测试要验的是「门禁这一步会不会红」，不是那份配置。

static 序号: AtomicU64 = AtomicU64::new(0);

/// 一个用完即删的临时目录。
struct 丢弃目录 {
    path: PathBuf,
}

impl Drop for 丢弃目录 {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// 造一份最小的独立 crate，`src/lib.rs` 的内容由调用方给。
fn 丢弃crate(tag: &str, lib: &str) -> 丢弃目录 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let unique = 序号.fetch_add(1, Ordering::Relaxed);
    let path = env::temp_dir().join(format!(
        "romcat-xtask-{tag}-{}-{nanos}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(path.join("src")).expect("能建临时目录");
    fs::write(
        path.join("Cargo.toml"),
        // `[workspace]` 那一行是**封口用的**，不是摆设：cargo 会从这个临时目录一路往上找
        // workspace 根，而 `TMPDIR` 被指进某个 cargo workspace 里头并不罕见（容器 / CI）。
        // 少了它，`cargo fmt` 会报「current package believes it's in a workspace when it's not」
        // 而红——红的原因跟门禁毫无关系，正是这条测试最该躲开的那种假红。
        "[package]\nname = \"丢弃\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n[workspace]\n",
    )
    .expect("能写 Cargo.toml");
    fs::write(path.join("src/lib.rs"), lib).expect("能写 lib.rs");
    丢弃目录 { path }
}

/// 门禁第一条（`cargo fmt --all --check`）在 `dir` 上跑出来的退出码是不是 0。
fn fmt这一条绿吗(dir: &Path) -> bool {
    let 四条 = steps(Limits::default());
    let fmt = 四条.first().expect("门禁至少有一条");
    assert_eq!(
        fmt.name, "fmt",
        "第一条得是 fmt——它最便宜，红了就别再编译整个仓库"
    );
    fmt.command_in(dir)
        .status()
        .expect("能起 cargo 子进程")
        .success()
}

#[test]
fn 排版不干净就红改回来就绿() {
    // ⭐ **门禁的全部价值在这一条上。** 四条命令写在 README 里的时候，「它会不会红」
    // 靠的是人记得跑；写成子命令之后这件事才第一次有东西钉着。
    let dir = 丢弃crate("fmt-red", "pub fn 加(a:i32,b:i32)->i32{a+b}\n");

    assert!(
        !fmt这一条绿吗(&dir.path),
        "参数挤在一起、大括号贴着函数体的源码，`cargo fmt --all --check` 必须退非零"
    );

    let 改回来 = Command::new(xtask::gate::cargo())
        .arg("fmt")
        .arg("--all")
        .current_dir(&dir.path)
        .status()
        .expect("能起 cargo fmt");
    assert!(改回来.success(), "`cargo fmt --all` 本身得跑得通");

    assert!(
        fmt这一条绿吗(&dir.path),
        "同一份输入 fmt 过之后，门禁第一条必须绿——否则它红的不是排版，是别的什么"
    );
}

#[test]
fn 门禁就是四条而且顺序是从便宜到贵() {
    // ⭐ **顺序不是随手排的**：fmt 两秒就出结果，`test` 要几分钟。默认在第一处红上就停，
    // 于是「忘了跑 fmt」这种最常见的红，代价是两秒而不是一整趟编译。
    let 四条 = steps(Limits::default());
    let 名字: Vec<&str> = 四条.iter().map(|it| it.name).collect();
    assert_eq!(名字, ["fmt", "clippy", "test", "doc"]);
}

#[test]
fn 四条打印出来就是原先写在_readme_里的那四条() {
    // ⭐ 这条钉的是**定义只有一份**：README 的门禁块从此只写 `cargo xtask gate`，
    // 「它到底跑什么」由 `cargo xtask gate --list` 当场打印，不再有第二份手抄的
    // 清单在旁边慢慢漂。下面这四行字面量抄自门禁落地之前 README 的开发块——
    // 它们是**独立的比对基准**，不是从 `steps` 反推出来的。
    let 打印: Vec<String> = steps(Limits::default()).iter().map(Step::display).collect();
    assert_eq!(
        打印,
        [
            "cargo fmt --all --check",
            "cargo clippy --workspace --all-targets --all-features",
            "cargo test --workspace --all-features",
            "cargo doc --workspace --no-deps",
        ]
    );
}

#[test]
fn 限流那一档把两个开关都递下去() {
    // ⭐ 15 GB 的开发机上跑全量测试会被 OOM 杀掉，所以要能退回限流那一档。
    // 两个开关**各管一段**：`-j` 管编译期的并行 rustc，`--test-threads` 管跑测试时
    // 同时活着的测试线程数——只限其中一个，另一段照样能把内存吃光。
    let 限流 = steps(Limits::throttled());
    let test = 限流
        .iter()
        .find(|it| it.name == "test")
        .expect("有 test 这一条");
    assert!(test.args.contains(&"-j".to_string()));
    assert!(test.args.contains(&"1".to_string()));
    assert!(test.args.contains(&"--test-threads=2".to_string()));

    // 默认那一档一个都不递——按机器给的资源跑。
    let 默认 = steps(Limits::default());
    let test = 默认
        .iter()
        .find(|it| it.name == "test")
        .expect("有 test 这一条");
    assert!(!test.args.iter().any(|a| a == "-j"));
    assert!(!test.args.iter().any(|a| a.starts_with("--test-threads")));
}
