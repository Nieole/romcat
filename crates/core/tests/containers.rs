//! 穿透**透明容器**：zip 与 7z 的零解压读取、部分解压与按块调度。
//!
//! 这些测试守的是票 03 的地基：**不解压就拿到 CRC-32 + 未压缩大小**，以及
//! **7z 的 solid block 一块只解一次**。后者不成立的话，主库那 4.71 TiB 的 7z
//! 会以约 100 倍的解压量跑，项目就不可行了。

use std::io::{Cursor, Read};

use romcat_core::container::{
    self, ContainerError, ContainerKind, Demand, InnerEntry, ReadPlan, ReadStats,
};
use romcat_core::fs::MemFs;
use romcat_core::testing::container::{
    ZipEntrySpec, crc32, zip_container, zip_container_with_prefix, zip_container_with_trailing,
    zip_container_with_zip64_eocd,
};

use sevenz_rust2::{ArchiveEntry, ArchiveWriter, SourceReader};

/// 一份看得出内容、又压得动的样本：不是全零，否则 deflate 会压到几乎没有，
/// 「读够就停」省下了多少就看不出来。
fn 样本(seed: u8, len: usize) -> Vec<u8> {
    (0..len)
        .map(|i| seed.wrapping_add((i % 251) as u8).wrapping_mul(31))
        .collect()
}

fn 建库(name: &str, bytes: Vec<u8>) -> MemFs {
    let mut library = MemFs::new();
    library.file(name, bytes);
    library
}

/// 造一个 7z。`solid` 为真时全部条目压进**一个块**。
fn 七z(entries: &[(&str, Vec<u8>)], solid: bool) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut writer = ArchiveWriter::new(&mut buffer).expect("能开始写 7z");
        if solid {
            let specs = entries
                .iter()
                .map(|(name, _)| ArchiveEntry::new_file(name))
                .collect();
            let readers = entries
                .iter()
                .map(|(_, data)| SourceReader::new(Cursor::new(data.clone())))
                .collect();
            writer.push_archive_entries(specs, readers).expect("能写");
        } else {
            for (name, data) in entries {
                writer
                    .push_archive_entry(
                        ArchiveEntry::new_file(name),
                        Some(Cursor::new(data.clone())),
                    )
                    .expect("能写");
            }
        }
        writer.finish().expect("能收尾");
    }
    buffer.into_inner()
}

fn 全读(
    library: &MemFs,
    path: &str,
    listing: &container::Listing,
    plan: &ReadPlan,
) -> (ReadStats, Vec<(String, Vec<u8>)>) {
    let mut got: Vec<(String, Vec<u8>)> = Vec::new();
    let stats = container::read_entries(
        library,
        std::path::Path::new(path),
        listing,
        plan,
        &mut |entry: &InnerEntry, reader: &mut dyn Read| {
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf)?;
            got.push((entry.path.clone(), buf));
            Ok(())
        },
    )
    .expect("读得出来");
    (stats, got)
}

// ───────────────────────────── zip ─────────────────────────────

#[test]
fn zip_不解压就读出内部文件的名字与未压缩大小与_crc32() {
    let 存储 = 样本(1, 300);
    let 压缩 = 样本(9, 4096);
    let bytes = zip_container(&[
        ZipEntrySpec::stored("超级马里奥.nes", 存储.clone()),
        ZipEntrySpec::deflated("魂斗罗.nes", 压缩.clone()),
    ]);
    let library = 建库("/lib/FC/合集.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/FC/合集.zip")).expect("能穿透");

    assert_eq!(listing.kind, ContainerKind::Zip);
    assert_eq!(listing.contents.entries.len(), 2);
    assert_eq!(listing.contents.entries[0].path, "超级马里奥.nes");
    assert_eq!(listing.contents.entries[0].size, 存储.len() as u64);
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&存储)));
    assert_eq!(listing.contents.entries[1].path, "魂斗罗.nes");
    assert_eq!(listing.contents.entries[1].size, 压缩.len() as u64);
    assert_eq!(listing.contents.entries[1].crc32, Some(crc32(&压缩)));
    assert_eq!(listing.contents.file_count(), 2);
    assert_eq!(
        listing.contents.total_size(),
        (存储.len() + 压缩.len()) as u64
    );
    assert_eq!(listing.contents.without_crc(), 0);
    assert!(
        !listing.contents.is_solid(),
        "zip 的条目各自独立，永远不是 solid"
    );
}

