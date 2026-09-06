//! **拿键折回盘上真名再开**：键是 NFC，盘上那个名字可能是分解形式（ADR-0020）。
//!
//! 键一律 NFC，而 macOS 的 NTFS 驱动交出来的名字实测 1.99% 是分解形式；到了**分解
//! 敏感**的文件系统上（Windows 的 NTFS、Linux 的 ext4，而 ADR-0018 说主力机正是
//! Windows），把键直接接在根后面拼出来的那条路径根本开不了。开不了这件事在每条路上
//! 都会被解释成别的意思——识别读成「无判据」、重解名字读成**不可读**、媒体入池读成
//! 「读不动」——而盘明明好好的。
//!
//! 这里钉的是两条此前漏掉的：**重解名字**（`scan::names`）与**媒体入池**
//! （`scrape::pool`）。两条都只证一件事：盘上是分解形式时它们照样读得到那个文件。
//!
//! fixture 用 [`MemFs`]，它默认**分大小写**、也**分解敏感**——`か` + U+3099 与
//! 预组合的 `が` 是两个不同的名字，正是本机的 fskit NTFS 驱动**看不见**的那种盘。

use std::path::Path;

use romcat_core::catalog::{Catalog, EntryRecord, Roots, Verdict};
use romcat_core::container::{ContainerKind, Contents, InnerEntry, Penetration};
use romcat_core::fs::{EntryKind, EntryMeta, LibraryFs, MemFs};
use romcat_core::scan::{self, CancelToken};
use romcat_core::scrape::pool::{self, Ingested, MediaPool};
use romcat_core::testing::container::{ZipEntrySpec, zip_container};
use romcat_core::testing::temp_dir;

/// 盘上那一段目录名：`か` 加组合浊音符 U+3099，**分解形式**。
const 分解: &str = "か\u{3099}";
/// 同一个名字的预组合形式 `が`（U+304C）。中立库的键里存的是它。
const 组合: &str = "が";

/// `上海大亨.nes` 的 GBK 字节——真库 `FC/【HACK版游戏】/…` 里那条名字。
/// 按票 03 的有损转换落库之后每个坏字节都成了 `U+FFFD`，再也回不来。
const GBK: &[u8] = &[
    0xc9, 0xcf, 0xba, 0xa3, 0xb4, 0xf3, 0xba, 0xe0, 0x2e, 0x6e, 0x65, 0x73,
];

#[test]
fn 盘上是分解形式时重解名字那一趟照样打得开容器() {
    let 盘上 = format!("/库/FC/{分解}/上海大亨.zip");
    let 键 = format!("库/FC/{组合}/上海大亨.zip");
    let 字节 = zip_container(&[ZipEntrySpec::stored_raw(GBK, vec![0xE5_u8; 4_096])]);
    let mut library = MemFs::new();
    library.file(&盘上, 字节.clone());

    // 这份盘**分解敏感**：拼出来的那条 NFC 路径开不了，折回真名才开得了。
    // 前提不成立的话下面证的就不是这件事了。
    assert!(
        library
            .open(Path::new(&format!("/库/FC/{组合}/上海大亨.zip")))
            .is_err()
    );
    assert!(library.open(Path::new(&盘上)).is_ok());

    let mut catalog = Catalog::open_in_memory().expect("能开中立库");
    let 有损 = String::from_utf8_lossy(GBK).into_owned();
    assert!(有损.contains('\u{fffd}'));
    catalog
        .write(
            1,
            &[EntryRecord {
                key: 键.clone(),
                kind: EntryKind::File,
                meta: EntryMeta::Known {
                    len: 字节.len() as u64,
                    modified: None,
                },
                non_utf8: false,
                verdict: Verdict::Changed,
                sample: None,
                container: Some(Penetration {
                    kind: ContainerKind::Zip,
                    contents: Contents {
                        entries: vec![InnerEntry {
                            path: 有损.clone(),
                            size: 4_096,
                            crc32: Some(0),
                            is_dir: false,
                            block: Some(0),
                            name_lossy: true,
                        }],
                        blocks: 1,
                    },
                    failure: None,
                }),
            }],
        )
        .expect("写得进去");

    let outcome = scan::names::recheck(
        &library,
        &mut catalog,
        &Roots::single("库", Path::new("/库")),
        &CancelToken::new(),
        &mut |_| {},
    )
    .expect("重读跑得动");

    assert_eq!(outcome.containers, 1);
    // 开不了就静默计入**不可读**、名字永远解不对——这一条正是那个 bug。
    assert_eq!(outcome.unreadable, 0, "折回真名之后不该还有读不动的");
    assert_eq!(outcome.reread, 1);
    assert_eq!(outcome.renamed, 1);
    assert_eq!(outcome.still_lossy, 0);
    let 名字 = catalog
        .container_files(&键)
        .expect("读得到")
        .first()
        .map(|it| it.0.clone())
        .expect("有一条");
    assert_eq!(名字, "上海大亨.nes");
}

#[test]
fn 盘上是分解形式时媒体照样收得进池() {
    let dir = temp_dir("realname-pool");
    let pool = MediaPool::open(&dir.path().join("池")).expect("开得了池");
    let 盘上 = format!("/库/media/{分解}/封面.png");
    let 键 = format!("库/media/{组合}/封面.png");
    let mut library = MemFs::new();
    library.file(&盘上, vec![7_u8; 64]);
    let mut catalog = Catalog::open_in_memory().expect("能开中立库");

    let got = pool::ingest(
        &library,
        &mut catalog,
        &pool,
        &Roots::single("库", Path::new("/库")),
        &pool::Claim {
            key: &键,
            bytes: None,
            max_bytes: None,
        },
    )
    .expect("入池跑得动");

    // 拼出来那条开不了会被报成**读不动**——而盘好好的，那张封面从此永远收不进来。
    assert!(
        matches!(got, Ingested::Stored { bytes: 64, .. }),
        "该收进来，实际是 {got:?}"
    );
}