#[test]
fn zip_通用标志位第三位置位时改读中央目录() {
    let 内容 = 样本(3, 1024);
    let bytes = zip_container(&[
        ZipEntrySpec::deflated("流式写出来的.sfc", 内容.clone()).with_data_descriptor()
    ]);

    // 先证明这份样本确实是那个坑：local header 的 CRC 与两个大小都是零。
    // 偏移 14/18/22 是 APPNOTE §4.3.7 的 crc-32 / compressed size / uncompressed size。
    assert_eq!(&bytes[14..26], &[0u8; 12], "样本没造出第 3 位那个坑");
    assert_eq!(bytes[6] & 0x08, 0x08, "通用标志位第 3 位没置上");

    let library = 建库("/lib/SFC/流式.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/SFC/流式.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&内容)));
    assert_eq!(listing.contents.entries[0].size, 内容.len() as u64);

    // 内容照样取得出来——数据起点只用 local header 的两个长度字段，不碰那三个零。
    let (_, got) = 全读(
        &library,
        "/lib/SFC/流式.zip",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got[0].1, 内容);
}

#[test]
fn zip64_哨兵值指向_extra_field_里的真值() {
    let 内容 = 样本(5, 2048);
    // 条目级：中央目录里三个字段都是哨兵，真值在 0x0001 的 extra field 里。
    let bytes =
        zip_container(&[ZipEntrySpec::stored("大镜像.iso", 内容.clone()).with_zip64_extra()]);
    let library = 建库("/lib/PS3/大.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/PS3/大.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries[0].size, 内容.len() as u64);
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&内容)));
    let (_, got) = 全读(
        &library,
        "/lib/PS3/大.zip",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(
        got[0].1, 内容,
        "local header 偏移也得从 extra field 里取回来"
    );

    // 容器级：EOCD 里的条目数、中央目录大小与偏移全是哨兵，真值在 ZIP64 EOCD 里。
    let bytes = zip_container_with_zip64_eocd(&[
        ZipEntrySpec::stored("a.iso", 样本(7, 64)),
        ZipEntrySpec::deflated("b.iso", 内容.clone()),
    ]);
    let library = 建库("/lib/PS3/双.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/PS3/双.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries.len(), 2);
    assert_eq!(listing.contents.entries[1].size, 内容.len() as u64);
    assert_eq!(listing.contents.entries[1].crc32, Some(crc32(&内容)));
}

#[test]
fn zip64_的_extra_field_被写满时按位置取而不是按哨兵顺序取() {
    // 规范说只有哨兵字段才该出现，但确实有工具无条件写满前几个字段。按哨兵顺序读
    // 这种记录会把未压缩大小当成 local header 偏移，然后 seek 到一个错的地方。
    let 内容 = 样本(97, 1234);
    let bytes = zip_container(&[
        ZipEntrySpec::stored("写满了.iso", 内容.clone()).with_zip64_extra_written_in_full()
    ]);
    let library = 建库("/lib/写满.zip", bytes);
    let listing = container::list(&library, std::path::Path::new("/lib/写满.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries[0].size, 内容.len() as u64);
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&内容)));
    let (_, got) = 全读(
        &library,
        "/lib/写满.zip",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got[0].1, 内容, "偏移得从第三个槽位取，不是第一个");
}

#[test]
fn zip_存储与压缩两种条目都取得出内容() {
    let 存储 = 样本(11, 500);
    let 压缩 = 样本(13, 8192);
    let bytes = zip_container(&[
        ZipEntrySpec::stored("原样.gb", 存储.clone()),
        ZipEntrySpec::deflated("压过.gba", 压缩.clone()),
    ]);
    let library = 建库("/lib/合集.zip", bytes);
    let listing = container::list(&library, std::path::Path::new("/lib/合集.zip")).expect("能穿透");
    let (stats, got) = 全读(
        &library,
        "/lib/合集.zip",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got[0].1, 存储);
    assert_eq!(got[1].1, 压缩);
    assert_eq!(stats.entries_read, 2);
    // zip 每个条目自成一块。
    assert_eq!(stats.blocks_decoded, 2);
    assert_eq!(stats.bytes_decompressed, (存储.len() + 压缩.len()) as u64);

    // 而且取出来的字节与容器记的 CRC 对得上——零解压那一层是可信的。
    assert_eq!(crc32(&got[1].1), listing.contents.entries[1].crc32.unwrap());
}

#[test]
fn zip_读够前若干字节就中止() {
    let 内容 = 样本(17, 1 << 16);
    let bytes = zip_container(&[ZipEntrySpec::deflated("大卡带.sfc", 内容.clone())]);
    let library = 建库("/lib/大.zip", bytes);
    let listing = container::list(&library, std::path::Path::new("/lib/大.zip")).expect("能穿透");

    let plan = ReadPlan::prefix(&listing, 0x200);
    let (stats, got) = 全读(&library, "/lib/大.zip", &listing, &plan);
    assert_eq!(got[0].1, 内容[..0x200]);
    assert_eq!(
        stats.bytes_decompressed, 0x200,
        "只该解出要的那一截，代价与文件总大小无关"
    );
    assert!(stats.bytes_decompressed * 100 < 内容.len() as u64);
}

#[test]
fn zip_跳过的条目一个字节都不解() {
    let 甲 = 样本(19, 4096);
    let 乙 = 样本(23, 4096);
    let bytes = zip_container(&[
        ZipEntrySpec::deflated("甲.nes", 甲.clone()),
        ZipEntrySpec::deflated("乙.nes", 乙.clone()),
    ]);
    let library = 建库("/lib/两个.zip", bytes);
    let listing = container::list(&library, std::path::Path::new("/lib/两个.zip")).expect("能穿透");

    let plan = ReadPlan::only(&listing, 1);
    assert_eq!(plan.blocks_needed(&listing), 1);
    let (stats, got) = 全读(&library, "/lib/两个.zip", &listing, &plan);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1, 乙);
    assert_eq!(stats.blocks_decoded, 1);
    assert_eq!(stats.bytes_decompressed, 乙.len() as u64);
}

#[test]
fn zip_自解压形态的偏移整体位移也读得出来() {
    let 内容 = 样本(29, 777);
    let bytes = zip_container_with_prefix(
        &[ZipEntrySpec::stored("藏在里面.nes", 内容.clone())],
        &vec![0xCCu8; 4096],
    );
    let library = 建库("/lib/自解压.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/自解压.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&内容)));
    let (_, got) = 全读(
        &library,
        "/lib/自解压.zip",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got[0].1, 内容);
}

#[test]
fn eocd_后面还挂着东西的_zip_照样读得出来() {
    // 真库里 GB 目录下那批 2002 年的 zip 就是这样：EOCD 之后 3 个填充字节。
    // 只认「EOCD 正好收在文件末尾」的话，它们会被整批判成不是 zip。
    let 内容 = 样本(83, 900);
    let bytes = zip_container_with_trailing(
        &[ZipEntrySpec::deflated("打地鼠.gb", 内容.clone())],
        &[0u8; 3],
    );
    let library = 建库("/lib/GB/尾巴.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/GB/尾巴.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&内容)));
    let (_, got) = 全读(
        &library,
        "/lib/GB/尾巴.zip",
        &listing,
        &ReadPlan::all(&listing),
    );
    assert_eq!(got[0].1, 内容);
}

#[test]
fn 压缩数据里碰巧出现的_eocd_签名不会认错() {
    // 只认签名会被内容骗到。判据必须走到底：顺着候选算出来的中央目录起点上
    // 得真的坐着一条 `PK\x01\x02`。
    let mut 内容 = 样本(89, 4000);
    内容[100..122].copy_from_slice(&[
        b'P', b'K', 5, 6, 0, 0, 0, 0, 1, 0, 1, 0, 0x99, 0, 0, 0, 0x77, 0, 0, 0, 0, 0,
    ]);
    let bytes = zip_container(&[ZipEntrySpec::stored("有假签名.nes", 内容.clone())]);
    let library = 建库("/lib/假签名.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/假签名.zip")).expect("能穿透");
    assert_eq!(listing.contents.entries.len(), 1);
    assert_eq!(listing.contents.entries[0].path, "有假签名.nes");
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&内容)));
}

#[test]
fn 扩展名说是容器内容不是时如实说不是() {
    let library = 建库("/lib/假的.zip", vec![0u8; 512]);
    let error = container::list(&library, std::path::Path::new("/lib/假的.zip")).unwrap_err();
    assert!(
        matches!(error, ContainerError::NotAContainer { .. }),
        "得到的是 {error}"
    );

    let library = 建库("/lib/假的.7z", vec![0u8; 512]);
    let error = container::list(&library, std::path::Path::new("/lib/假的.7z")).unwrap_err();
    assert!(
        matches!(error, ContainerError::NotAContainer { .. }),
        "得到的是 {error}"
    );
}

#[test]
fn rar_与_zst_这票不碰() {
    // 它们在归类上同样是**透明容器**，但 rar 是票 04、zst 是票 26。
    for name in ["/lib/a.rar", "/lib/a.zst"] {
        let library = 建库(name, vec![0u8; 64]);
        let error = container::list(&library, std::path::Path::new(name)).unwrap_err();
        assert!(matches!(error, ContainerError::NotSupportedHere));
    }
}

// ───────────────────────────── 7z ─────────────────────────────

#[test]
fn 七z_不解压就读出内部文件的名字与未压缩大小与_crc32() {
    let 甲 = 样本(2, 1000);
    let 乙 = 样本(4, 2000);
    let bytes = 七z(&[("甲.sfc", 甲.clone()), ("乙.sfc", 乙.clone())], true);
    let library = 建库("/lib/SFC/合集.7z", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/SFC/合集.7z")).expect("能穿透");

    assert_eq!(listing.kind, ContainerKind::SevenZip);
    assert_eq!(listing.contents.entries.len(), 2);
    assert_eq!(listing.contents.entries[0].path, "甲.sfc");
    assert_eq!(listing.contents.entries[0].size, 甲.len() as u64);
    assert_eq!(listing.contents.entries[0].crc32, Some(crc32(&甲)));
    assert_eq!(listing.contents.entries[1].crc32, Some(crc32(&乙)));
    assert_eq!(listing.contents.without_crc(), 0);
}

#[test]
fn solid_的_7z_一块只解一次而不是一个文件解一趟() {
    let 每份 = 4096;
    let 条目: Vec<(&str, Vec<u8>)> = vec![
        ("一.sfc", 样本(31, 每份)),
        ("二.sfc", 样本(37, 每份)),
        ("三.sfc", 样本(41, 每份)),
        ("四.sfc", 样本(43, 每份)),
    ];
    let bytes = 七z(&条目, true);
    let library = 建库("/lib/SFC/solid.7z", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/SFC/solid.7z")).expect("能穿透");

    assert!(listing.contents.is_solid(), "四个条目该压在同一个块里");
    assert_eq!(listing.contents.blocks, 1);
    assert_eq!(listing.contents.entries_in_block(0), 4);

    // 按块调度：一趟走完，解压量等于块的总大小。
    let plan = ReadPlan::all(&listing);
    assert_eq!(plan.blocks_needed(&listing), 1);
    let (stats, got) = 全读(&library, "/lib/SFC/solid.7z", &listing, &plan);
    assert_eq!(stats.blocks_decoded, 1, "一个块只该解一次");
    assert_eq!(stats.entries_read, 4);
    assert_eq!(stats.bytes_decompressed, (每份 * 4) as u64);
    for (index, (_, 原文)) in 条目.iter().enumerate() {
        assert_eq!(&got[index].1, 原文);
        assert_eq!(
            crc32(&got[index].1),
            listing.contents.entries[index].crc32.unwrap()
        );
    }

    // 朴素做法作对照：一个文件一趟，块被反复解开，解压量成倍放大。
    let mut 朴素 = ReadStats::default();
    for index in 0..listing.contents.entries.len() {
        let (one, _) = 全读(
            &library,
            "/lib/SFC/solid.7z",
            &listing,
            &ReadPlan::only(&listing, index),
        );
        朴素.blocks_decoded += one.blocks_decoded;
        朴素.bytes_decompressed += one.bytes_decompressed;
    }
    assert_eq!(朴素.blocks_decoded, 4, "朴素做法把同一个块解了四次");
    assert!(
        朴素.bytes_decompressed > stats.bytes_decompressed * 2,
        "朴素做法解了 {} 字节，按块调度只解了 {}",
        朴素.bytes_decompressed,
        stats.bytes_decompressed
    );
}

#[test]
fn 非_solid_的_7z_每个文件自成一块() {
    let 条目: Vec<(&str, Vec<u8>)> = vec![
        ("一.gba", 样本(47, 3000)),
        ("二.gba", 样本(53, 3000)),
        ("三.gba", 样本(59, 3000)),
    ];
    let bytes = 七z(&条目, false);
    let library = 建库("/lib/GBA/散.7z", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/GBA/散.7z")).expect("能穿透");

    assert!(!listing.contents.is_solid(), "每个文件自成一块就不是 solid");
    assert_eq!(listing.contents.blocks, 3);
    for block in 0..3 {
        assert_eq!(listing.contents.entries_in_block(block), 1);
    }

    // 非 solid 时取单个文件只解它自己那一块。
    let plan = ReadPlan::only(&listing, 2);
    assert_eq!(plan.blocks_needed(&listing), 1);
    let (stats, got) = 全读(&library, "/lib/GBA/散.7z", &listing, &plan);
    assert_eq!(stats.blocks_decoded, 1);
    assert_eq!(stats.bytes_decompressed, 3000);
    assert_eq!(got[0].1, 条目[2].1);
}

#[test]
fn solid_块里最后一个想要的条目读完就中止() {
    let 每份 = 8192;
    let 条目: Vec<(&str, Vec<u8>)> = (0..4)
        .map(|i| 样本(61 + i as u8, 每份))
        .enumerate()
        .map(|(i, data)| (["一.sfc", "二.sfc", "三.sfc", "四.sfc"][i], data))
        .collect();
    let bytes = 七z(&条目, true);
    let library = 建库("/lib/SFC/前面.7z", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/SFC/前面.7z")).expect("能穿透");
    assert!(listing.contents.is_solid());

    // 只要第一个条目的前 0x200 字节：解到那儿就停，后面三个条目一个字节都不解。
    let plan = ReadPlan::new(&listing, |entry| {
        if entry.path == "一.sfc" {
            Demand::Prefix(0x200)
        } else {
            Demand::Skip
        }
    });
    let (stats, got) = 全读(&library, "/lib/SFC/前面.7z", &listing, &plan);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1, 条目[0].1[..0x200]);
    assert_eq!(stats.blocks_decoded, 1);
    assert_eq!(stats.bytes_decompressed, 0x200);

    // 只要最后一个条目：格式使然，前面三份必须顺着流过去，但整块仍然只解一次，
    // 且末尾之后的字节不再解。
    let plan = ReadPlan::only(&listing, 3);
    let (stats, got) = 全读(&library, "/lib/SFC/前面.7z", &listing, &plan);
    assert_eq!(got[0].1, 条目[3].1);
    assert_eq!(stats.blocks_decoded, 1);
    assert_eq!(stats.bytes_decompressed, (每份 * 4) as u64);
}

#[test]
fn 一趟流同时喂多个_hasher() {
    // 「一块一趟流式喂多个 hasher」的形状：拿到的是流不是缓冲，
    // 一边读一边算，整块内容不必驻留内存。
    let 条目: Vec<(&str, Vec<u8>)> = vec![
        ("一.md", 样本(67, 5000)),
        ("二.md", 样本(71, 5000)),
        ("三.md", 样本(73, 5000)),
    ];
    let bytes = 七z(&条目, true);
    let library = 建库("/lib/MD/合集.7z", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/MD/合集.7z")).expect("能穿透");

    let mut 结果: Vec<(String, u32, u64, u8)> = Vec::new();
    let stats = container::read_entries(
        &library,
        std::path::Path::new("/lib/MD/合集.7z"),
        &listing,
        &ReadPlan::all(&listing),
        &mut |entry, reader| {
            let mut crc = flate2::Crc::new();
            let mut 字节数 = 0u64;
            let mut 异或 = 0u8;
            let mut buf = [0u8; 512];
            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                crc.update(&buf[..n]);
                字节数 += n as u64;
                for byte in &buf[..n] {
                    异或 ^= *byte;
                }
            }
            结果.push((entry.path.clone(), crc.sum(), 字节数, 异或));
            Ok(())
        },
    )
    .expect("读得出来");

    assert_eq!(stats.blocks_decoded, 1, "三个文件的三套哈希在同一趟里算完");
    assert_eq!(结果.len(), 3);
    for (index, (name, data)) in 条目.iter().enumerate() {
        assert_eq!(结果[index].0, *name);
        // 流式算出来的 CRC-32 与容器头部零解压读到的那个是同一个数。
        assert_eq!(结果[index].1, crc32(data));
        assert_eq!(
            结果[index].1,
            listing.contents.entries[index].crc32.unwrap()
        );
        assert_eq!(结果[index].2, data.len() as u64);
    }
}

#[test]
fn 目录条目与空文件不占块也不必碰盘() {
    let bytes = zip_container(&[
        ZipEntrySpec::stored("子目录/", Vec::new()),
        ZipEntrySpec::stored("子目录/空.txt", Vec::new()),
        ZipEntrySpec::stored("子目录/有内容.nes", 样本(79, 128)),
    ]);
    let library = 建库("/lib/带目录.zip", bytes);
    let listing =
        container::list(&library, std::path::Path::new("/lib/带目录.zip")).expect("能穿透");

    assert!(listing.contents.entries[0].is_dir);
    assert_eq!(listing.contents.entries[0].block, None);
    assert!(!listing.contents.entries[1].is_dir);
    assert_eq!(listing.contents.entries[1].block, None, "空文件没有数据流");
    assert_eq!(
        listing.contents.entries[2].block,
        Some(0),
        "块号只发给真有数据的条目，不是照条目序号发"
    );
    assert_eq!(listing.contents.file_count(), 2, "目录条目不算文件");
    assert_eq!(
        listing.contents.blocks, 1,
        "三个条目里只有一个真占着块，别把目录条目也数成块"
    );

    let plan = ReadPlan::all(&listing);
    assert_eq!(plan.blocks_needed(&listing), 1);
}

// ────────────────────── 穿透进了库体检报告 ──────────────────────

fn 扫一遍(
    library: &MemFs,
    options: &romcat_core::scan::ScanOptions,
) -> romcat_core::catalog::Catalog {
    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    romcat_core::scan::scan(
        library,
        &mut catalog,
        options,
        &romcat_core::scan::CancelToken::new(),
    )
    .expect("扫描不该失败");
    catalog
}

fn 建三个容器的库() -> (MemFs, u64) {
    let 甲 = 样本(101, 600);
    let 乙 = 样本(103, 1500);
    let 丙 = 样本(107, 2000);
    let 丁 = 样本(109, 2500);
    let 戊 = 样本(113, 3000);
    let 合计 = (甲.len() + 乙.len() + 丙.len() + 丁.len() + 戊.len()) as u64;

    let mut library = MemFs::new();
    library
        .dir("/lib")
        .file(
            "/lib/FC/合集.zip",
            zip_container(&[
                ZipEntrySpec::stored("超级马里奥.nes", 甲),
                ZipEntrySpec::deflated("魂斗罗.nes", 乙),
            ]),
        )
        .file(
            "/lib/SFC/solid.7z",
            七z(&[("一.sfc", 丙), ("二.sfc", 丁), ("三.sfc", 戊)], true),
        )
        // 扩展名说是 zip，内容不是——穿不透，但绝不该中断扫描。
        .file("/lib/FC/坏的.zip", vec![0u8; 512]);
    (library, 合计)
}

#[test]
fn 体检报告说得出容器内部的文件数与构成() {
    let (library, 内部合计) = 建三个容器的库();
    let options = romcat_core::scan::ScanOptions::new("/lib");
    let catalog = 扫一遍(&library, &options);

    let aggregate = catalog.aggregate(&Default::default()).expect("能折出统计");
    let meta = catalog.report_meta().expect("能取元信息");
    let report = romcat_core::report::HealthReport::build(&aggregate, &meta);
    let containers = &report.containers;

    assert!(containers.penetrated_this_scan);
    assert_eq!(containers.containers, 3, "三个 zip/7z 都该被数到");
    assert_eq!(containers.penetrated, 2);
    assert_eq!(containers.failed, 1);
    assert_eq!(containers.inner_files, 5, "两个 nes 加三个 sfc");
    assert_eq!(containers.inner_bytes, 内部合计);
    assert_eq!(
        containers.inner_with_crc, 5,
        "五个内部文件全都零解压拿得到 CRC-32"
    );
    assert_eq!(containers.inner_without_crc, 0);
    assert_eq!(containers.solid, 1, "只有那个 7z 是 solid");

    // 内部构成：按扩展名与按三类主线都说得出来。
    let 按扩展名: Vec<(&str, u64)> = containers
        .inner_extensions
        .iter()
        .map(|e| (e.extension.as_str(), e.files))
        .collect();
    assert!(按扩展名.contains(&("sfc", 3)), "{按扩展名:?}");
    assert!(按扩展名.contains(&("nes", 2)), "{按扩展名:?}");
    let 裸文件 = containers
        .inner_categories
        .iter()
        .find(|c| c.category == romcat_core::classify::Category::BareFile)
        .expect("有裸文件这一类");
    assert_eq!(裸文件.files, 5);

    // 穿不透的那个如实报出来，带原因，而且**分了类**——「不是这个格式」与
    // 「需要密码」的后续处置完全不同，合成一个数就没法照着它动手。
    assert_eq!(containers.failures.len(), 1);
    assert!(containers.failures[0].0.ends_with("坏的.zip"));
    assert_eq!(containers.failures_by_reason.len(), 1);
    assert_eq!(
        containers.failures_by_reason[0].reason,
        romcat_core::container::FailureReason::WrongFormat
    );
    assert_eq!(containers.failures_by_reason[0].containers, 1);

    let text = report.render_text();
    assert!(text.contains("透明容器穿透"), "{text}");
    assert!(text.contains("零解压就能拿去撞 DAT 的那一批"), "{text}");
}

#[test]
fn 盘不在位时报告照样说得出容器里装着什么() {
    let (library, 内部合计) = 建三个容器的库();
    let options = romcat_core::scan::ScanOptions::new("/lib");
    let catalog = 扫一遍(&library, &options);

    // 这一步一个字节都不碰主库：结论全在中立库里（ADR-0001）。
    let aggregate = catalog.aggregate(&Default::default()).expect("能折出统计");
    let report =
        romcat_core::report::HealthReport::build(&aggregate, &catalog.report_meta().unwrap());
    assert_eq!(report.containers.inner_files, 5);
    assert_eq!(report.containers.inner_bytes, 内部合计);
}

#[test]
fn 关掉穿透就一个容器都不去读() {
    let (library, _) = 建三个容器的库();
    let mut options = romcat_core::scan::ScanOptions::new("/lib");
    options.penetrate_containers = false;
    let catalog = 扫一遍(&library, &options);

    let aggregate = catalog.aggregate(&Default::default()).expect("能折出统计");
    let report =
        romcat_core::report::HealthReport::build(&aggregate, &catalog.report_meta().unwrap());
    assert_eq!(report.containers.containers, 0);
    assert_eq!(report.containers.inner_files, 0);
    assert!(
        !report.containers.penetrated_this_scan,
        "报告要说得出这次没去穿透，否则「0 个内部文件」会被读成「容器是空的」"
    );
    assert!(
        report.render_text().contains("--no-containers"),
        "文本报告要写明这次没穿透"
    );
}

#[test]
fn 容器变了内部构成跟着换掉而不是叠加() {
    let mut library = MemFs::new();
    library.dir("/lib").file(
        "/lib/FC/合集.zip",
        zip_container(&[
            ZipEntrySpec::stored("旧的甲.nes", 样本(131, 100)),
            ZipEntrySpec::stored("旧的乙.nes", 样本(137, 100)),
        ]),
    );
    let options = romcat_core::scan::ScanOptions::new("/lib");
    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    let cancel = romcat_core::scan::CancelToken::new();
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("首扫");
    let 首扫 = catalog.aggregate(&Default::default()).unwrap();
    assert_eq!(首扫.containers.totals().inner_files, 2);

    // 换成一个只装一个文件的 zip，并把修改时间往后拨。
    library.file(
        "/lib/FC/合集.zip",
        zip_container(&[ZipEntrySpec::stored("新的.nes", 样本(139, 250))]),
    );
    library.touch("/lib/FC/合集.zip", 60);
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("二扫");

    let 二扫 = catalog.aggregate(&Default::default()).unwrap();
    assert_eq!(
        二扫.containers.totals().inner_files,
        1,
        "旧的两条内部条目必须消失，不能与新的叠在一起"
    );
    assert_eq!(二扫.containers.totals().inner_bytes, 250);
    let 名字: Vec<&String> = 二扫.containers.inner_extensions.keys().collect();
    assert_eq!(名字, vec!["nes"]);
}

#[test]
fn 容器被删掉后内部构成一起消失() {
    let (mut library, _) = 建三个容器的库();
    let options = romcat_core::scan::ScanOptions::new("/lib");
    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    let cancel = romcat_core::scan::CancelToken::new();
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("首扫");
    assert_eq!(
        catalog
            .aggregate(&Default::default())
            .unwrap()
            .containers
            .totals()
            .inner_files,
        5
    );

    library.remove("/lib/SFC/solid.7z");
    romcat_core::scan::scan(&library, &mut catalog, &options, &cancel).expect("二扫");
    let 二扫 = catalog.aggregate(&Default::default()).unwrap();
    assert_eq!(二扫.containers.totals().containers, 2);
    assert_eq!(
        二扫.containers.totals().inner_files,
        2,
        "7z 没了，它那三个内部文件不该还留在报告里"
    );
}

#[test]
fn 上次带着_no_containers_扫过的容器下次会补穿() {
    let (library, 内部合计) = 建三个容器的库();
    let mut catalog = romcat_core::catalog::Catalog::open_in_memory().expect("能开中立库");
    let cancel = romcat_core::scan::CancelToken::new();

    let mut 不穿 = romcat_core::scan::ScanOptions::new("/lib");
    不穿.penetrate_containers = false;
    romcat_core::scan::scan(&library, &mut catalog, &不穿, &cancel).expect("首扫");
    assert_eq!(
        catalog
            .aggregate(&Default::default())
            .unwrap()
            .containers
            .totals()
            .containers,
        0
    );

    // 第二趟打开穿透。三元组一个都没变，但中立库里压根没有穿透结论——
    // 只看三元组的话这批容器会永远不被穿透。
    let 穿 = romcat_core::scan::ScanOptions::new("/lib");
    romcat_core::scan::scan(&library, &mut catalog, &穿, &cancel).expect("二扫");
    let totals = catalog
        .aggregate(&Default::default())
        .unwrap()
        .containers
        .totals();
    assert_eq!(totals.containers, 3);
    assert_eq!(totals.inner_files, 5);
    assert_eq!(totals.inner_bytes, 内部合计);
}
